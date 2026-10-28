pub(crate) mod artwork;
pub(crate) mod catalog;
pub(crate) mod douban;
pub(crate) mod file_filter;
pub(crate) mod ignored;
pub(crate) mod metadata;
pub(crate) mod model;
pub(crate) mod people;
pub(crate) mod playback;
pub(crate) mod presentation;
pub(crate) mod probe;
mod removal;
mod cleanup;
pub(crate) mod scan;
pub(crate) mod statistics;
pub(crate) mod store;
mod tools;
mod source_links;
use metadata::season_match as season_matching;

use serde_json::Value;
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use tauri::{Manager, WebviewWindow};
use tokio::sync::{OwnedRwLockReadGuard, OwnedRwLockWriteGuard, RwLock};

pub(crate) struct State {
    pub dir: PathBuf,
    operations: Arc<RwLock<()>>,
    maintenance: AtomicBool,
}

pub(crate) fn check_maintenance(app: &tauri::AppHandle) -> Result<(), String> {
    if app.state::<State>().maintenance.load(Ordering::Acquire) {
        Err("正在恢复数据，请等待应用重新启动".into())
    } else {
        Ok(())
    }
}

pub(crate) fn begin_maintenance(app: &tauri::AppHandle) -> Result<(), String> {
    app.state::<State>()
        .maintenance
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .map(|_| ())
        .map_err(|_| "正在恢复数据，请等待应用重新启动".into())
}

pub(crate) fn end_maintenance(app: &tauri::AppHandle) {
    app.state::<State>()
        .maintenance
        .store(false, Ordering::Release);
}

pub(crate) async fn operation(app: &tauri::AppHandle) -> Result<OwnedRwLockReadGuard<()>, String> {
    check_maintenance(app)?;
    let guard = app.state::<State>().operations.clone().read_owned().await;
    check_maintenance(app)?;
    Ok(guard)
}

pub(crate) async fn exclusive(app: &tauri::AppHandle) -> Result<OwnedRwLockWriteGuard<()>, String> {
    Ok(app.state::<State>().operations.clone().write_owned().await)
}

pub(crate) fn initialize(app: &tauri::AppHandle, dir: PathBuf) -> Result<(), String> {
    let conn = store::open(&dir)?;
    conn.execute("UPDATE jobs SET state='failed',message='上次任务已中断，可重新执行',error='',updated_at=?1 WHERE kind IN('scan','metadata','probe') AND state IN('running','queued')",
[store::now()]).map_err(|_|"无法恢复影视任务状态")?;

    drop(conn);
    app.manage(State {
        dir,
        operations: Arc::new(RwLock::new(())),
        maintenance: AtomicBool::new(false),
    });
    Ok(())
}

pub(crate) fn start(app: tauri::AppHandle) {
    scan::start(app.clone());
    douban::start(app.clone());
    metadata::start(app.clone());
    people::start(app.clone());
    playback::start(app);
}

fn local(window: &WebviewWindow) -> Result<(), String> {
    if matches!(window.label(), "main" | "media-validation") {
        Ok(())
    } else {
        Err("此操作仅允许从游迹主界面执行".into())
    }
}

macro_rules! media_command {
    ($name:ident, $module:ident, $function:ident) => {
        #[tauri::command]
        pub(crate) async fn $name(
            app: tauri::AppHandle,
            window: WebviewWindow,
            input: Option<Value>,
        ) -> Result<Value, String> {
            local(&window)?;
            check_maintenance(&app)?;
            $module::$function(app, input.unwrap_or_else(|| serde_json::json!({}))).await
        }
    };
}

macro_rules! locked_media_command {
    ($name:ident, $module:ident, $function:ident) => {
        #[tauri::command]
        pub(crate) async fn $name(
            app: tauri::AppHandle,
            window: WebviewWindow,
            input: Option<Value>,
        ) -> Result<Value, String> {
            local(&window)?;
            let _operation = operation(&app).await?;
            $module::$function(app, input.unwrap_or_else(|| serde_json::json!({}))).await
        }
    };
}

locked_media_command!(media_snapshot, catalog, snapshot);
locked_media_command!(media_people_snapshot, catalog, people_snapshot);
locked_media_command!(media_settings_snapshot, catalog, settings_snapshot);
media_command!(media_artwork, artwork, resolve);
locked_media_command!(media_detail, catalog, detail);
media_command!(media_presentation, presentation, get);
locked_media_command!(media_create_item, catalog, create_item);
locked_media_command!(media_set_record, catalog, set_record);
locked_media_command!(media_organization, catalog, organization);
locked_media_command!(media_update_settings, catalog, update_settings);
media_command!(media_add_source, scan, add_source);
media_command!(media_scan, scan, scan);
media_command!(media_browse, scan, browse);
media_command!(media_remove_source, scan, remove_source);
media_command!(media_ignored_paths, ignored, list);
media_command!(media_ignore_path, ignored, update);
media_command!(media_search_metadata, metadata, search);
media_command!(media_identify, metadata, identify);
media_command!(media_match_season, season_matching, command);
media_command!(media_cleanup, cleanup, command);
media_command!(media_refresh_metadata, metadata, refresh);
media_command!(media_probe, probe, probe);
media_command!(media_douban_begin_login, douban, begin_login);
media_command!(media_douban_status, douban, status);
media_command!(media_douban_sync, douban, sync);
media_command!(media_douban_disconnect, douban, disconnect);
media_command!(media_douban_metadata, douban, manual_metadata);
media_command!(media_open_source, source_links, open);
media_command!(media_play, playback, play);
media_command!(media_player_status, playback, status);
media_command!(media_player_stop, playback, stop);
locked_media_command!(media_statistics, statistics, statistics);
