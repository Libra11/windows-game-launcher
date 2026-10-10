use super::{archive, export, model::*, paths, Manager, Prepared};
use crate::{db, lock_db, AppState};
use std::{path::{Path, PathBuf}, sync::atomic::Ordering};
use tauri::{Emitter, Manager as _};

fn directories(app: &tauri::AppHandle) -> Result<(PathBuf, PathBuf), String> {
    Ok((app.path().app_data_dir().map_err(|error| error.to_string())?,
        app.path().app_cache_dir().map_err(|error| error.to_string())?.join("covers")))
}
fn check_cancel(app: &tauri::AppHandle) -> Result<(), String> {
    if app.state::<Manager>().cancelled.load(Ordering::Acquire) { Err("备份操作已取消".into()) } else { Ok(()) }
}
fn progress(app: &tauri::AppHandle, id: &str, phase: &str, completed: usize, total: usize) -> Result<(), String> {
    if phase == "commit" { app.state::<Manager>().seal()?; } else { check_cancel(app)?; }
    let _ = app.emit_to("main", "data-backup-progress", serde_json::json!({"operationId":id,"phase":phase,"completed":completed,"total":total}));
    Ok(())
}
fn capture(app: &tauri::AppHandle, appearance: Appearance) -> Result<(Dataset, Preferences), String> {
    crate::runtime_persistence::checkpoint(app, true)?;
    let state = app.state::<AppState>();
    let mut conn = lock_db(&state)?;
    export::capture(&mut conn, appearance, &chrono::Utc::now().to_rfc3339())
}
fn count(conn: &rusqlite::Connection, table: &str) -> Result<usize, String> {
    let value: i64 = conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| row.get(0)).map_err(|error| error.to_string())?;
    usize::try_from(value).map_err(|_| "当前数据数量超出有效范围".into())
}
pub(crate) fn require_idle(app: &tauri::AppHandle) -> Result<(), String> {
    require_games_idle(app)?;
    if crate::xbox_local::has_active_capture() { return Err("成就捕获尚未停止，请退出游戏并等待捕获结束后重试".into()); }
    Ok(())
}

pub(crate) fn require_games_idle(app: &tauri::AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let tracker = state.runtime.lock().map_err(|_| "运行状态不可用")?;
    let conn = state.db.lock().map_err(|_| "数据库暂时不可用")?;
    let games = db::games(&conn)?;
    if games.iter().any(|game| matches!(tracker.info(&game.id).state.as_str(), "starting" | "running")) {
        return Err("请先退出正在启动或运行的游戏，再执行维护操作".into());
    }
    drop(conn); drop(tracker);
    if crate::runtime::any_running(&games) { return Err("检测到游戏进程仍在运行，请退出游戏后重试".into()); }
    Ok(())
}

#[tauri::command]
pub(crate) async fn export_backup(app: tauri::AppHandle, path: String, appearance: Appearance, operation_id: String) -> Result<ExportResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let manager = app.state::<Manager>();
        let _operation = manager.begin(&operation_id)?;
        let destination = Path::new(&path);
        if destination.extension().and_then(|value| value.to_str()) != Some("youji-backup") { return Err("请选择 .youji-backup 文件名".into()); }
        progress(&app, &operation_id, "snapshot", 0, 1)?;
        let (data, preferences) = capture(&app, appearance)?;
        let (_, covers) = directories(&app)?;
        let manifest = archive::write(destination, data, preferences, &covers, app.package_info().version.to_string(),
            |phase, done, total| progress(&app, &operation_id, phase, done, total))?;
        Ok(ExportResult { path, manifest })
    }).await.map_err(|error| error.to_string())?
}

#[tauri::command]
pub(crate) async fn inspect_backup(app: tauri::AppHandle, path: String, operation_id: String) -> Result<Inspection, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let manager = app.state::<Manager>();
        let _operation = manager.begin(&operation_id)?;
        progress(&app, &operation_id, "verify", 0, 1)?;
        let source = std::fs::canonicalize(&path).map_err(|error| error.to_string())?;
        // 不读取链接文件，准备标识只绑定用户选中的这一份文件。
        if std::fs::symlink_metadata(&path).map_err(|error| error.to_string())?.file_type().is_symlink() { return Err("备份文件不能是链接".into()); }
        let hash = archive::file_hash_with(&source, || check_cancel(&app))?;
        let loaded = archive::read_with_progress(&source, |done, total| progress(&app, &operation_id, "verify", done, total))?;
        if archive::file_hash_with(&source, || check_cancel(&app))? != hash { return Err("备份在检查期间被修改，请重新选择".into()); }
        let state = app.state::<AppState>();
        let conn = lock_db(&state)?;
        let current = Counts { tags:count(&conn,"library_tags")?, collections:count(&conn,"library_collections")?, games:db::games(&conn)?.len(), favorites:crate::activity::all(&conn)?.values().filter(|item| item.favorite).count(),
            sessions:count(&conn,"play_sessions")?, achievements:count(&conn,"achievements")?, unlocks:count(&conn,"unlocks")?, covers:{
                let (_, root) = directories(&app)?;
                let mut statement = conn.prepare("SELECT value FROM settings WHERE key LIKE 'cover_cache:%'").map_err(|error| error.to_string())?;
                let rows = statement.query_map([], |row| row.get::<_, String>(0)).map_err(|error| error.to_string())?;
                let mut files = std::collections::HashSet::new();
                for value in rows {
                    let value: serde_json::Value = serde_json::from_str(&value.map_err(|error| error.to_string())?).map_err(|_| "当前封面索引无效")?;
                    if let Some(name) = value["file"].as_str().filter(|name| super::schema::cover_name(name)) {
                        if root.join(name).is_file() { files.insert(name.to_owned()); }
                    }
                }
                files.len()
            } };
        let id = uuid::Uuid::new_v4().to_string();
        let mut copy = loaded.data.clone();
        let preview = paths::apply(&mut copy, &Relocation::default())?;
        let result = Inspection { preparation_id:id.clone(), manifest:loaded.manifest.clone(), current, paths:preview };
        check_cancel(&app)?;
        let mut prepared = manager.prepared.lock().map_err(|_| "备份准备状态不可用")?;
        prepared.clear(); prepared.insert(id, Prepared { path:source, sha256:hash, loaded });
        let _ = progress(&app, &operation_id, "complete", 1, 1);
        Ok(result)
    }).await.map_err(|error| error.to_string())?
}

#[tauri::command]
pub(crate) async fn preview_backup_paths(app: tauri::AppHandle, preparation_id: String, relocation: Relocation) -> Result<Vec<PathPreview>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let manager = app.state::<Manager>();
        let prepared = manager.prepared.lock().map_err(|_| "备份准备状态不可用")?;
        let bundle = prepared.get(&preparation_id).ok_or("备份准备已过期，请重新选择")?;
        let mut data = bundle.loaded.data.clone();
        paths::apply(&mut data, &relocation)
    }).await.map_err(|error| error.to_string())?
}

#[tauri::command]
pub(crate) async fn schedule_backup_restore(app: tauri::AppHandle, preparation_id: String, relocation: Relocation, current_appearance: Appearance, operation_id: String) -> Result<serde_json::Value, String> {
    let scheduling_app = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let manager = app.state::<Manager>();
        let _operation = manager.begin(&operation_id)?;
        current_appearance.validate()?;
        let prepared = manager.prepared.lock().map_err(|_| "备份准备状态不可用")?;
        let bundle = prepared.get(&preparation_id).ok_or("备份准备已过期，请重新选择")?;
        if archive::file_hash_with(&bundle.path, || check_cancel(&app))? != bundle.sha256 { return Err("备份已改变，请重新检查".into()); }
        let mut candidate = bundle.loaded.data.clone(); paths::apply(&mut candidate, &relocation)?;
        require_idle(&app)?;
        crate::runtime_persistence::checkpoint(&app, true)?;
        let state = app.state::<AppState>();
        let _gate=state.operation_gate.lock().map_err(|_|"启动操作状态不可用")?;
        let maintenance=crate::maintenance::Guard::enter(&state.maintenance)?;
        let scheduled = (|| {
            require_idle(&app)?;
            let (directory, covers) = directories(&app)?;
            let safety_dir = app.path().app_local_data_dir().map_err(|error| error.to_string())?.join("safety-backups");
            std::fs::create_dir_all(&safety_dir).map_err(|error| error.to_string())?;
            let safety = safety_dir.join(format!("游迹恢复前_{}_{}.youji-backup", chrono::Local::now().format("%Y%m%d_%H%M%S"), uuid::Uuid::new_v4()));
            let (data, preferences) = {
                let mut conn = state.db.lock().map_err(|_| "数据库暂时不可用")?;
                export::capture(&mut conn, current_appearance, &chrono::Utc::now().to_rfc3339())?
            };
            progress(&app, &operation_id, "safety", 0, 1)?;
            archive::write(&safety, data, preferences, &covers, app.package_info().version.to_string(),
                |_, done, total| progress(&app, &operation_id, "safety", done, total))?;
            let id = uuid::Uuid::new_v4().to_string();
            let archive_name = format!("restore-{id}.youji-backup");
            let staged = directory.join("backup-staging").join(&archive_name);
            archive::copy_verified(&bundle.path, &staged, &bundle.sha256)?;
            let pending = Pending { restore_id:id, archive:archive_name, sha256:bundle.sha256.clone(), relocation,
                safety_backup:safety.to_string_lossy().into_owned() };
            require_idle(&app)?;
            manager.seal()?;
            archive::atomic_json(&directory.join("pending-backup-restore.json"), &pending)?;
            progress(&app, &operation_id, "restart", 1, 1)?;
            Ok::<_, String>(serde_json::json!({"safetyBackup":pending.safety_backup,"restartScheduled":true}))
        })();
        if scheduled.is_ok() {maintenance.keep_active();}
        scheduled
    }).await.map_err(|error| error.to_string())??;
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        scheduling_app.restart();
    });
    Ok(result)
}

#[tauri::command]
pub(crate) fn get_backup_restore_status(app: tauri::AppHandle) -> Result<Option<RestoreNotice>, String> {
    let (directory, _) = directories(&app)?;
    let path = directory.join("backup-restore-notice.json");
    if !path.is_file() { return Ok(None); }
    serde_json::from_reader(std::fs::File::open(path).map_err(|error| error.to_string())?).map(Some).map_err(|_| "恢复结果无法读取".into())
}

#[tauri::command]
pub(crate) fn ack_backup_restore(app: tauri::AppHandle, restore_id: String, appearance_applied: bool) -> Result<(), String> {
    let state = app.state::<AppState>();
    let conn = lock_db(&state)?;
    if appearance_applied {
        let pending = db::setting(&conn, "restore_pending_appearance")?;
        if !pending.is_empty() {
            let value: serde_json::Value = serde_json::from_str(&pending).map_err(|_| "恢复外观状态无效")?;
            if value["id"].as_str() == Some(restore_id.as_str()) { db::set_setting(&conn, "restore_pending_appearance", "")?; }
        }
    }
    let (directory, _) = directories(&app)?;
    let path = directory.join("backup-restore-notice.json");
    if path.is_file() {
        let mut notice: RestoreNotice = serde_json::from_reader(std::fs::File::open(&path).map_err(|error| error.to_string())?).map_err(|_| "恢复结果无效")?;
        if notice.id == restore_id { notice.acknowledged = true; archive::atomic_json(&path, &notice)?; }
    }
    Ok(())
}

#[tauri::command]
pub(crate) fn cancel_backup_operation(app: tauri::AppHandle, operation_id: String) -> Result<bool, String> {
    let manager = app.state::<Manager>();
    let active = manager.active.lock().map_err(|_| "备份操作状态不可用")?;
    if active.as_deref() != Some(operation_id.as_str()) || !manager.cancelable.load(Ordering::Acquire) { return Ok(false); }
    manager.cancelled.store(true, Ordering::Release); Ok(true)
}
