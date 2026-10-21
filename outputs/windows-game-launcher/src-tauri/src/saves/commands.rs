use super::{directory, restore, store, ManagerState};
use serde_json::{json, Value};
use std::{fs, path::Path};
use tauri::{AppHandle, Manager};
#[tauri::command]
pub async fn saves_list(app: AppHandle, game_id: String) -> Result<Value,String> {
    let dir = directory(&app)?;
    let c = store::open(&dir)?;
    let profiles = store::profiles(&c,&game_id)?;
    let mut entries = Vec::new();
    for p in profiles { let snapshots=store::snapshots(&c,&p.id)?; entries.push(json!({"profile":p,"snapshots":snapshots})); }
    Ok(json!(entries))
}
#[tauri::command]
pub async fn saves_bind(app: AppHandle, game_id: String, name: String, root: String) -> Result<store::Binding,String> {
    let manager=app.state::<ManagerState>(); let _guard=manager.gate.lock().await;
    let state=app.state::<crate::AppState>();
    let _gate=crate::maintenance::lock_operation(&state.operation_gate,&state.maintenance)?;
    let conn=crate::lock_db(&state)?;
    if crate::db::game(&conn,&game_id)?.is_none() { return Err("游戏不存在".into()); }
    drop(conn);
    let name=name.trim();
    if name.is_empty() || name.chars().count()>80 || name.chars().any(char::is_control) { return Err("档案名必须为 1–80 个字符".into()); }
    let root=youji_save_archive::validate_root(Path::new(&root))?;
    let dir=directory(&app)?;
    let c=store::open(&dir)?;
    let appdata=dir.parent().unwrap().canonicalize().map_err(|e|e.to_string())?;
    if root.starts_with(&appdata) || appdata.starts_with(&root) { return Err("存档目录不能包含游迹数据目录".into()); }
    let mut stmt=c.prepare("SELECT data FROM save_profiles").map_err(|e|e.to_string())?;
    for row in stmt.query_map([],|r|r.get::<_,String>(0)).map_err(|e|e.to_string())? {
        let other:store::Binding=serde_json::from_str(&row.map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
        let path=Path::new(&other.root);
        if root.starts_with(path) || path.starts_with(&root) { return Err("此目录与已有存档档案重叠".into()); }
    }
    let b=store::Binding{id:uuid::Uuid::new_v4().to_string(),game_id,name:name.into(),root:root.to_str().ok_or("路径不是 UTF-8")?.into(),cloud_id:None,parent_id:None,revision:0};
    store::put_binding(&c,&b)?; Ok(b)
}
#[tauri::command]
pub async fn saves_backup(app: AppHandle, profile_id: String) -> Result<store::Snapshot,String> {
    let manager=app.state::<ManagerState>(); let _guard=manager.gate.lock().await;
    let worker=app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let app=worker;
        let state=app.state::<crate::AppState>();
        let _gate=crate::maintenance::lock_operation(&state.operation_gate,&state.maintenance)?;
        let _maintenance=crate::maintenance::Guard::enter(&state.maintenance)?;
        crate::backup::require_idle(&app)?;
        let dir=directory(&app)?; let mut c=store::open(&dir)?;
        let mut b=store::binding(&c,&profile_id)?;
        let id=uuid::Uuid::new_v4().to_string(); let archive=dir.join(format!("{id}.zip"));
        let manifest=youji_save_archive::create(Path::new(&b.root),&archive)?;
        let s=store::Snapshot{id:id.clone(),profile_id:b.id.clone(),archive:archive.to_string_lossy().into_owned(),
            manifest_hash:youji_save_archive::manifest_hash(&manifest)?,archive_hash:youji_save_archive::file_hash(&archive)?,
            bytes:fs::metadata(&archive).map_err(|e|e.to_string())?.len(),parent_ids:b.parent_id.clone().into_iter().collect(),
            base_revision:b.revision,created_at:chrono::Utc::now().to_rfc3339(),uploaded:false,upload_started:false,error:None};
        b.parent_id=Some(id);
        let tx=c.transaction().map_err(|e|e.to_string())?;
        store::put_snapshot(&tx,&s)?; store::put_binding(&tx,&b)?; tx.commit().map_err(|e|e.to_string())?;
        Ok(s)
    }).await.map_err(|e|e.to_string())?
}
#[tauri::command]
pub async fn saves_restore(app: AppHandle, profile_id: String, snapshot_id: String, confirmed: bool) -> Result<(),String> {
    if !confirmed { return Err("恢复需要明确确认替换整个专用目录".into()); }
    let manager=app.state::<ManagerState>(); let _guard=manager.gate.lock().await;
    let worker=app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let app=worker;
        let state=app.state::<crate::AppState>();
        let _gate=crate::maintenance::lock_operation(&state.operation_gate,&state.maintenance)?;
        let maintenance=crate::maintenance::Guard::enter(&state.maintenance)?;
        crate::backup::require_idle(&app)?;
        let dir=directory(&app)?; let c=store::open(&dir)?;
        let b=store::binding(&c,&profile_id)?; let s=store::snapshot(&c,&snapshot_id)?;
        if s.profile_id!=b.id { return Err("快照不属于此存档档案".into()); }
        if youji_save_archive::file_hash(Path::new(&s.archive))? != s.archive_hash { return Err("归档校验失败".into()); }
        let job=restore::prepare(&c,&dir,&b,&s)?;
        match restore::apply(&c,&dir,job) {
            Ok(())=>Ok(()),
            Err(error)=>{
                if let Err(recovery)=restore::recover(&c,&dir) {
                    maintenance.keep_active();
                    return Err(format!("{error}；自动恢复未完成：{recovery}。已阻止游戏启动，请重新启动游迹重试恢复。"));
                }
                Err(format!("{error}；已检查恢复日志并保留安全快照。"))
            }
        }
    }).await.map_err(|e|e.to_string())?
}
