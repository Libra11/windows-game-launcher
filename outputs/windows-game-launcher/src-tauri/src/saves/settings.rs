use super::{cloud, directory, paths, store, tasks};
use serde_json::{json, Value};
use tauri::{AppHandle, Manager};
#[tauri::command]
pub async fn saves_automation_settings(
    app: AppHandle,
    profile_id: String,
    auto_backup: bool,
    auto_upload: bool,
) -> Result<Value, String> {
    let manager = app.state::<super::ManagerState>();
    let _guard = manager.gate.lock().await;
    let c = store::open(&directory(&app)?)?;
    let b = store::binding(&c, &profile_id)?;
    if auto_upload && b.cloud_id.is_none() {
        return Err("请先关联云存档，再开启自动上传".into());
    }
    let mut settings = tasks::settings(&c, &profile_id)?;
    settings.directory_rule = paths::infer(
        std::path::Path::new(&b.root),
        &paths::bases(&app, &b.game_id)?,
    );
    if let Some(id) = &b.cloud_id {
        cloud::settings(&app, id, &settings.directory_rule, auto_upload).await?;
    }
    settings.auto_backup = auto_backup;
    settings.auto_upload = auto_upload;
    tasks::put(&c, &profile_id, &settings)?;
    if auto_backup {
        tasks::enqueue(&c, &profile_id, "backup")?;
    } else {
        c.execute(
            "DELETE FROM save_tasks WHERE profile_id=?1 AND kind='backup'",
            [&profile_id],
        )
        .map_err(|e| e.to_string())?;
    }
    if auto_upload {
        tasks::enqueue(&c, &profile_id, "upload")?;
    } else {
        c.execute(
            "DELETE FROM save_tasks WHERE profile_id=?1 AND kind='upload'",
            [&profile_id],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(json!(settings))
}
#[tauri::command]
pub async fn saves_retry(app: AppHandle, profile_id: String) -> Result<(), String> {
    let manager = app.state::<super::ManagerState>();
    let _guard = manager.gate.lock().await;
    let c = store::open(&directory(&app)?)?;
    store::binding(&c, &profile_id)?;
    c.execute("DELETE FROM save_upload_handles WHERE snapshot_id IN (SELECT id FROM save_snapshots WHERE profile_id=?1)",[&profile_id]).map_err(|e|e.to_string())?;
    c.execute(
        "UPDATE save_tasks SET phase='pending',attempts=0,next_at=0,error=NULL WHERE profile_id=?1",
        [&profile_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
#[tauri::command]
pub async fn saves_locate(
    app: AppHandle,
    game_id: String,
    cloud_id: String,
) -> Result<Value, String> {
    let profiles = cloud::profiles(&app, &super::cloud_game(&app, &game_id)?).await?;
    let p = profiles["profiles"]
        .as_array()
        .and_then(|v| v.iter().find(|p| p["id"] == cloud_id))
        .ok_or("云档案不属于此游戏")?;
    let candidates = if p["directoryRule"].is_null() {
        Vec::new()
    } else {
        let rule =
            serde_json::from_value(p["directoryRule"].clone()).map_err(|_| "云目录规则无效")?;
        paths::resolve(&rule, &paths::bases(&app, &game_id)?)?
    };
    Ok(json!({"candidates":candidates,"requiresConfirmation":true}))
}
#[tauri::command]
pub async fn saves_pin(
    app: AppHandle,
    profile_id: String,
    snapshot_id: String,
    pinned: bool,
) -> Result<(), String> {
    let manager = app.state::<super::ManagerState>();
    let _guard = manager.gate.lock().await;
    let c = store::open(&directory(&app)?)?;
    let b = store::binding(&c, &profile_id)?;
    if b.cloud_id.is_some() {
        let h = cloud::history(&app, b.cloud_id.as_deref().unwrap()).await?;
        if h["snapshots"]
            .as_array()
            .is_some_and(|v| v.iter().any(|s| s["id"] == snapshot_id))
        {
            cloud::pin(&app, &snapshot_id, pinned).await?;
        } else if store::snapshot(&c, &snapshot_id)?.profile_id != profile_id {
            return Err("快照不属于此档案".into());
        }
    } else if store::snapshot(&c, &snapshot_id)?.profile_id != profile_id {
        return Err("快照不属于此档案".into());
    }
    c.execute("INSERT INTO save_local_flags(snapshot_id,pinned) VALUES(?1,?2) ON CONFLICT(snapshot_id) DO UPDATE SET pinned=excluded.pinned",rusqlite::params![snapshot_id,pinned]).map_err(|e|e.to_string())?;
    Ok(())
}
