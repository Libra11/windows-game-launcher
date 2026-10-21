use super::store::{Binding, Snapshot};
use serde_json::{json, Value};
use std::{collections::BTreeMap, path::Path};
use tauri::AppHandle;
fn id(value: &str) -> Result<String, String> {
    uuid::Uuid::parse_str(value)
        .map(|v| v.to_string())
        .map_err(|_| "无效云端存档标识".into())
}
pub async fn profiles(app: &AppHandle, game_id: &str) -> Result<Value, String> {
    crate::cloud::request(
        app,
        "GET",
        &format!("/v1/save-profiles?gameId={}", id(game_id)?),
        None,
    )
    .await
}
pub async fn create_profile(app: &AppHandle, game_id: &str, name: &str) -> Result<Value, String> {
    crate::cloud::request(
        app,
        "POST",
        "/v1/save-profiles",
        Some(json!({"gameId":id(game_id)?,"name":name})),
    )
    .await
}
pub async fn history(app: &AppHandle, profile: &str) -> Result<Value, String> {
    crate::cloud::request(
        app,
        "GET",
        &format!("/v1/save-profiles/{}/snapshots", id(profile)?),
        None,
    )
    .await
}
pub async fn upload(
    app: &AppHandle,
    profile: &Binding,
    snapshot: &Snapshot,
) -> Result<Value, String> {
    use rusqlite::OptionalExtension;
    let c = super::store::open(&super::directory(app)?)?;
    let handle: Option<String> = c
        .query_row(
            "SELECT upload_id FROM save_upload_handles WHERE snapshot_id=?1",
            [&snapshot.id],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    if let Some(handle) = handle {
        let status = crate::cloud::request(
            app,
            "GET",
            &format!("/v1/save-uploads/{}", id(&handle)?),
            None,
        )
        .await?;
        match status["status"].as_str() {
            Some("completed" | "queued" | "validating") => return Ok(status),
            Some("failed") => {
                return Err(format!(
                    "云端存档校验失败，请检查后点击重新尝试：{}",
                    status["error"].as_str().unwrap_or("未知原因")
                ))
            }
            Some("expired" | "uploading") => {}
            _ => return Err("上传查询状态无效".into()),
        }
    }
    let request = youji_save_archive::UploadRequest {
        snapshot_id: uuid::Uuid::parse_str(&snapshot.id).map_err(|e| e.to_string())?,
        profile_id: uuid::Uuid::parse_str(profile.cloud_id.as_deref().ok_or("尚未关联云端档案")?)
            .map_err(|e| e.to_string())?,
        parent_ids: snapshot
            .parent_ids
            .iter()
            .map(|s| uuid::Uuid::parse_str(s).map_err(|e| e.to_string()))
            .collect::<Result<_, _>>()?,
        base_revision: snapshot.base_revision,
        manifest_hash: snapshot.manifest_hash.clone(),
        archive_hash: snapshot.archive_hash.clone(),
        bytes: snapshot.bytes,
    };
    let response = crate::cloud::request(
        app,
        "POST",
        "/v1/save-uploads",
        Some(serde_json::to_value(request).map_err(|e| e.to_string())?),
    )
    .await?;
    let upload_id = id(response["uploadId"]
        .as_str()
        .ok_or("上传响应缺少任务标识")?)?;
    c.execute("INSERT INTO save_upload_handles VALUES(?1,?2) ON CONFLICT(snapshot_id) DO UPDATE SET upload_id=excluded.upload_id",rusqlite::params![snapshot.id,upload_id]).map_err(|e|e.to_string())?;
    match response["status"].as_str() {
        Some("completed" | "queued" | "validating") => return Ok(response),
        Some("uploading") => {}
        _ => return Err("上传任务状态无效".into()),
    }
    let url = response["uploadUrl"].as_str().ok_or("上传响应缺少地址")?;
    let headers: BTreeMap<String, String> =
        serde_json::from_value(response["headers"].clone()).map_err(|_| "上传响应缺少请求头")?;
    youji_save_archive::transfer::upload(url, &headers, Path::new(&snapshot.archive)).await?;
    crate::cloud::request(
        app,
        "POST",
        &format!("/v1/save-uploads/{upload_id}/complete"),
        Some(json!({})),
    )
    .await
}
pub async fn download(app: &AppHandle, snapshot: &str, path: &Path) -> Result<(), String> {
    let result = crate::cloud::request(
        app,
        "POST",
        &format!("/v1/save-snapshots/{}/download", id(snapshot)?),
        Some(json!({})),
    )
    .await?;
    youji_save_archive::transfer::download(result["url"].as_str().ok_or("下载响应缺少地址")?, path)
        .await
}
pub async fn resolve(
    app: &AppHandle,
    profile: &str,
    snapshot: &str,
    revision: i64,
    parents: Vec<String>,
) -> Result<Value, String> {
    let parents: Vec<String> = parents.iter().map(|v| id(v)).collect::<Result<_, _>>()?;
    crate::cloud::request(
        app,
        "POST",
        &format!("/v1/save-profiles/{}/resolve", id(profile)?),
        Some(json!({"snapshotId":id(snapshot)?,"baseRevision":revision,"parentIds":parents})),
    )
    .await
}
pub async fn delete(app: &AppHandle, snapshot: &str) -> Result<Value, String> {
    crate::cloud::request(
        app,
        "DELETE",
        &format!("/v1/save-snapshots/{}", id(snapshot)?),
        None,
    )
    .await
}

pub async fn settings(
    app: &AppHandle,
    id: &str,
    rule: &Option<youji_cloud_protocol::save_policy::DirectoryRule>,
    retention: bool,
) -> Result<Value, String> {
    crate::cloud::request(
        app,
        "POST",
        &format!("/v1/save-profiles/{}/settings", self::id(id)?),
        Some(json!({"directoryRule":rule,"autoRetention":retention})),
    )
    .await
}
pub async fn prune(app: &AppHandle, id: &str) -> Result<Value, String> {
    crate::cloud::request(
        app,
        "POST",
        &format!("/v1/save-profiles/{}/prune", self::id(id)?),
        Some(json!({})),
    )
    .await
}
pub async fn pin(app: &AppHandle, id: &str, pinned: bool) -> Result<Value, String> {
    crate::cloud::request(
        app,
        "POST",
        &format!("/v1/save-snapshots/{}/pin", self::id(id)?),
        Some(json!({"pinned":pinned})),
    )
    .await
}
