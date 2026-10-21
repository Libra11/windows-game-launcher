use super::*;
use youji_cloud_protocol::saves::{valid_hash, UploadRequest, MAX_BYTES};

// 调用方须已取得用户同步锁；GC 覆盖暂存、失败尝试和延迟删除对象。
pub(super) async fn quota_used(tx: &mut Transaction<'_, Postgres>, user: Uuid) -> Result<i64> {
    Ok(sqlx::query_scalar("SELECT COALESCE((SELECT sum(bytes) FROM save_objects WHERE user_id=$1),0)::bigint + COALESCE((SELECT sum(bytes) FROM save_object_gc WHERE user_id=$1),0)::bigint")
        .bind(user).fetch_one(&mut **tx).await?)
}
fn validate(request: &UploadRequest) -> Result<()> {
    if request.bytes == 0
        || request.bytes > MAX_BYTES
        || request.base_revision < 0
        || !valid_hash(&request.manifest_hash)
        || !valid_hash(&request.archive_hash)
        || request.parent_ids.contains(&request.snapshot_id)
    {
        return Err(ApiError::bad("上传大小、版本或校验值无效"));
    }
    Ok(())
}
pub async fn create_upload(
    State(app): State<App>,
    identity: Identity,
    Json(request): Json<UploadRequest>,
) -> Result<Json<Value>> {
    validate(&request)?;
    let storage = storage(&app)?;
    storage.check_private().await?;
    let mut tx = lock(&app, identity.user_id).await?;
    profile(&mut tx, identity.user_id, request.profile_id).await?;
    parents(
        &mut tx,
        identity.user_id,
        request.profile_id,
        &request.parent_ids,
    )
    .await?;
    let value = serde_json::to_value(&request).map_err(|_| internal())?;
    let existing = sqlx::query("SELECT * FROM save_uploads WHERE user_id=$1 AND snapshot_id=$2")
        .bind(identity.user_id)
        .bind(request.snapshot_id)
        .fetch_optional(&mut *tx)
        .await?;
    let (id, key) = if let Some(row) = existing {
        if row.get::<Value, _>("request") != value
            || row.get::<Uuid, _>("device_id") != identity.device_id
        {
            return Err(ApiError::conflict("快照标识已被其他上传使用"));
        }
        if row.get::<bool, _>("completed")
            || (row.get::<chrono::DateTime<chrono::Utc>, _>("expires_at") > chrono::Utc::now()
                && matches!(
                    row.get::<String, _>("status").as_str(),
                    "queued" | "validating"
                ))
        {
            let state = super::jobs::state(&row);
            tx.commit().await?;
            return Ok(Json(state));
        }
        if row.get::<chrono::DateTime<chrono::Utc>, _>("expires_at") <= chrono::Utc::now() {
            // 快照身份保持不变；旧上传令牌已失效，重新预留配额并签发新任务。
            let used = quota_used(&mut tx, identity.user_id).await?;
            if (used as u64)
                .checked_add(request.bytes)
                .is_none_or(|n| n > storage.quota)
            {
                return Err(ApiError::conflict("存档配额不足"));
            }
            let next = Uuid::new_v4();
            let staging = format!("staging/{}/{next}", identity.user_id);
            let object = format!("objects/{}/{}", identity.user_id, Uuid::new_v4());
            sqlx::query("UPDATE save_uploads SET id=$3,staging_key=$4,object_key=$5,expires_at=now()+interval '24 hours',status='uploading',attempts=0,lease_token=NULL,lease_until=NULL,last_error='' WHERE user_id=$1 AND snapshot_id=$2")
                .bind(identity.user_id).bind(request.snapshot_id).bind(next).bind(&staging).bind(&object).execute(&mut *tx).await?;
            sqlx::query("INSERT INTO save_object_gc(object_key,user_id,bytes,not_before) VALUES($1,$2,$3,now()+interval '25 hours')")
                .bind(&staging).bind(identity.user_id).bind(request.bytes as i64).execute(&mut *tx).await?;
            (next, staging)
        } else {
            sqlx::query("UPDATE save_uploads SET status='uploading',attempts=0,lease_token=NULL,lease_until=NULL,last_error='' WHERE user_id=$1 AND id=$2")
                .bind(identity.user_id).bind(row.get::<Uuid,_>("id")).execute(&mut *tx).await?;
            (
                row.get::<Uuid, _>("id"),
                row.get::<String, _>("staging_key"),
            )
        }
    } else {
        let used = quota_used(&mut tx, identity.user_id).await?;
        if (used as u64)
            .checked_add(request.bytes)
            .filter(|n| *n <= storage.quota)
            .is_none()
        {
            return Err(ApiError::conflict("存档配额不足"));
        }
        let id = Uuid::new_v4();
        let key = format!("staging/{}/{id}", identity.user_id);
        let object = format!("objects/{}/{}", identity.user_id, Uuid::new_v4());
        sqlx::query("INSERT INTO save_uploads(user_id,id,profile_id,device_id,snapshot_id,request,bytes,staging_key,object_key) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)")
            .bind(identity.user_id).bind(id).bind(request.profile_id).bind(identity.device_id).bind(request.snapshot_id)
            .bind(&value).bind(request.bytes as i64).bind(&key).bind(&object).execute(&mut *tx).await?;
        // 这里只预留实际暂存键；每次校验尝试单独预留发布对象容量。
        sqlx::query("INSERT INTO save_object_gc(object_key,user_id,bytes,not_before) VALUES($1,$2,$3,now()+interval '25 hours')")
            .bind(&key).bind(identity.user_id).bind(request.bytes as i64).execute(&mut *tx).await?;
        (id, key)
    };
    tx.commit().await?;
    let (url, headers) = storage.upload_url(&key, request.bytes).await?;
    Ok(Json(
        json!({"status":"uploading","uploadId":id,"uploadUrl":url,"headers":headers}),
    ))
}
// 后台定期调用；失败保留队列。对象删除后才清除 GC 记录，可安全重试。
pub async fn cleanup(app: &App) -> Result<()> {
    let Some(storage) = app.storage.clone() else {
        return Ok(());
    };
    // 只锁 GC 队列行；不持有用户同步锁等待 OSS。未发布对象不允许重新挂接。
    let mut tx = app.pool.begin().await?;
    let keys:Vec<String>=sqlx::query_scalar("SELECT object_key FROM save_object_gc g WHERE not_before<now() AND NOT EXISTS(SELECT 1 FROM save_objects o WHERE o.object_key=g.object_key) AND NOT EXISTS(SELECT 1 FROM save_uploads u WHERE (u.object_key=g.object_key OR u.staging_key=g.object_key) AND NOT u.completed AND u.expires_at>now()) LIMIT 100 FOR UPDATE SKIP LOCKED")
        .fetch_all(&mut *tx).await?;
    for key in keys {
        storage.delete(&key).await?;
        sqlx::query("DELETE FROM save_object_gc WHERE object_key=$1")
            .bind(key)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_unreserved_and_self_parent_uploads() {
        let mut r = UploadRequest {
            snapshot_id: Uuid::new_v4(),
            profile_id: Uuid::new_v4(),
            parent_ids: vec![],
            base_revision: 0,
            manifest_hash: "a".repeat(64),
            archive_hash: "b".repeat(64),
            bytes: 1,
        };
        assert!(validate(&r).is_ok());
        r.bytes = MAX_BYTES + 1;
        assert!(validate(&r).is_err());
        r.bytes = 1;
        r.parent_ids.push(r.snapshot_id);
        assert!(validate(&r).is_err());
    }
}
