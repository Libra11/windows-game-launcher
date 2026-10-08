use crate::{
    db, grime, lock_db,
    model::{Game, UnlockEvent},
    record_paths, runtime_environment, runtime_record, source, AppState,
};
use tauri::{Emitter, Manager};

pub(crate) fn scan_one(
    state: &AppState,
    game: &Game,
    notify: bool,
) -> Result<Vec<UnlockEvent>, String> {
    let current = db::game(&*lock_db(state)?, &game.id)?.ok_or("游戏已从游戏库移除")?;
    if current.appid != game.appid
        || current.exe_path != game.exe_path
        || current.custom_unlock_path != game.custom_unlock_path
    {
        return Err("游戏设置已变更，等待下一次扫描".into());
    }
    if game.schema_source.is_empty() || game.schema_source == "本地记录（仅已解锁）" {
        if let Some(schema_file) = source::local_schema_path(game) {
            if let Ok(text) = std::fs::read_to_string(&schema_file) {
                if let Ok(items) = source::parse_local_schema(&text, &schema_file) {
                    if !items.is_empty() {
                        db::save_definitions(
                            &mut *lock_db(state)?,
                            &game.id,
                            &game.appid,
                            &items,
                            "本地定义文件",
                        )?;
                    }
                }
            }
        }
    }
    let profile = crate::achievement_platform::for_game(&*lock_db(state)?, game)?;
    let runtime_file = record_paths::runtime_file(
        game,
        &profile,
        state.appdata.as_deref(),
        state.public.as_deref(),
    );
    let (unlocks, file_name, status) = if let Some(file) = runtime_file {
        let file_name = file.to_string_lossy().into_owned();
        let definitions = db::achievement_api_names(&*lock_db(state)?, &game.appid)?;
        let require_match = !profile.steam_appid.is_empty() && profile.steam_appid != game.appid;
        let items = match runtime_record::read(&file, &definitions, require_match) {
            Ok(items) => items,
            Err(error) => {
                db::update_scan(&*lock_db(state)?, &game.id, &error, &file_name)?;
                return Ok(Vec::new());
            }
        };
        (items, file_name, "已读取本地记录")
    } else if game.appid == grime::APPID {
        let definitions = db::achievement_api_names(&*lock_db(state)?, &game.appid)?;
        match grime::read_saves(state.appdata.as_deref(), &definitions) {
            Ok(Some((items, folder))) => (
                items,
                folder.to_string_lossy().into_owned(),
                "已读取 GRIME 存档",
            ),
            Ok(None) => {
                db::update_scan(
                    &*lock_db(state)?,
                    &game.id,
                    "未找到解锁记录文件或 GRIME 存档",
                    "",
                )?;
                return Ok(Vec::new());
            }
            Err(error) => {
                db::update_scan(
                    &*lock_db(state)?,
                    &game.id,
                    &format!("GRIME 存档读取失败：{error}"),
                    "",
                )?;
                return Ok(Vec::new());
            }
        }
    } else {
        let status = if profile.record_conflicts.is_empty() {
            runtime_environment::missing_record_status(game)
        } else {
            format!(
                "本地配置编号 {} 与《{}》共用，已停止从共用编号自动读取成就；请修正游戏配置或指定本游戏的独立记录文件",
                profile.steam_appid, profile.record_conflicts.join("》《")
            )
        };
        db::update_scan(
            &*lock_db(state)?,
            &game.id,
            &status,
            "",
        )?;
        return Ok(Vec::new());
    };
    let conn = lock_db(state)?;
    let mut events = Vec::new();
    if !unlocks.is_empty() {
        db::mark_runtime_only_schema(&conn, &game.id)?;
    }
    for unlock in unlocks {
        db::ensure_definition(&conn, &game.appid, &unlock.api_name)?;
        let now = chrono::Utc::now().to_rfc3339();
        let added = db::add_unlock(
            &conn,
            &game.id,
            &unlock.api_name,
            "local",
            unlock.unlocked_at.as_deref().unwrap_or(&now),
            &file_name,
        )?;
        if added && notify {
            let name: String = conn
                .query_row(
                    "SELECT name FROM achievements WHERE appid=?1 AND api_name=?2",
                    rusqlite::params![game.appid, unlock.api_name],
                    |row| row.get(0),
                )
                .unwrap_or_else(|_| unlock.api_name.clone());
            events.push(UnlockEvent {
                game_id: game.id.clone(),
                game_title: game.title.clone(),
                api_name: unlock.api_name,
                achievement_name: name,
            });
        }
    }
    db::update_scan(&conn, &game.id, status, &file_name)?;
    Ok(events)
}

fn notify_unlocks(app: &tauri::AppHandle, events: Vec<UnlockEvent>) {
    crate::achievement_notifications::notify(app, events);
}

pub(crate) fn scan_all(app: &tauri::AppHandle) {
    let state = app.state::<AppState>();
    let games = match lock_db(&state).and_then(|conn| db::games(&conn)) {
        Ok(games) => games,
        Err(_) => return,
    };
    let xbox_games = lock_db(&state)
        .map(|conn| {
            games
                .iter()
                .filter(|g| {
                    crate::achievement_platform::for_game(&conn, g)
                        .is_ok_and(|p| p.platform == "xbox")
                })
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    crate::xbox_local::retain(&xbox_games);
    for game in games.into_iter().filter(|game| game.source == "local") {
        // 串行处理初次扫描，避免向导和后台扫描同时导入历史记录时误通知。
        if let Ok(mut initialized) = state.initialized.lock() {
            let first = !initialized.contains(&game.id);
            let result = scan_platform(app, &state, &game, !first);
            if let Ok(events) = result {
                initialized.insert(game.id.clone());
                drop(initialized);
                let changed = first
                    || lock_db(&state)
                        .and_then(|conn| db::game(&conn, &game.id))
                        .is_ok_and(|updated| {
                            updated.is_some_and(|g| {
                                g.scan_status != game.scan_status
                                    || g.source_file != game.source_file
                            })
                        });
                if changed {
                    let _ = app.emit("library-changed", ());
                }
                notify_unlocks(app, events);
            } else if let Err(error) = result {
                let status = format!(
                    "Xbox 捕获失败：{}",
                    error.trim_start_matches("Xbox 捕获失败：")
                );
                if let Ok(conn) = lock_db(&state) {
                    let _ = db::update_scan(&conn, &game.id, &status, &game.source_file);
                }
                if crate::xbox_local::supported(&game) && game.scan_status != status {
                    crate::achievement_notifications::monitor_failed(app, &game.title, &status);
                    let _ = app.emit("library-changed", ());
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "scanner_tests.rs"]
mod tests;

#[tauri::command]
pub(crate) fn scan_now(app: tauri::AppHandle, game_id: String) -> Result<(), String> {
    let state = app.state::<AppState>();
    let game = db::game(&*lock_db(&state)?, &game_id)?.ok_or("游戏不存在")?;
    if game.source != "local" {
        return Err("只有本地游戏可以扫描本地记录".into());
    }
    let mut initialized = state.initialized.lock().map_err(|_| "扫描状态不可用")?;
    let first = !initialized.contains(&game.id);
    let events = scan_platform(&app, &state, &game, !first)?;
    initialized.insert(game.id.clone());
    drop(initialized);
    notify_unlocks(&app, events);
    Ok(())
}

fn scan_platform(
    app: &tauri::AppHandle,
    state: &AppState,
    game: &Game,
    notify: bool,
) -> Result<Vec<UnlockEvent>, String> {
    let profile = crate::achievement_platform::for_game(&*lock_db(state)?, game)?;
    match profile.platform.as_str() {
        "xbox" if crate::xbox_local::supported(game) => {
            crate::xbox_local::scan(app, state, game, notify)
        }
        "steam" => scan_one(state, game, notify),
        "xbox" => {
            db::update_scan(
                &*lock_db(state)?,
                &game.id,
                "当前无法自动检测：Xbox 成就定义可以获取，但此游戏版本尚无受支持的本地事件捕获",
                " ",
            )?;
            Ok(Vec::new())
        }
        _ => {
            db::update_scan(
                &*lock_db(state)?,
                &game.id,
                "成就平台尚未确认或未关联；未启动自动解锁检测",
                "",
            )?;
            Ok(Vec::new())
        }
    }
}
