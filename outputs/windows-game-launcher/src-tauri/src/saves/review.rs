use super::{cloud, directory, store, ManagerState};
use serde_json::{json, Value};
use tauri::{AppHandle, Manager};
pub(super) struct Review {
    profile: String,
    snapshot: String,
    hash: String,
    expires: i64,
}
#[tauri::command]
pub async fn saves_review(
    app: AppHandle,
    profile_id: String,
    snapshot_id: String,
) -> Result<Value, String> {
    let manager = app.state::<ManagerState>();
    let _guard = manager.gate.lock().await;
    let c = store::open(&directory(&app)?)?;
    let b = store::binding(&c, &profile_id)?;
    let s = store::snapshot(&c, &snapshot_id)?;
    if s.profile_id != profile_id {
        return Err("快照不属于此档案".into());
    }
    if !std::path::Path::new(&s.archive).is_file() {
        return Err("此版本归档已清理，请选择可恢复版本".into());
    }
    let root = b.root;
    let scan = tauri::async_runtime::spawn_blocking(move || {
        youji_save_archive::scan(std::path::Path::new(&root))
    })
    .await
    .map_err(|e| e.to_string())??;
    let hash = youji_save_archive::manifest_hash(&scan)?;
    let id = uuid::Uuid::new_v4().to_string();
    let mut reviews = manager.reviews.lock().map_err(|_| "恢复确认状态不可用")?;
    let now = chrono::Utc::now().timestamp();
    reviews.retain(|_, r| r.expires > now);
    if reviews.len() >= 32 {
        return Err("恢复预览过多，请稍后重试".into());
    }
    reviews.insert(
        id.clone(),
        Review {
            profile: profile_id,
            snapshot: snapshot_id,
            hash,
            expires: now + 600,
        },
    );
    Ok(
        json!({"reviewId":id,"localFiles":scan.files.len(),"snapshotCreatedAt":s.created_at,"snapshotBytes":s.bytes}),
    )
}
#[tauri::command]
pub async fn saves_restore_review(
    app: AppHandle,
    review_id: String,
    confirmed: bool,
) -> Result<(), String> {
    if !confirmed {
        return Err("恢复需要明确确认".into());
    }
    let r = app
        .state::<ManagerState>()
        .reviews
        .lock()
        .map_err(|_| "恢复确认状态不可用")?
        .remove(&review_id)
        .ok_or("请重新预览恢复")?;
    if r.expires <= chrono::Utc::now().timestamp() {
        return Err("恢复预览已过期，请重新确认".into());
    }
    super::commands::restore_checked(app, r.profile, r.snapshot, r.hash).await
}
#[tauri::command]
pub async fn saves_preflight(app: AppHandle, game_id: String) -> Result<Value, String> {
    let profiles = store::profiles(&store::open(&directory(&app)?)?, &game_id)?;
    let mut notices = Vec::new();
    for b in profiles {
        let Some(id) = &b.cloud_id else {
            continue;
        };
        match cloud::history(&app, id).await {
            Ok(h) => {
                let selected = h["profile"]["selectedSnapshotId"].as_str();
                let items = h["snapshots"].as_array().ok_or("云历史格式错误")?;
                let leaves = items
                    .iter()
                    .filter(|s| {
                        !items.iter().any(|child| {
                            child["parentIds"]
                                .as_array()
                                .is_some_and(|p| p.contains(&s["id"]))
                        })
                    })
                    .count();
                let chosen = items.iter().find(|s| s["id"].as_str() == selected);
                let c = store::open(&directory(&app)?)?;
                let local = store::snapshots(&c, &b.id)?;
                let baseline = super::tasks::settings(&c, &b.id)?.baseline;
                let inherited =
                    selected.is_some_and(|id| is_ancestor(id, b.parent_id.as_deref(), &local));
                let same_content = chosen.is_some_and(|s| {
                    s["manifestHash"]
                        .as_str()
                        .is_some_and(|hash| Some(hash) == baseline.as_deref())
                });
                if selected.is_some() && ((!inherited && !same_content) || leaves > 1) {
                    notices.push(json!({"profileId":b.id,"name":b.name,"localSnapshotId":b.parent_id,"snapshotId":selected,"cloud":chosen,"forked":leaves>1}));
                }
            }
            Err(error) => return Ok(json!({"offline":true,"error":error,"notices":notices})),
        }
    }
    Ok(json!({"offline":false,"notices":notices}))
}
fn is_ancestor(selected: &str, current: Option<&str>, local: &[store::Snapshot]) -> bool {
    let mut queue: Vec<_> = current.into_iter().collect();
    let mut seen = std::collections::HashSet::new();
    while let Some(id) = queue.pop() {
        if id == selected {
            return true;
        }
        if !seen.insert(id) {
            continue;
        }
        if let Some(s) = local.iter().find(|s| s.id == id) {
            queue.extend(s.parent_ids.iter().map(String::as_str));
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn own_unuploaded_descendants_do_not_prompt_for_old_cloud_version() {
        let s = store::Snapshot {
            id: "local".into(),
            profile_id: "p".into(),
            archive: String::new(),
            manifest_hash: String::new(),
            archive_hash: String::new(),
            bytes: 0,
            parent_ids: vec!["cloud".into()],
            base_revision: 1,
            created_at: String::new(),
            uploaded: false,
            upload_started: false,
            error: None,
        };
        assert!(is_ancestor("cloud", Some("local"), &[s.clone()]));
        assert!(!is_ancestor("other-device", Some("local"), &[s]));
        assert!(!is_ancestor("cloud", None, &[]));
    }
}
