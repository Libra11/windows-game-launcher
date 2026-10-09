use crate::{
    activity, db, lock_db,
    model::{Achievement, Game},
    steam, steam_playtime, AppState,
};
use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Duration;
use tauri::Manager;

#[tauri::command]
pub(crate) fn list_games(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<activity::LibraryGame>, String> {
    let (games, mut activities, steam_time, connected) = {
        let conn = lock_db(&state)?;
        let steamid = db::setting(&conn, "steam_id")?;
        let connected = !steamid.is_empty() && !db::setting(&conn, "steam_api_key")?.is_empty();
        (
            db::games(&conn)?,
            activity::all(&conn)?,
            steam_playtime::read(&conn, &steamid)?,
            connected,
        )
    };
    let tracker = state.runtime.lock().map_err(|_| "运行状态暂时不可用")?;
    Ok(games
        .into_iter()
        .map(|game| {
            let activity = activities.remove(&game.id).unwrap_or_default();
            let runtime = tracker.info(&game.id);
            let installation = Some(if game.source == "steam" {
                state.installation.info(&game.id)
            } else if game.source == "epic" {
                crate::installation::Info {
                    state: crate::installation::Status::Unknown,
                    reason: "账号游戏库不包含本机安装状态，请在 Epic 客户端安装或启动".into(),
                    checked_at: String::new(),
                }
            } else {
                let path = PathBuf::from(&game.exe_path);
                let (status, reason) = match path.try_exists() {
                    Ok(true) if path.is_file() => (
                        crate::installation::Status::Installed,
                        "本地启动文件可用".to_string(),
                    ),
                    Ok(_) => (
                        crate::installation::Status::NotInstalled,
                        "本地启动文件不存在，请重新选择".to_string(),
                    ),
                    Err(error) => (
                        crate::installation::Status::Unknown,
                        format!("无法访问本地启动文件：{error}"),
                    ),
                };
                crate::installation::Info {
                    state: status,
                    reason,
                    checked_at: chrono::Utc::now().to_rfc3339(),
                }
            });
            let playtime = steam_playtime::info(
                &game,
                activity
                    .played_seconds
                    .saturating_add(tracker.uncredited_seconds(&game.id)),
                &steam_time,
                connected,
            );
            let achievement_platform = lock_db(&state)
                .and_then(|conn| crate::achievement_platform::for_game(&conn, &game))
                .unwrap_or_default();
            activity::LibraryGame {
                game,
                activity,
                runtime,
                installation,
                playtime,
                achievement_platform,
            }
        })
        .collect())
}

#[tauri::command]
pub(crate) fn list_achievements(
    state: tauri::State<'_, AppState>,
    game_id: String,
) -> Result<Vec<Achievement>, String> {
    db::achievements(&*lock_db(&state)?, &game_id)
}

#[tauri::command]
pub(crate) fn import_local(
    state: tauri::State<'_, AppState>,
    exe_path: String,
    appid: String,
    title: String,
    achievement_platform: String,
    public_achievement_url: String,
    steam_identity_confirmed: bool,
) -> Result<Game, String> {
    let path = PathBuf::from(&exe_path);
    if !path.is_file() {
        return Err("请选择存在的游戏可执行文件".into());
    }
    if !appid.is_empty() && !appid.chars().all(|ch| ch.is_ascii_digit()) {
        return Err("Steam AppID 必须是数字".into());
    }
    let title = title.trim();
    if title.is_empty() {
        return Err("请输入游戏名称".into());
    }
    let game = Game {
        id: format!("local-{}", uuid::Uuid::new_v4()),
        source: "local".into(),
        appid,
        title: title.to_owned(),
        exe_path,
        launch_uri: String::new(),
        custom_unlock_path: String::new(),
        metadata_json: "{}".into(),
        scan_status: String::new(),
        source_file: String::new(),
        last_scan: String::new(),
        schema_source: String::new(),
    };
    let mut conn = lock_db(&state)?;
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    if db::games(&tx)?.iter().any(|existing| {
        existing.source == "local"
            && crate::local_import::same_program(&existing.exe_path, &game.exe_path)
    }) {
        return Err("此游戏程序已在游戏库中，请编辑已有条目".into());
    }
    crate::achievement_platform::save(
        &tx,
        &game,
        &achievement_platform,
        &public_achievement_url,
        steam_identity_confirmed,
    )?;
    db::upsert_game(&tx, &game)?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(game)
}

#[tauri::command]
pub(crate) fn update_local(
    state: tauri::State<'_, AppState>,
    game_id: String,
    title: String,
    appid: String,
    custom_unlock_path: String,
    exe_path: String,
    achievement_platform: String,
    public_achievement_url: String,
    steam_identity_confirmed: bool,
) -> Result<(), String> {
    if !appid.is_empty() && !appid.chars().all(|ch| ch.is_ascii_digit()) {
        return Err("Steam AppID 必须是数字".into());
    }
    if title.trim().is_empty() {
        return Err("游戏名称不能为空".into());
    }
    let mut initialized = state.initialized.lock().map_err(|_| "扫描状态不可用")?;
    let tracker = state.runtime.lock().map_err(|_| "运行状态暂时不可用")?;
    if matches!(
        tracker.info(&game_id).state.as_str(),
        "starting" | "running"
    ) {
        return Err("请退出游戏后再编辑启动程序或 Steam 关联".into());
    }
    let mut conn = lock_db(&state)?;
    let tx = conn.transaction().map_err(|error| error.to_string())?;
    let mut game = db::game(&tx, &game_id)?.ok_or("游戏不存在")?;
    if game.source != "local" {
        return Err("只能编辑本地游戏".into());
    }
    let previous_namespace = crate::achievement_platform::namespace(&tx, &game)?;
    let changed = game.appid != appid;
    game.title = title.trim().to_owned();
    game.appid = appid;
    game.custom_unlock_path = custom_unlock_path.trim().to_owned();
    if !PathBuf::from(&exe_path).is_file() {
        return Err("游戏启动文件不存在".into());
    }
    game.launch_uri.clear();
    game.exe_path = exe_path;
    if changed {
        game.metadata_json = "{}".into();
    }
    crate::achievement_platform::save(
        &tx,
        &game,
        &achievement_platform,
        &public_achievement_url,
        steam_identity_confirmed,
    )?;
    let platform_changed =
        previous_namespace != crate::achievement_platform::namespace(&tx, &game)?;
    db::upsert_game(&tx, &game)?;
    if platform_changed {
        tx.execute("DELETE FROM unlocks WHERE game_id=?1", [&game_id])
            .map_err(|error| error.to_string())?;
        tx.execute("UPDATE games SET scan_status='',schema_source='',source_file='',last_scan='' WHERE id=?1", [&game_id]).map_err(|error| error.to_string())?;
    }
    tx.commit().map_err(|error| error.to_string())?;
    initialized.remove(&game_id);
    Ok(())
}

#[tauri::command]
pub(crate) async fn import_steam(app: tauri::AppHandle) -> Result<usize, String> {
    let state = app.state::<AppState>();
    let (key, steamid) = {
        let conn = lock_db(&state)?;
        (
            db::setting(&conn, "steam_api_key")?,
            db::setting(&conn, "steam_id")?,
        )
    };
    if key.is_empty() || steamid.is_empty() {
        return Err("请先在设置中填写 Steam Web API Key 和 SteamID64".into());
    }
    let games = steam::owned_games(&key, &steamid).await?;
    let conn = lock_db(&state)?;
    let mut added = 0;
    steam_playtime::save_owned(&conn, &steamid, &games)?;
    for game in games {
        if db::game(&conn, &game.id)?.is_none() {
            db::upsert_game(&conn, &game)?;
            added += 1;
        }
    }
    drop(conn);
    start_metadata_refresh(app);
    Ok(added)
}

pub(crate) fn needs_store_metadata(game: &Game) -> bool {
    game.source == "steam"
        && !game.appid.is_empty()
        && serde_json::from_str::<serde_json::Value>(&game.metadata_json)
            .ok()
            .and_then(|value| {
                value
                    .get("name")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned)
            })
            .is_none_or(|name| name.is_empty())
}

pub(crate) fn start_metadata_refresh(app: tauri::AppHandle) {
    let state = app.state::<AppState>();
    let Ok(mut refreshing) = state.metadata_refreshing.lock() else {
        return;
    };
    if *refreshing {
        return;
    }
    *refreshing = true;
    drop(refreshing);

    tauri::async_runtime::spawn(async move {
        let mut attempted = HashSet::new();
        loop {
            let next = {
                let state = app.state::<AppState>();
                lock_db(&state)
                    .and_then(|conn| db::games(&conn))
                    .ok()
                    .and_then(|games| {
                        games.into_iter().find(|game| {
                            !attempted.contains(&game.id) && needs_store_metadata(game)
                        })
                    })
            };
            let Some(game) = next else { break };
            attempted.insert(game.id.clone());
            if let Ok(metadata) = steam::metadata(&game.appid).await {
                let state = app.state::<AppState>();
                if let Ok(conn) = lock_db(&state) {
                    let _ = db::save_metadata(&conn, &game.id, &metadata);
                };
            }
            tokio::time::sleep(Duration::from_millis(1200)).await;
        }
        let state = app.state::<AppState>();
        if let Ok(mut refreshing) = state.metadata_refreshing.lock() {
            *refreshing = false;
        };
    });
}

#[tauri::command]
pub(crate) fn get_settings(state: tauri::State<'_, AppState>) -> Result<serde_json::Value, String> {
    let conn = lock_db(&state)?;
    Ok(serde_json::json!({
        "steamApiKey": db::setting(&conn, "steam_api_key")?,
        "steamId": db::setting(&conn, "steam_id")?,
        "minimizeOnLaunch": db::setting(&conn, "minimize_on_launch")? == "true",
        "closeToTray": db::setting(&conn, "close_to_tray")? != "false",
        "achievementNotifications": db::setting(&conn, "achievement_notifications")? != "false"
    }))
}

#[tauri::command]
pub(crate) fn save_settings(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    steam_api_key: String,
    steam_id: String,
    minimize_on_launch: bool,
    close_to_tray: bool,
    achievement_notifications: bool,
) -> Result<(), String> {
    if !steam_id.is_empty() && !steam_id.chars().all(|ch| ch.is_ascii_digit()) {
        return Err("SteamID 必须是数字".into());
    }
    let conn = lock_db(&state)?;
    db::set_setting(&conn, "steam_api_key", steam_api_key.trim())?;
    db::set_setting(&conn, "steam_id", steam_id.trim())?;
    db::set_setting(
        &conn,
        "minimize_on_launch",
        if minimize_on_launch { "true" } else { "false" },
    )?;
    db::set_setting(
        &conn,
        "close_to_tray",
        if close_to_tray { "true" } else { "false" },
    )?;
    db::set_setting(
        &conn,
        "achievement_notifications",
        if achievement_notifications {
            "true"
        } else {
            "false"
        },
    )?;
    drop(conn);
    steam_playtime::start_refresh(app);
    Ok(())
}

#[tauri::command]
pub(crate) fn toggle_manual(
    state: tauri::State<'_, AppState>,
    game_id: String,
    api_name: String,
) -> Result<(), String> {
    let conn = lock_db(&state)?;
    let game = db::game(&conn, &game_id)?.ok_or("游戏不存在")?;
    if game.source != "local" {
        return Err("Steam 官方成就不能手动修改".into());
    }
    db::toggle_manual(&conn, &game_id, &api_name)
}
