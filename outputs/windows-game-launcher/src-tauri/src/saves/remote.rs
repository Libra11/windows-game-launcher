use super::{cloud, cloud_game, directory, store, ManagerState};
use serde_json::{json, Value};
use tauri::{AppHandle, Manager};
#[tauri::command]
pub async fn saves_cloud_profiles(app: AppHandle, game_id: String) -> Result<Value, String> {
    cloud::profiles(&app, &cloud_game(&app, &game_id)?).await
}
#[tauri::command]
pub async fn saves_connect(
    app: AppHandle,
    profile_id: String,
    cloud_id: Option<String>,
) -> Result<store::Binding, String> {
    let manager = app.state::<ManagerState>();
    let _guard = manager.gate.lock().await;
    let dir = directory(&app)?;
    let mut b = store::binding(&store::open(&dir)?, &profile_id)?;
    if b.cloud_id.is_some() {
        return Err("档案已经关联云端，不能换绑".into());
    }
    let game = cloud_game(&app, &b.game_id)?;
    let remote = if let Some(id) = cloud_id {
        let value = cloud::profiles(&app, &game).await?;
        let items = value
            .as_array()
            .or_else(|| value["profiles"].as_array())
            .ok_or("云档案列表格式错误")?;
        items
            .iter()
            .find(|v| v["id"] == id)
            .cloned()
            .ok_or("云档案不属于此游戏")?
    } else {
        cloud::create_profile(&app, &game, &b.name).await?
    };
    b.cloud_id = Some(remote["id"].as_str().ok_or("云档案缺少标识")?.into());
    b.revision = remote["revision"].as_i64().ok_or("云档案缺少版本")?;
    // 关联不意味着本机内容承接云端当前版本。
    store::put_binding(&store::open(&dir)?, &b)?;
    Ok(b)
}
#[tauri::command]
pub async fn saves_upload(app: AppHandle, profile_id: String) -> Result<Value, String> {
    let c = store::open(&directory(&app)?)?;
    super::tasks::enqueue(&c, &profile_id, "upload")?;
    let task = super::tasks::list(&c)?
        .into_iter()
        .find(|t| t.profile_id == profile_id && t.kind == "upload")
        .ok_or("上传任务不存在")?;
    drop(c);
    let result = upload(app.clone(), profile_id, Some(&task)).await?;
    if result["pending"] != true {
        super::tasks::finish(&store::open(&directory(&app)?)?, &task)?;
    }
    Ok(result)
}
pub(super) async fn upload(
    app: AppHandle,
    profile_id: String,
    task: Option<&super::tasks::Task>,
) -> Result<Value, String> {
    let manager = app.state::<ManagerState>();
    let _guard = manager.gate.lock().await;
    let dir = directory(&app)?;
    if let Some(task) = task {
        if !super::tasks::list(&store::open(&dir)?)?.iter().any(|t| {
            t.profile_id == task.profile_id
                && t.kind == task.kind
                && t.generation == task.generation
        }) {
            return Ok(json!({"pending":false,"cancelled":true}));
        }
    }
    let mut b = store::binding(&store::open(&dir)?, &profile_id)?;
    let mut queue = store::snapshots(&store::open(&dir)?, &profile_id)?;
    let flags = super::retention::flags(&store::open(&dir)?)?;
    queue.retain(|s| flags.get(&s.id).is_none_or(|f| f["available"] != false));
    let has_local_snapshot = !queue.is_empty();
    queue.reverse();
    let mut uploaded = 0;
    for mut s in queue.into_iter().filter(|s| !s.uploaded) {
        // 请求前持久化冻结的版本；响应丢失时先核对云端历史，避免重复发布。
        let history = cloud::history(&app, b.cloud_id.as_deref().ok_or("请先关联云存档")?).await?;
        if let Some(existing) = history["snapshots"]
            .as_array()
            .and_then(|items| items.iter().find(|v| v["id"] == s.id))
        {
            if existing["available"] == false {
                return Err("云端归档已清理，需要手动创建新的备份版本".into());
            }
            if existing["archiveHash"].as_str() != Some(s.archive_hash.as_str())
                || existing["manifestHash"].as_str() != Some(s.manifest_hash.as_str())
            {
                return Err("云端同标识快照与本机内容不一致".into());
            }
            if super::retention::flags(&store::open(&dir)?)?
                .get(&s.id)
                .is_some_and(|f| f["pinned"] == true)
            {
                cloud::pin(&app, &s.id, true).await?;
            }
            s.uploaded = true;
            s.error = None;
            store::put_snapshot(&store::open(&dir)?, &s)?;
            continue;
        }
        if !s.upload_started {
            // 仅本地模式可能已回收旧归档，云端从仍可恢复的历史开始。
            s.parent_ids.retain(|id| {
                flags.get(id).is_none_or(|f| f["available"] != false)
                    || history["snapshots"]
                        .as_array()
                        .is_some_and(|items| items.iter().any(|v| v["id"] == *id))
            });
            if s.parent_ids.len() == 1
                && history["profile"]["selectedSnapshotId"].as_str()
                    == Some(s.parent_ids[0].as_str())
            {
                s.base_revision = history["profile"]["revision"]
                    .as_i64()
                    .ok_or("云档案缺少版本")?;
            }
            s.upload_started = true;
            store::put_snapshot(&store::open(&dir)?, &s)?;
        }
        let result = cloud::upload(&app, &b, &s).await;
        match result {
            Ok(response) => {
                if response["status"] != "completed" {
                    if !matches!(response["status"].as_str(), Some("queued" | "validating")) {
                        return Err("云端校验状态无效".into());
                    }
                    s.error = None;
                    store::put_snapshot(&store::open(&dir)?, &s)?;
                    // 父快照校验发布前，不能提交依赖它的后续快照。
                    return Ok(
                        json!({"uploaded":uploaded,"pending":true,"status":response["status"]}),
                    );
                }
                if super::retention::flags(&store::open(&dir)?)?
                    .get(&s.id)
                    .is_some_and(|f| f["pinned"] == true)
                {
                    cloud::pin(&app, &s.id, true).await?;
                }
                s.uploaded = true;
                s.error = None;
                store::put_snapshot(&store::open(&dir)?, &s)?;
                uploaded += 1;
            }
            Err(error) => {
                if error.contains("配额")
                    && super::tasks::settings(&store::open(&dir)?, &profile_id)?.auto_upload
                {
                    let _ = cloud::prune(&app, b.cloud_id.as_deref().unwrap()).await;
                }
                s.error = Some(error.clone());
                store::put_snapshot(&store::open(&dir)?, &s)?;
                return Err(format!("本地快照已保留；上传失败：{error}"));
            }
        }
    }
    let history = cloud::history(&app, b.cloud_id.as_deref().ok_or("请先关联云存档")?).await?;
    b.revision = history["profile"]["revision"]
        .as_i64()
        .ok_or("云档案缺少版本")?;
    store::put_binding(&store::open(&dir)?, &b)?;
    let mut settings = super::tasks::settings(&store::open(&dir)?, &profile_id)?;
    record_upload_status(&mut settings, has_local_snapshot);
    super::tasks::put(&store::open(&dir)?, &profile_id, &settings)?;
    if settings.auto_upload && has_local_snapshot {
        cloud::prune(&app, b.cloud_id.as_deref().unwrap()).await?;
        super::retention::prune(&store::open(&dir)?, &dir, &b, &settings)?;
    }
    Ok(json!({"uploaded":uploaded,"pending":false,"history":history}))
}
#[tauri::command]
pub async fn saves_history(app: AppHandle, profile_id: String) -> Result<Value, String> {
    let b = store::binding(&store::open(&directory(&app)?)?, &profile_id)?;
    cloud::history(&app, b.cloud_id.as_deref().ok_or("尚未关联云档案")?).await
}
#[tauri::command]
pub async fn saves_download(
    app: AppHandle,
    profile_id: String,
    snapshot_id: String,
) -> Result<store::Snapshot, String> {
    let manager = app.state::<ManagerState>();
    let _guard = manager.gate.lock().await;
    let dir = directory(&app)?;
    let b = store::binding(&store::open(&dir)?, &profile_id)?;
    let history = cloud::history(&app, b.cloud_id.as_deref().ok_or("尚未关联云档案")?).await?;
    let remote = history["snapshots"]
        .as_array()
        .ok_or("云快照列表错误")?
        .iter()
        .find(|v| v["id"] == snapshot_id)
        .ok_or("快照不属于此档案")?;
    if remote["available"] == false {
        return Err("此版本归档已清理，不能恢复".into());
    }
    let id = uuid::Uuid::parse_str(&snapshot_id).map_err(|_| "快照标识无效")?;
    let temporary = tempfile::tempdir_in(&dir).map_err(|e| e.to_string())?;
    let path = temporary.path().join("download.zip");
    cloud::download(&app, &snapshot_id, &path).await?;
    let hash = youji_save_archive::file_hash(&path)?;
    if remote["archiveHash"].as_str() != Some(&hash) {
        return Err("云快照归档哈希不匹配".into());
    }
    let manifest = youji_save_archive::verify(&path)?;
    let manifest_hash = youji_save_archive::manifest_hash(&manifest)?;
    if remote["manifestHash"].as_str() != Some(&manifest_hash) {
        return Err("云快照清单哈希不匹配".into());
    }
    let archive = dir.join(format!("download-{id}.zip"));
    // 不能覆盖已有的安全快照；同名内容只有校验一致时才复用。
    if archive.exists() {
        if youji_save_archive::file_hash(&archive)? != hash {
            return Err("已有下载快照内容不同".into());
        }
    } else {
        std::fs::hard_link(&path, &archive).map_err(|e| e.to_string())?;
    }
    let s = store::Snapshot {
        id: snapshot_id,
        profile_id,
        archive: archive.to_string_lossy().into_owned(),
        manifest_hash,
        archive_hash: hash,
        bytes: std::fs::metadata(&archive)
            .map_err(|e| e.to_string())?
            .len(),
        parent_ids: serde_json::from_value(remote["parentIds"].clone())
            .map_err(|_| "快照缺少父版本")?,
        base_revision: history["profile"]["revision"]
            .as_i64()
            .ok_or("档案缺少版本")?,
        created_at: remote["serverCreatedAt"].as_str().unwrap_or("").into(),
        uploaded: true,
        upload_started: true,
        error: None,
    };
    store::put_snapshot(&store::open(&dir)?, &s)?;
    store::open(&dir)?
        .execute(
            "UPDATE save_local_flags SET available=1 WHERE snapshot_id=?1",
            [&s.id],
        )
        .map_err(|e| e.to_string())?;
    Ok(s)
}
#[tauri::command]
pub async fn saves_resolve(
    app: AppHandle,
    profile_id: String,
    snapshot_id: String,
    base_revision: i64,
    parent_ids: Vec<String>,
) -> Result<Value, String> {
    let manager = app.state::<ManagerState>();
    let _guard = manager.gate.lock().await;
    let b = store::binding(&store::open(&directory(&app)?)?, &profile_id)?;
    cloud::resolve(
        &app,
        b.cloud_id.as_deref().ok_or("尚未关联云档案")?,
        &snapshot_id,
        base_revision,
        parent_ids,
    )
    .await
}
#[tauri::command]
pub async fn saves_delete_cloud(
    app: AppHandle,
    snapshot_id: String,
    confirmed: bool,
) -> Result<Value, String> {
    if !confirmed {
        return Err("删除云快照需要确认".into());
    }
    cloud::delete(&app, &snapshot_id).await
}

fn record_upload_status(settings: &mut super::tasks::Settings, has_local_snapshot: bool) {
    if has_local_snapshot {
        settings.last_success = Some(chrono::Utc::now().to_rfc3339());
        settings.notice = Some("云端已备份".into());
    } else {
        settings.last_success = None;
        settings.notice = Some("尚无本地快照，等待备份后上传".into());
    }
}

#[cfg(test)]
mod upload_status_tests {
    use super::*;
    #[test]
    fn empty_upload_does_not_report_success_or_keep_false_success() {
        let mut settings = super::super::tasks::Settings::default();
        record_upload_status(&mut settings, true);
        assert!(settings.last_success.is_some());
        record_upload_status(&mut settings, false);
        assert!(settings.last_success.is_none());
        assert_eq!(settings.notice.as_deref(), Some("尚无本地快照，等待备份后上传"));
    }
}
