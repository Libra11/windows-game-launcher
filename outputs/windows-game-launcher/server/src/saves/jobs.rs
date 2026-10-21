use super::*;
use axum::{extract::Path, http::StatusCode};
use sqlx::postgres::PgRow;

pub(super) struct Job {
    pub user_id: Uuid,
    pub device_id: Uuid,
    pub id: Uuid,
    pub token: Uuid,
    pub request: Value,
    pub staging_key: String,
    pub object_key: String,
}
pub(super) fn state(row: &PgRow) -> Value {
    let complete = row.get::<bool, _>("completed");
    let expired = row.get::<chrono::DateTime<chrono::Utc>, _>("expires_at") <= chrono::Utc::now();
    json!({"uploadId":row.get::<Uuid,_>("id"),"snapshotId":row.get::<Uuid,_>("snapshot_id"),
        "status":if complete {"completed"} else if expired {"expired"} else {row.get::<&str,_>("status")},
        "attempts":row.get::<i32,_>("attempts"),"error":row.get::<String,_>("last_error")})
}
pub async fn upload_status(
    State(app): State<App>,
    identity: Identity,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    let row = sqlx::query("SELECT * FROM save_uploads WHERE user_id=$1 AND id=$2 AND device_id=$3")
        .bind(identity.user_id)
        .bind(id)
        .bind(identity.device_id)
        .fetch_optional(&app.pool)
        .await?
        .ok_or_else(missing)?;
    Ok(Json(state(&row)))
}
pub async fn complete_upload(
    State(app): State<App>,
    identity: Identity,
    Path(id): Path<Uuid>,
) -> Result<(StatusCode, Json<Value>)> {
    storage(&app)?;
    let mut tx = lock(&app, identity.user_id).await?;
    let row = sqlx::query(
        "SELECT * FROM save_uploads WHERE user_id=$1 AND id=$2 AND device_id=$3 FOR UPDATE",
    )
    .bind(identity.user_id)
    .bind(id)
    .bind(identity.device_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(missing)?;
    let value = state(&row);
    if value["status"] == "expired" {
        return Err(ApiError::conflict("上传已过期，请重新申请上传"));
    }
    if value["status"] == "failed" {
        return Err(ApiError::conflict("校验失败，请重新上传本地快照"));
    }
    if value["status"] == "uploading" {
        sqlx::query("UPDATE save_uploads SET status='queued',next_attempt_at=now(),last_error='' WHERE user_id=$1 AND id=$2")
            .bind(identity.user_id).bind(id).execute(&mut *tx).await?;
    }
    let row = sqlx::query("SELECT * FROM save_uploads WHERE user_id=$1 AND id=$2")
        .bind(identity.user_id)
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
    let result = state(&row);
    tx.commit().await?;
    Ok((
        if result["status"] == "completed" {
            StatusCode::OK
        } else {
            StatusCode::ACCEPTED
        },
        Json(result),
    ))
}

// 短事务领取任务，网络与 ZIP 校验都在事务之外。租约过期后可由其他进程接续。
pub(super) async fn claim(app: &App) -> Result<Option<Job>> {
    let quota = storage(app)?.quota;
    let mut tx = app.pool.begin().await?;
    // 与申请/发布保持同一顺序：先用户锁，再上传行锁。跳过忙用户而不是先锁任务。
    let user:Option<Uuid>=sqlx::query_scalar("SELECT s.user_id FROM user_sync_state s WHERE EXISTS(SELECT 1 FROM save_uploads u WHERE u.user_id=s.user_id AND NOT u.completed AND u.expires_at>now() AND ((u.status='queued' AND u.next_attempt_at<=now()) OR (u.status='validating' AND u.lease_until<now()))) ORDER BY s.user_id LIMIT 1 FOR UPDATE OF s SKIP LOCKED")
        .fetch_optional(&mut *tx).await?;
    let Some(user_id) = user else {
        tx.commit().await?;
        return Ok(None);
    };
    let row=sqlx::query("SELECT * FROM save_uploads WHERE user_id=$1 AND NOT completed AND expires_at>now() AND ((status='queued' AND next_attempt_at<=now()) OR (status='validating' AND lease_until<now())) ORDER BY next_attempt_at,id LIMIT 1 FOR UPDATE SKIP LOCKED")
        .bind(user_id).fetch_optional(&mut *tx).await?;
    let Some(row) = row else {
        tx.commit().await?;
        return Ok(None);
    };
    let id: Uuid = row.get("id");
    let exhausted = row.get::<i32, _>("attempts") >= 3;
    let bytes: i64 = row.get("bytes");
    let used = uploads::quota_used(&mut tx, user_id).await?;
    if exhausted
        || (used as u64)
            .checked_add(bytes as u64)
            .is_none_or(|n| n > quota)
    {
        let message = if exhausted {
            "校验进程多次中断，请重新上传"
        } else {
            "存档配额不足，待暂存对象清理后再重试"
        };
        sqlx::query("UPDATE save_uploads SET status='failed',last_error=$3,lease_token=NULL,lease_until=NULL WHERE user_id=$1 AND id=$2")
            .bind(user_id).bind(id).bind(message).execute(&mut *tx).await?;
        tx.commit().await?;
        return Ok(None);
    }
    let token = Uuid::new_v4();
    let object_key = format!("objects/{user_id}/{}", Uuid::new_v4());
    sqlx::query("UPDATE save_uploads SET status='validating',attempts=attempts+1,lease_token=$3,lease_until=now()+interval '2 minutes',object_key=$4 WHERE user_id=$1 AND id=$2")
        .bind(user_id).bind(id).bind(token).bind(&object_key).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO save_object_gc(object_key,user_id,bytes,not_before) VALUES($1,$2,$3,now()+interval '25 hours')")
        .bind(&object_key).bind(user_id).bind(bytes).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Some(Job {
        user_id,
        id,
        token,
        device_id: row.get("device_id"),
        request: row.get("request"),
        staging_key: row.get("staging_key"),
        object_key,
    }))
}
pub(super) async fn renew(app: &App, job: &Job) -> Result<()> {
    let updated=sqlx::query("UPDATE save_uploads SET lease_until=now()+interval '2 minutes' WHERE user_id=$1 AND id=$2 AND lease_token=$3 AND status='validating' AND lease_until>now() AND expires_at>now()")
        .bind(job.user_id).bind(job.id).bind(job.token).execute(&app.pool).await?.rows_affected();
    if updated != 1 {
        return Err(ApiError::conflict("校验租约已失效"));
    }
    Ok(())
}
pub(super) async fn fail(app: &App, job: &Job, error: ApiError) -> Result<()> {
    let transient = error.0.is_server_error();
    // 不持久化存储签名、凭据、文件路径或底层网络错误。
    let message = if transient {
        "存储读取或校验服务暂时失败，后台将重试；多次失败后可重新上传"
    } else {
        "存档校验失败或上传已过期，请检查本地快照后重新上传"
    };
    sqlx::query("UPDATE save_uploads SET status=CASE WHEN $4 AND attempts<3 AND expires_at>now() THEN 'queued' ELSE 'failed' END,next_attempt_at=now()+interval '30 seconds',last_error=$5,lease_token=NULL,lease_until=NULL WHERE user_id=$1 AND id=$2 AND lease_token=$3 AND status='validating'")
        .bind(job.user_id).bind(job.id).bind(job.token).bind(transient).bind(message).execute(&app.pool).await?;
    Ok(())
}

pub use super::worker::run_workers;
