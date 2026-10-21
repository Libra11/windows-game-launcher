// 存档元数据仅在完整验证后发布；所有配额/分支变更按用户串行提交。
mod history;
mod jobs;
mod policy;
#[cfg(test)]
mod tests;
mod uploads;
mod verification;
mod worker;
use crate::{auth::Identity, ApiError, App, Result};
use axum::{
    extract::{Query, State},
    routing::{delete, get, post},
    Json, Router,
};
pub use history::{delete_snapshot, download, list_snapshots, resolve};
pub use jobs::{complete_upload, run_workers, upload_status};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::{Postgres, Row, Transaction};
use std::sync::Arc;
pub use uploads::{cleanup, create_upload};
use uuid::Uuid;

pub fn routes() -> Router<App> {
    Router::new()
        .route("/v1/save-profiles", post(create_profile).get(list_profiles))
        .route("/v1/save-profiles/{id}/settings", post(policy::settings))
        .route("/v1/save-profiles/{id}/prune", post(policy::prune))
        .route("/v1/save-snapshots/{id}/pin", post(policy::pin))
        .route("/v1/save-uploads", post(create_upload))
        .route("/v1/save-uploads/{id}/complete", post(complete_upload))
        .route("/v1/save-uploads/{id}", get(upload_status))
        .route("/v1/save-profiles/{id}/snapshots", get(list_snapshots))
        .route("/v1/save-snapshots/{id}/download", post(download))
        .route("/v1/save-profiles/{id}/resolve", post(resolve))
        .route("/v1/save-snapshots/{id}", delete(delete_snapshot))
}
fn storage(app: &App) -> Result<Arc<crate::storage::Storage>> {
    app.storage.clone().ok_or_else(|| {
        ApiError(
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            "未配置私有存档存储".into(),
        )
    })
}
fn missing() -> ApiError {
    ApiError(axum::http::StatusCode::NOT_FOUND, "存档资源不存在".into())
}
fn internal() -> ApiError {
    ApiError(
        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
        "存档处理失败".into(),
    )
}
async fn lock<'a>(app: &'a App, user: Uuid) -> Result<Transaction<'a, Postgres>> {
    let mut tx = app.pool.begin().await?;
    // 同步模块使用相同用户锁，防止游戏归属在检查过程中改变。
    let found = sqlx::query("SELECT user_id FROM user_sync_state WHERE user_id=$1 FOR UPDATE")
        .bind(user)
        .fetch_optional(&mut *tx)
        .await?;
    if found.is_none() {
        return Err(ApiError::conflict("请先初始化资料库"));
    }
    Ok(tx)
}
async fn profile(
    tx: &mut Transaction<'_, Postgres>,
    user: Uuid,
    id: Uuid,
) -> Result<sqlx::postgres::PgRow> {
    sqlx::query("SELECT * FROM save_profiles WHERE user_id=$1 AND id=$2")
        .bind(user)
        .bind(id)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(missing)
}
fn profile_json(row: &sqlx::postgres::PgRow) -> Value {
    json!({"id":row.get::<Uuid,_>("id"),"gameId":row.get::<Uuid,_>("game_id"),
        "name":row.get::<String,_>("name"),"revision":row.get::<i64,_>("revision"),
        "directoryRule":row.get::<Option<Value>,_>("directory_rule"),"autoRetention":row.get::<bool,_>("auto_retention"),"selectedSnapshotId":row.get::<Option<Uuid>,_>("selected_snapshot_id")})
}
async fn parents(
    tx: &mut Transaction<'_, Postgres>,
    user: Uuid,
    profile: Uuid,
    ids: &[Uuid],
) -> Result<()> {
    if ids.len() > 100 || ids.iter().collect::<std::collections::HashSet<_>>().len() != ids.len() {
        return Err(ApiError::bad("父快照数量或标识无效"));
    }
    for id in ids {
        let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM save_snapshots WHERE user_id=$1 AND profile_id=$2 AND id=$3)")
            .bind(user).bind(profile).bind(id).fetch_one(&mut **tx).await?;
        if !exists {
            return Err(ApiError::bad("父快照不属于当前存档档案"));
        }
    }
    Ok(())
}
async fn add_parents(
    tx: &mut Transaction<'_, Postgres>,
    user: Uuid,
    profile: Uuid,
    id: Uuid,
    ids: &[Uuid],
) -> Result<()> {
    for parent in ids {
        sqlx::query("INSERT INTO save_snapshot_parents(user_id,profile_id,snapshot_id,parent_id) VALUES($1,$2,$3,$4)")
            .bind(user).bind(profile).bind(id).bind(parent).execute(&mut **tx).await?;
    }
    Ok(())
}
pub(super) fn advances(selected: Option<Uuid>, revision: i64, base: i64, parents: &[Uuid]) -> bool {
    revision == base
        && match selected {
            None => parents.is_empty(),
            Some(id) => parents == [id],
        }
}
async fn snapshot(tx: &mut Transaction<'_, Postgres>, user: Uuid, id: Uuid) -> Result<Value> {
    let row=sqlx::query("SELECT s.*,o.available,o.bytes,o.archive_hash,o.manifest_hash,o.manifest,d.name AS device_name FROM save_snapshots s JOIN save_objects o ON o.user_id=s.user_id AND o.id=s.object_id JOIN devices d ON d.user_id=s.user_id AND d.id=s.device_id WHERE s.user_id=$1 AND s.id=$2")
        .bind(user).bind(id).fetch_optional(&mut **tx).await?.ok_or_else(missing)?;
    let parents:Vec<Uuid>=sqlx::query_scalar("SELECT parent_id FROM save_snapshot_parents WHERE user_id=$1 AND snapshot_id=$2 ORDER BY parent_id")
        .bind(user).bind(id).fetch_all(&mut **tx).await?;
    Ok(
        json!({"id":id,"profileId":row.get::<Uuid,_>("profile_id"),"deviceId":row.get::<Uuid,_>("device_id"),
        "deviceName":row.get::<String,_>("device_name"),"parentIds":parents,
        "available":row.get::<bool,_>("available"),"pinned":row.get::<bool,_>("pinned"),"bytes":row.get::<i64,_>("bytes"),"archiveHash":row.get::<String,_>("archive_hash"),
        "manifestHash":row.get::<String,_>("manifest_hash"),"manifest":row.get::<Value,_>("manifest"),
        "serverCreatedAt":row.get::<chrono::DateTime<chrono::Utc>,_>("created_at")}),
    )
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateProfile {
    game_id: Uuid,
    name: String,
}
pub async fn create_profile(
    State(app): State<App>,
    identity: Identity,
    Json(request): Json<CreateProfile>,
) -> Result<Json<Value>> {
    let name = request.name.trim();
    if name.is_empty() || name.chars().count() > 80 || name.chars().any(char::is_control) {
        return Err(ApiError::bad("存档档案名称无效"));
    }
    let mut tx = lock(&app, identity.user_id).await?;
    let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM sync_resources WHERE user_id=$1 AND kind='game' AND id=$2 AND NOT deleted)")
        .bind(identity.user_id).bind(request.game_id.to_string()).fetch_one(&mut *tx).await?;
    if !exists {
        return Err(missing());
    }
    let row = sqlx::query(
        "INSERT INTO save_profiles(user_id,id,game_id,name) VALUES($1,$2,$3,$4) RETURNING *",
    )
    .bind(identity.user_id)
    .bind(Uuid::new_v4())
    .bind(request.game_id)
    .bind(name)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Json(profile_json(&row)))
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProfileQuery {
    game_id: Uuid,
}
pub async fn list_profiles(
    State(app): State<App>,
    identity: Identity,
    Query(query): Query<ProfileQuery>,
) -> Result<Json<Value>> {
    let rows =
        sqlx::query("SELECT * FROM save_profiles WHERE user_id=$1 AND game_id=$2 ORDER BY name,id")
            .bind(identity.user_id)
            .bind(query.game_id)
            .fetch_all(&app.pool)
            .await?;
    Ok(Json(
        json!({"profiles":rows.iter().map(profile_json).collect::<Vec<_>>(),
        "limits":{"maxBytes":youji_cloud_protocol::saves::MAX_BYTES,"maxFiles":youji_cloud_protocol::saves::MAX_FILES,
        "quotaBytes":app.storage.as_ref().map(|s|s.quota)}}),
    ))
}
