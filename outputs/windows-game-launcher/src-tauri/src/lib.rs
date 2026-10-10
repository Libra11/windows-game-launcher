mod achievement_notifications;
mod achievement_overlay;
mod achievement_platform;
mod achievement_repair;
mod activity;
mod backup;
mod cover_cache;
mod cover_download;
mod db;
mod desktop_lifecycle;
mod detection;
mod epic;
mod epic_artwork;
mod epic_achievements;
mod epic_auth;
mod epic_library;
mod grime;
mod installation;
mod library_commands;
mod library_removal;
mod local_import;
mod model;
mod organization;
mod network;
mod record_paths;
mod runtime;
mod runtime_commands;
mod runtime_environment;
mod runtime_persistence;
mod runtime_record;
mod scanner;
mod source;
mod statistics;
mod steam;
mod steam_family;
mod steam_achievements;
mod steam_identity;
mod steam_playtime;
mod steam_search;
mod steam_store;
mod steam_sync;
mod system_proxy;
mod webview_proxy;
mod xbox;
mod xbox_local;

use library_commands::start_metadata_refresh;
use rusqlite::Connection;
use scanner::scan_all;
use std::{collections::HashSet, path::PathBuf, sync::Mutex, time::Duration};
use steam_sync::sync_game_internal;
use tauri::Manager;

struct AppState {
    maintenance: std::sync::atomic::AtomicBool,
    db: Mutex<Connection>,
    initialized: Mutex<HashSet<String>>,
    metadata_refreshing: Mutex<bool>,
    runtime: Mutex<runtime::Tracker>,
    runtime_pending: Mutex<Vec<runtime::Update>>,
    installation: installation::Monitor,
    appdata: Option<PathBuf>,
    public: Option<PathBuf>,
}

fn lock_db(state: &AppState) -> Result<std::sync::MutexGuard<'_, Connection>, String> {
    if state.maintenance.load(std::sync::atomic::Ordering::Acquire) { return Err("游迹正在准备恢复，请等待重启".into()); }
    let conn = state.db.lock().map_err(|_| "数据库暂时不可用".to_string())?;
    if state.maintenance.load(std::sync::atomic::Ordering::Acquire) { return Err("游迹正在准备恢复，请等待重启".into()); }
    Ok(conn)
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            desktop_lifecycle::show(app)
        }))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            let mut conn = db::open(&dir.join("games.sqlite")).map_err(std::io::Error::other)?;
            let covers = app.path().app_cache_dir()?.join("covers");
            backup::startup(&mut conn, &dir, &covers);
            let restored_appearance = backup::appearance_script(&conn).map_err(std::io::Error::other)?;
            let proxy = network::initialize(&conn, app.handle()).map_err(std::io::Error::other)?;
            app.manage(webview_proxy::WebviewProxy::from(&proxy));
            app.manage(cover_cache::CoverCache::default());
            app.manage(backup::Manager::default());
            achievement_repair::remove_inferred_unlocks(&mut conn)
                .map_err(std::io::Error::other)?;
            app.manage(AppState {
                maintenance: Default::default(),
                db: Mutex::new(conn),
                initialized: Mutex::new(HashSet::new()),
                metadata_refreshing: Mutex::new(false),
                runtime: Mutex::new(runtime::Tracker::default()),
                runtime_pending: Mutex::new(Vec::new()),
                installation: installation::Monitor::default(),
                appdata: std::env::var_os("APPDATA").map(PathBuf::from),
                public: std::env::var_os("PUBLIC").map(PathBuf::from),
            });
            // 读取已保存代理后才创建 WebView，避免图片先使用旧的系统配置。
            let main = tauri::WebviewWindowBuilder::from_config(
                app.handle(),
                &app.config().app.windows[0],
            )?;
            let main = if let Some(script) = restored_appearance { main.initialization_script(script) } else { main };
            webview_proxy::configure(app.handle(), main).build()?;
            if let Err(error) = achievement_overlay::initialize(app.handle()) {
                eprintln!("成就弹层初始化失败：{error}");
            }
            runtime_persistence::restore(app.handle()).map_err(std::io::Error::other)?;
            if let Err(error) = desktop_lifecycle::initialize(app.handle()) {
                eprintln!("托盘初始化失败：{error}");
            }
            start_metadata_refresh(app.handle().clone());
            steam_playtime::watch(app.handle().clone());
            let family_time = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let _ = steam_family::playtime::refresh(&family_time).await;
            });
            runtime_commands::watch(app.handle().clone());
            installation::watch(app.handle().clone());
            let watcher = app.handle().clone();
            std::thread::spawn(move || loop {
                scan_all(&watcher);
                std::thread::sleep(Duration::from_secs(3));
            });
            let public_refresh = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let _ = xbox::public::xbox_refresh_definitions(public_refresh).await;
            });
            let synchronizer = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    tokio::time::sleep(Duration::from_secs(1800)).await;
                    let ids = {
                        let state = synchronizer.state::<AppState>();
                        lock_db(&state)
                            .and_then(|conn| db::games(&conn))
                            .unwrap_or_default()
                            .into_iter()
                            .filter(|game| game.source == "steam" || game.source == "epic")
                            .map(|game| game.id)
                            .collect::<Vec<_>>()
                    };
                    for id in ids {
                        let _ = sync_game_internal(&synchronizer, &id, false).await;
                    }
                }
            });
            Ok(())
        })
        .on_window_event(desktop_lifecycle::closing)
        .invoke_handler(tauri::generate_handler![
            library_commands::list_games,
            organization::get_library_organization,
            organization::create_library_tag,
            organization::rename_library_tag,
            organization::delete_library_tag,
            organization::create_library_collection,
            organization::rename_library_collection,
            organization::delete_library_collection,
            organization::reorder_library_collections,
            organization::set_game_organization,
            organization::batch_update_game_organization,

            backup::export_backup,
            backup::inspect_backup,
            backup::preview_backup_paths,
            backup::schedule_backup_restore,
            backup::get_backup_restore_status,
            backup::ack_backup_restore,
            backup::cancel_backup_operation,
            cover_cache::get_cached_cover,
            statistics::get_statistics,
            statistics::list_statistics_sessions,
            library_commands::list_achievements,
            library_commands::import_local,
            achievement_platform::detect_game_platform,
            xbox::public::xbox_refresh_definitions,
            library_removal::remove_local_game,
            steam_search::search_steam_games,
            library_commands::update_local,
            library_commands::import_steam,
            steam_family::steam_family_begin_login,
            steam_family::steam_family_connection_status,
            steam_family::steam_family_disconnect,
            steam_family::import_steam_family,
            steam_family::refresh_steam_family_playtime,
            epic::import_epic,
            epic_auth::epic_begin_login,
            epic_auth::epic_open_account_login,
            epic_auth::epic_complete_login,
            epic_auth::epic_connection_status,
            epic_auth::epic_disconnect,
            library_commands::get_settings,
            library_commands::save_settings,
            network::get_network_settings,
            network::save_network_settings,
            network::test_network_connection,
            library_commands::toggle_manual,
            runtime_commands::launch_game,
            local_import::prepare_local_import,
            runtime_commands::set_favorite,
            runtime_commands::set_launch_path,
            detection::check_detection,
            detection::set_record_path,
            scanner::scan_now,
            achievement_overlay::achievement_overlay_ready,
            achievement_overlay::get_achievement_overlay_options,
            achievement_overlay::save_achievement_overlay_options,
            achievement_notifications::list_recent_unlocks,
            achievement_notifications::test_achievement_notification,
            desktop_lifecycle::quit_launcher,
            steam_sync::sync_game
        ])
        .build(tauri::generate_context!())
        .expect("无法启动游迹")
        .run(|app, event| {
            if matches!(event, tauri::RunEvent::Exit) {
                xbox_local::stop_all();
            }
            if let tauri::RunEvent::ExitRequested { api, .. } = event {
                if app.state::<AppState>().maintenance.load(std::sync::atomic::Ordering::Acquire) { return; }
                if let Err(error) = runtime_persistence::checkpoint(app, true) {
                    api.prevent_exit();
                    desktop_lifecycle::show(app);
                    let _ = tauri::Emitter::emit(app, "launcher-error", error);
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{model::Game, scanner::scan_one};
    use std::fs;

    #[test]
    fn imports_history_notifies_only_new_unlock_and_survives_path_move() {
        let root = std::env::temp_dir().join(format!("launcher-test-{}", uuid::Uuid::new_v4()));
        let appdata = root.join("roaming");
        let original = appdata.join("GSE Saves/480/achievements.json");
        fs::create_dir_all(original.parent().unwrap()).unwrap();
        fs::write(&original, include_str!("../fixtures/runtime-initial.json")).unwrap();
        let state = AppState {
            maintenance: Default::default(),
            db: Mutex::new(db::open(&root.join("games.sqlite")).unwrap()),
            initialized: Mutex::new(HashSet::new()),
            metadata_refreshing: Mutex::new(false),
            runtime: Mutex::new(runtime::Tracker::default()),
            runtime_pending: Mutex::new(Vec::new()),
            installation: installation::Monitor::default(),
            appdata: Some(appdata),
            public: None,
        };
        let mut game = Game {
            id: "local-test".into(),
            source: "local".into(),
            appid: "480".into(),
            title: "测试游戏".into(),
            exe_path: String::new(),
            launch_uri: String::new(),
            custom_unlock_path: String::new(),
            metadata_json: "{}".into(),
            scan_status: String::new(),
            source_file: String::new(),
            last_scan: String::new(),
            schema_source: String::new(),
        };
        db::upsert_game(&*lock_db(&state).unwrap(), &game).unwrap();
        let definitions = source::parse_local_schema(
            include_str!("../fixtures/definitions.json"),
            std::path::Path::new("steam_settings/achievements.json"),
        )
        .unwrap();
        db::save_definitions(
            &mut *lock_db(&state).unwrap(),
            &game.id,
            "480",
            &definitions,
            "本地定义文件",
        )
        .unwrap();
        assert!(scan_one(&state, &game, false).unwrap().is_empty());
        assert_eq!(
            db::achievements(&*lock_db(&state).unwrap(), &game.id)
                .unwrap()
                .iter()
                .filter(|item| item.unlocked_at.is_some())
                .count(),
            1
        );
        fs::write(&original, include_str!("../fixtures/runtime-updated.json")).unwrap();
        assert_eq!(scan_one(&state, &game, true).unwrap().len(), 1);
        assert!(scan_one(&state, &game, true).unwrap().is_empty());
        let moved = root.join("portable/achievements.json");
        fs::create_dir_all(moved.parent().unwrap()).unwrap();
        fs::rename(&original, &moved).unwrap();
        game.custom_unlock_path = moved.to_string_lossy().into_owned();
        db::upsert_game(&*lock_db(&state).unwrap(), &game).unwrap();
        assert!(scan_one(&state, &game, true).unwrap().is_empty());
        fs::remove_file(&moved).unwrap();
        assert!(scan_one(&state, &game, true).unwrap().is_empty());
        assert_eq!(
            db::game(&*lock_db(&state).unwrap(), &game.id)
                .unwrap()
                .unwrap()
                .scan_status,
            "未找到解锁记录文件"
        );
        drop(state);
        let conn = db::open(&root.join("games.sqlite")).unwrap();
        assert_eq!(
            db::achievements(&conn, &game.id)
                .unwrap()
                .iter()
                .filter(|item| item.unlocked_at.is_some())
                .count(),
            2
        );
        fs::remove_dir_all(root).unwrap();
    }
}
