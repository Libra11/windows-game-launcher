use super::jobs::Job;
use super::*;
use std::path::Path;
use youji_cloud_protocol::saves::UploadRequest;

// 只有当前租约持有者能发布索引；每次尝试使用不同对象键，旧 worker 无法覆盖新对象。
pub(super) async fn verify(
    app: &App,
    job: &Job,
    work_dir: &Path,
    slots: Arc<tokio::sync::Semaphore>,
) -> Result<Value> {
    let storage = storage(app)?;
    let request: UploadRequest =
        serde_json::from_value(job.request.clone()).map_err(|_| internal())?;
    let permit = slots.acquire_owned().await.map_err(|_| internal())?;
    let temp = tempfile::Builder::new()
        .prefix("youji-job-")
        .tempdir_in(work_dir)
        .map_err(|_| internal())?;
    let archive = temp.path().join("archive.zip");
    storage
        .fetch(&job.staging_key, &archive, request.bytes)
        .await?;
    let path = archive.clone();
    let (manifest, hash, _temp) = tokio::task::spawn_blocking(move || {
        // 取消异步等待不会终止阻塞任务；许可与临时目录随真正的校验任务存活。
        let _permit = permit;
        let hash = youji_save_archive::file_hash(&path)?;
        let manifest = youji_save_archive::verify(&path)?;
        Ok::<_, String>((manifest, hash, temp))
    })
    .await
    .map_err(|_| internal())?
    .map_err(ApiError::bad)?;
    let manifest_hash = youji_cloud_protocol::digest(&manifest).map_err(ApiError::bad)?;
    if hash != request.archive_hash || manifest_hash != request.manifest_hash {
        return Err(ApiError::bad("存档哈希与上传声明不一致"));
    }
    let key = job.object_key.clone();
    // 已校验内容先查重，避免每个相同快照都产生完整 OSS 副本。
    let duplicate:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM save_objects WHERE user_id=$1 AND archive_hash=$2 AND manifest_hash=$3 AND bytes=$4 AND available)")
        .bind(job.user_id).bind(&hash).bind(&manifest_hash).bind(request.bytes as i64).fetch_one(&app.pool).await?;
    let published = !duplicate;
    if published {
        storage.publish(&key, &archive, request.bytes).await?;
    }
    let mut tx = lock(&app, job.user_id).await?;
    let current =
        sqlx::query("SELECT completed,expires_at,lease_token,lease_until FROM save_uploads WHERE user_id=$1 AND id=$2 FOR UPDATE")
            .bind(job.user_id)
            .bind(job.id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(missing)?;
    if current.get::<Option<Uuid>, _>("lease_token") != Some(job.token)
        || current
            .get::<Option<chrono::DateTime<chrono::Utc>>, _>("lease_until")
            .is_none_or(|until| until <= chrono::Utc::now())
    {
        return Err(ApiError::conflict("校验任务租约已失效"));
    }
    if current.get::<bool, _>("completed") {
        let result = snapshot(&mut tx, job.user_id, request.snapshot_id).await?;
        tx.commit().await?;
        return Ok(result);
    }
    if current.get::<chrono::DateTime<chrono::Utc>, _>("expires_at") <= chrono::Utc::now() {
        return Err(ApiError::conflict("上传已过期"));
    }
    parents(
        &mut tx,
        job.user_id,
        request.profile_id,
        &request.parent_ids,
    )
    .await?;
    let existing:Option<Uuid>=sqlx::query_scalar("SELECT id FROM save_objects WHERE user_id=$1 AND archive_hash=$2 AND manifest_hash=$3 AND bytes=$4 AND available LIMIT 1")
        .bind(job.user_id).bind(&hash).bind(&manifest_hash).bind(request.bytes as i64).fetch_optional(&mut *tx).await?;
    let object_id = if let Some(existing) = existing {
        if !published {
            // 此次尝试从未写入对象，可立即释放未使用的发布预留。
            sqlx::query("DELETE FROM save_object_gc WHERE object_key=$1 AND user_id=$2")
                .bind(&key)
                .bind(job.user_id)
                .execute(&mut *tx)
                .await?;
        }
        // 若并发尝试先发布，当前实际上传的副本继续计费，直到 GC 物理删除。
        existing
    } else {
        if !published {
            // 查重对象在最终用户锁之前被删除，不能挂接不存在的内容。
            return Err(ApiError(
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                "查重对象已变化，请重试校验".into(),
            ));
        }
        let object_id = Uuid::new_v4();
        sqlx::query("INSERT INTO save_objects(user_id,id,object_key,archive_hash,manifest_hash,bytes,manifest) VALUES($1,$2,$3,$4,$5,$6,$7)")
            .bind(job.user_id).bind(object_id).bind(&key).bind(hash).bind(manifest_hash)
            .bind(request.bytes as i64).bind(serde_json::to_value(&manifest).map_err(|_|internal())?).execute(&mut *tx).await?;
        sqlx::query("DELETE FROM save_object_gc WHERE object_key=$1")
            .bind(key)
            .execute(&mut *tx)
            .await?;
        object_id
    };
    sqlx::query("INSERT INTO save_snapshots(user_id,id,profile_id,device_id,object_id) VALUES($1,$2,$3,$4,$5)")
        .bind(job.user_id).bind(request.snapshot_id).bind(request.profile_id).bind(job.device_id).bind(object_id).execute(&mut *tx).await?;
    add_parents(
        &mut tx,
        job.user_id,
        request.profile_id,
        request.snapshot_id,
        &request.parent_ids,
    )
    .await?;
    let p = profile(&mut tx, job.user_id, request.profile_id).await?;
    if advances(
        p.get("selected_snapshot_id"),
        p.get("revision"),
        request.base_revision,
        &request.parent_ids,
    ) {
        sqlx::query("UPDATE save_profiles SET selected_snapshot_id=$3,revision=revision+1 WHERE user_id=$1 AND id=$2")
            .bind(job.user_id).bind(request.profile_id).bind(request.snapshot_id).execute(&mut *tx).await?;
    }
    sqlx::query("UPDATE save_uploads SET completed=true,status='completed',lease_token=NULL,lease_until=NULL,last_error='' WHERE user_id=$1 AND id=$2")
        .bind(job.user_id)
        .bind(job.id)
        .execute(&mut *tx)
        .await?;
    let result = snapshot(&mut tx, job.user_id, request.snapshot_id).await?;
    tx.commit().await?;
    Ok(result)
}
