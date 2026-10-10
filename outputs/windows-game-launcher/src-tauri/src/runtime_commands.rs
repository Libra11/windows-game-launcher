use crate::{activity, db, lock_db, runtime, AppState};
use tauri::{Emitter, Manager};

#[tauri::command]
pub(crate) async fn launch_game(
    app: tauri::AppHandle,
    game_id: String,
) -> Result<runtime::RuntimeInfo, String> {
    tauri::async_runtime::spawn_blocking(move || launch(&app, &game_id))
        .await
        .map_err(|e| e.to_string())?
}
fn launch(app: &tauri::AppHandle, game_id: &str) -> Result<runtime::RuntimeInfo, String> {
    let state = app.state::<AppState>();
    let _gate=crate::maintenance::lock_operation(&state.operation_gate,&state.maintenance)?;
    let game = db::game(&*lock_db(&state)?, game_id)?.ok_or("游戏不存在")?;
    crate::steam_family::require_available(&game)?;
    if matches!(
        state
            .runtime
            .lock()
            .map_err(|_| "运行状态暂时不可用")?
            .info(game_id)
            .state
            .as_str(),
        "starting" | "running"
    ) {
        return Err("游戏已在启动或运行中，请勿重复启动".into());
    }
    let installation = if game.source == "steam" {
        let installation = state.installation.check_before_launch(&game)?;
        Some(installation)
    } else {
        None
    };
    let reserved = {
        let mut tracker = state.runtime.lock().map_err(|_| "运行状态暂时不可用")?;
        if db::game(&*lock_db(&state)?, game_id)?.is_none() {
            return Err("游戏已从游戏库移除".into());
        }
        reserve_launch(&mut tracker, &game, installation.as_ref())
    };
    let _ = app.emit("library-changed", &game_id);
    let should_spawn = reserved?;
    if !should_spawn {
        return Ok(state
            .runtime
            .lock()
            .map_err(|_| "运行状态暂时不可用")?
            .info(game_id));
    }
    match runtime::spawn(&game) {
        Ok(pid) => {
            let mut tracker = state.runtime.lock().map_err(|_| "运行状态暂时不可用")?;
            tracker.attach(&game_id, pid);
            Ok(tracker.info(&game_id))
        }
        Err(error) => {
            state
                .runtime
                .lock()
                .map_err(|_| "运行状态暂时不可用")?
                .fail(&game_id, &error);
            let _ = app.emit("library-changed", &game_id);
            Err(error)
        }
    }
}

fn reserve_launch(
    tracker: &mut runtime::Tracker,
    game: &crate::model::Game,
    installation: Option<&crate::installation::Info>,
) -> Result<bool, String> {
    if game.source == "steam" {
        installation
            .ok_or("尚未确认 Steam 游戏安装状态")?
            .require_installed()?;
    }
    tracker.reserve(game)
}

#[tauri::command]
pub(crate) fn set_favorite(
    app: tauri::AppHandle,
    game_id: String,
    favorite: bool,
) -> Result<(), String> {
    activity::set_favorite(&*lock_db(&app.state::<AppState>())?, &game_id, favorite)?;
    let _ = app.emit("library-changed", &game_id);
    Ok(())
}

#[tauri::command]
pub(crate) fn set_launch_path(
    state: tauri::State<'_, AppState>,
    game_id: String,
    exe_path: String,
) -> Result<(), String> {
    let conn = lock_db(&state)?;
    let mut game = db::game(&conn, &game_id)?.ok_or("游戏不存在")?;
    if (exe_path.is_empty() && game.source == "local")
        || (!exe_path.is_empty() && !std::path::Path::new(&exe_path).is_file())
    {
        return Err("请选择存在的游戏启动程序".into());
    }
    if game.source == "local" {
        game.launch_uri.clear();
    }
    game.exe_path = exe_path;
    db::upsert_game(&conn, &game)
}

pub(crate) fn watch(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        let mut auto_minimized = false;
        let mut save_error_reported = false;
        loop {
            let state = app.state::<AppState>();
            if state.maintenance.load(std::sync::atomic::Ordering::Acquire){std::thread::sleep(std::time::Duration::from_millis(250));continue;}
            let updates = match crate::runtime_persistence::checkpoint(&app, false) {
                Ok(updates) => {
                    save_error_reported = false;
                    updates
                }
                Err(error) => {
                    if !save_error_reported {
                        let _ = app.emit("launcher-error", &error);
                    }
                    save_error_reported = true;
                    eprintln!("{error}");
                    Vec::new()
                }
            };
            for update in updates {
                if update.finished && !update.failed {
                    let is_steam = lock_db(&state)
                        .and_then(|conn| db::game(&conn, &update.game_id))
                        .ok()
                        .flatten()
                        .is_some_and(|game| game.source == "steam");
                    if is_steam {
                        crate::steam_playtime::start_refresh(app.clone());
                    }
                }
                let saved = (|| -> Result<bool, String> {
                    let conn = lock_db(&state)?;
                    Ok(update.started_at.is_some()
                        && db::setting(&conn, "minimize_on_launch")? == "true")
                })();
                match saved {
                    Ok(true) => {
                        if let Some(window) = app.get_webview_window("main") {
                            auto_minimized |= window.minimize().is_ok();
                        }
                    }
                    Err(error) => eprintln!("游玩记录保存失败：{error}"),
                    _ => {}
                }
                let _ = app.emit("library-changed", &update.game_id);
                if update.finished {
                    let local = lock_db(&state)
                        .and_then(|conn| db::game(&conn, &update.game_id))
                        .ok()
                        .flatten()
                        .is_some_and(|game| game.source == "local");
                    if local {
                        let _ = crate::scanner::scan_now(app.clone(), update.game_id.clone());
                    }
                    if auto_minimized
                        && state
                            .runtime
                            .lock()
                            .map(|tracker| tracker.is_empty())
                            .unwrap_or(false)
                    {
                        if let Some(window) = app.get_webview_window("main") {
                            if window.is_minimized().unwrap_or(false) {
                                let _ = window.unminimize();
                                let _ = window.set_focus();
                            }
                        }
                        auto_minimized = false;
                    }
                }
            }
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn installation_rejection_does_not_reserve_or_start_a_process() {
        let game = crate::model::Game {
            id: "steam-480".into(),
            source: "steam".into(),
            appid: "480".into(),
            launch_uri: "steam://rungameid/480".into(),
            ..Default::default()
        };
        let mut tracker = runtime::Tracker::default();
        for status in [
            crate::installation::Status::NotInstalled,
            crate::installation::Status::Unknown,
            crate::installation::Status::ClientMissing,
            crate::installation::Status::Checking,
        ] {
            let installation = crate::installation::Info {
                state: status,
                reason: "不可启动".into(),
                checked_at: String::new(),
            };
            assert!(reserve_launch(&mut tracker, &game, Some(&installation)).is_err());
            assert!(tracker.is_empty());
            assert_eq!(tracker.info(&game.id).state, "idle");
        }
        assert!(reserve_launch(&mut tracker, &game, None).is_err());
    }
}
