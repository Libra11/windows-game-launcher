use super::{cloud, cloud_game, directory, store, ManagerState};
use serde_json::{json, Value};
use tauri::{AppHandle, Manager};
#[tauri::command]
pub async fn saves_cloud_profiles(app: AppHandle, game_id: String) -> Result<Value,String> {
    cloud::profiles(&app,&cloud_game(&app,&game_id)?).await
}
#[tauri::command]
pub async fn saves_connect(app: AppHandle, profile_id: String, cloud_id: Option<String>) -> Result<store::Binding,String> {
    let manager=app.state::<ManagerState>();let _guard=manager.gate.lock().await;
    let dir=directory(&app)?;let mut b=store::binding(&store::open(&dir)?,&profile_id)?;
    if b.cloud_id.is_some() { return Err("档案已经关联云端，不能换绑".into()); }
    let game=cloud_game(&app,&b.game_id)?;
    let remote=if let Some(id)=cloud_id {
        let value=cloud::profiles(&app,&game).await?;
        let items=value.as_array().or_else(||value["profiles"].as_array()).ok_or("云档案列表格式错误")?;
        items.iter().find(|v|v["id"]==id).cloned().ok_or("云档案不属于此游戏")?
    } else { cloud::create_profile(&app,&game,&b.name).await? };
    b.cloud_id=Some(remote["id"].as_str().ok_or("云档案缺少标识")?.into());
    b.revision=remote["revision"].as_i64().ok_or("云档案缺少版本")?;
    // 关联不意味着本机内容承接云端当前版本。
    store::put_binding(&store::open(&dir)?,&b)?; Ok(b)
}
#[tauri::command]
pub async fn saves_upload(app: AppHandle, profile_id: String) -> Result<Value,String> {
    let manager=app.state::<ManagerState>();let _guard=manager.gate.lock().await;
    let dir=directory(&app)?;let mut b=store::binding(&store::open(&dir)?,&profile_id)?;
    let mut queue=store::snapshots(&store::open(&dir)?,&profile_id)?;
    queue.reverse();
    let mut uploaded=0;
    for mut s in queue.into_iter().filter(|s|!s.uploaded) {
        // 请求前持久化冻结的版本；响应丢失时先核对云端历史，避免重复发布。
        let history=cloud::history(&app,b.cloud_id.as_deref().ok_or("请先关联云存档")?).await?;
        if let Some(existing)=history["snapshots"].as_array().and_then(|items|items.iter().find(|v|v["id"]==s.id)) {
            if existing["archiveHash"].as_str()!=Some(s.archive_hash.as_str()) || existing["manifestHash"].as_str()!=Some(s.manifest_hash.as_str()) {
                return Err("云端同标识快照与本机内容不一致".into());
            }
            s.uploaded=true;s.error=None;store::put_snapshot(&store::open(&dir)?,&s)?;continue;
        }
        if !s.upload_started {
            if s.parent_ids.len()==1 && history["profile"]["selectedSnapshotId"].as_str()==Some(s.parent_ids[0].as_str()) {
                s.base_revision=history["profile"]["revision"].as_i64().ok_or("云档案缺少版本")?;
            }
            s.upload_started=true;store::put_snapshot(&store::open(&dir)?,&s)?;
        }
        let result=cloud::upload(&app,&b,&s).await;
        match result {
            Ok(response)=>{
                if response["status"] != "completed" {
                    if !matches!(response["status"].as_str(), Some("queued" | "validating")) { return Err("云端校验状态无效".into()); }
                    s.error=None;store::put_snapshot(&store::open(&dir)?,&s)?;
                    // 父快照校验发布前，不能提交依赖它的后续快照。
                    return Ok(json!({"uploaded":uploaded,"pending":true,"status":response["status"]}));
                }
                s.uploaded=true;s.error=None;store::put_snapshot(&store::open(&dir)?,&s)?;
                uploaded+=1;
            }
            Err(error)=>{s.error=Some(error.clone());store::put_snapshot(&store::open(&dir)?,&s)?;return Err(format!("本地快照已保留；上传失败：{error}"));}
        }
    }
    let history=cloud::history(&app,b.cloud_id.as_deref().ok_or("请先关联云存档")?).await?;
    b.revision=history["profile"]["revision"].as_i64().ok_or("云档案缺少版本")?;
    store::put_binding(&store::open(&dir)?,&b)?;
    Ok(json!({"uploaded":uploaded,"pending":false,"history":history}))
}
#[tauri::command]
pub async fn saves_history(app: AppHandle, profile_id: String) -> Result<Value,String> {
    let b=store::binding(&store::open(&directory(&app)?)?,&profile_id)?;
    cloud::history(&app,b.cloud_id.as_deref().ok_or("尚未关联云档案")?).await
}
#[tauri::command]
pub async fn saves_download(app: AppHandle, profile_id: String, snapshot_id: String) -> Result<store::Snapshot,String> {
    let manager=app.state::<ManagerState>();let _guard=manager.gate.lock().await;
    let dir=directory(&app)?;let b=store::binding(&store::open(&dir)?,&profile_id)?;
    let history=cloud::history(&app,b.cloud_id.as_deref().ok_or("尚未关联云档案")?).await?;
    let remote=history["snapshots"].as_array().ok_or("云快照列表错误")?.iter()
        .find(|v|v["id"]==snapshot_id).ok_or("快照不属于此档案")?;
    let id=uuid::Uuid::parse_str(&snapshot_id).map_err(|_|"快照标识无效")?;
    let temporary=tempfile::tempdir_in(&dir).map_err(|e|e.to_string())?;
    let path=temporary.path().join("download.zip");
    cloud::download(&app,&snapshot_id,&path).await?;
    let hash=youji_save_archive::file_hash(&path)?;
    if remote["archiveHash"].as_str()!=Some(&hash) { return Err("云快照归档哈希不匹配".into()); }
    let manifest=youji_save_archive::verify(&path)?;let manifest_hash=youji_save_archive::manifest_hash(&manifest)?;
    if remote["manifestHash"].as_str()!=Some(&manifest_hash) { return Err("云快照清单哈希不匹配".into()); }
    let archive=dir.join(format!("download-{id}.zip"));
    // 不能覆盖已有的安全快照；同名内容只有校验一致时才复用。
    if archive.exists() {
        if youji_save_archive::file_hash(&archive)?!=hash { return Err("已有下载快照内容不同".into()); }
    } else {
        std::fs::hard_link(&path,&archive).map_err(|e|e.to_string())?;
    }
    let s=store::Snapshot{id:snapshot_id,profile_id,archive:archive.to_string_lossy().into_owned(),manifest_hash,
        archive_hash:hash,bytes:std::fs::metadata(&archive).map_err(|e|e.to_string())?.len(),
        parent_ids:serde_json::from_value(remote["parentIds"].clone()).map_err(|_|"快照缺少父版本")?,
        base_revision:history["profile"]["revision"].as_i64().ok_or("档案缺少版本")?,
        created_at:remote["serverCreatedAt"].as_str().unwrap_or("").into(),uploaded:true,upload_started:true,error:None};
    store::put_snapshot(&store::open(&dir)?,&s)?;Ok(s)
}
#[tauri::command]
pub async fn saves_resolve(app: AppHandle, profile_id: String, snapshot_id: String, base_revision: i64, parent_ids: Vec<String>) -> Result<Value,String> {
    let manager=app.state::<ManagerState>();let _guard=manager.gate.lock().await;
    let b=store::binding(&store::open(&directory(&app)?)?,&profile_id)?;
    cloud::resolve(&app,b.cloud_id.as_deref().ok_or("尚未关联云档案")?,&snapshot_id,base_revision,parent_ids).await
}
#[tauri::command]
pub async fn saves_delete_cloud(app: AppHandle, snapshot_id: String, confirmed: bool) -> Result<Value,String> {
    if !confirmed {return Err("删除云快照需要确认".into());}
    cloud::delete(&app,&snapshot_id).await
}
