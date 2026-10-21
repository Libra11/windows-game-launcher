use super::*;
use axum::extract::Path;

pub async fn list_snapshots(
    State(app): State<App>,
    identity: Identity,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    let mut tx = lock(&app, identity.user_id).await?;
    let p = profile(&mut tx, identity.user_id, id).await?;
    let ids:Vec<Uuid>=sqlx::query_scalar("SELECT id FROM save_snapshots WHERE user_id=$1 AND profile_id=$2 ORDER BY created_at DESC,id")
        .bind(identity.user_id).bind(id).fetch_all(&mut *tx).await?;
    let mut snapshots = Vec::new();
    for id in ids {
        snapshots.push(snapshot(&mut tx, identity.user_id, id).await?);
    }
    tx.commit().await?;
    Ok(Json(
        json!({"profile":profile_json(&p),"snapshots":snapshots}),
    ))
}
pub async fn download(
    State(app): State<App>,
    identity: Identity,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    let storage = storage(&app)?;
    let key:Option<String>=sqlx::query_scalar("SELECT o.object_key FROM save_snapshots s JOIN save_objects o ON s.user_id=o.user_id AND s.object_id=o.id WHERE s.user_id=$1 AND s.id=$2")
        .bind(identity.user_id).bind(id).fetch_optional(&app.pool).await?;
    Ok(Json(
        json!({"url":storage.download_url(&key.ok_or_else(missing)?).await?}),
    ))
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Resolve {
    pub(super) snapshot_id: Uuid,
    pub(super) base_revision: i64,
    pub(super) parent_ids: Vec<Uuid>,
}
pub async fn resolve(
    State(app): State<App>,
    identity: Identity,
    Path(id): Path<Uuid>,
    Json(request): Json<Resolve>,
) -> Result<Json<Value>> {
    let mut tx = lock(&app, identity.user_id).await?;
    let p = profile(&mut tx, identity.user_id, id).await?;
    if p.get::<i64, _>("revision") != request.base_revision {
        return Err(ApiError::conflict("档案已变化，请刷新后重新选择"));
    }
    let selected: Option<Uuid> = p.get("selected_snapshot_id");
    if !request.parent_ids.contains(&request.snapshot_id)
        || selected.is_some_and(|v| !request.parent_ids.contains(&v))
    {
        return Err(ApiError::bad("解决分叉必须包含所选内容和当前版本"));
    }
    parents(&mut tx, identity.user_id, id, &request.parent_ids).await?;
    let object: Uuid = sqlx::query_scalar(
        "SELECT object_id FROM save_snapshots WHERE user_id=$1 AND profile_id=$2 AND id=$3",
    )
    .bind(identity.user_id)
    .bind(id)
    .bind(request.snapshot_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(missing)?;
    let new_id = Uuid::new_v4();
    sqlx::query("INSERT INTO save_snapshots(user_id,id,profile_id,device_id,object_id) VALUES($1,$2,$3,$4,$5)")
        .bind(identity.user_id).bind(new_id).bind(id).bind(identity.device_id).bind(object).execute(&mut *tx).await?;
    add_parents(&mut tx, identity.user_id, id, new_id, &request.parent_ids).await?;
    sqlx::query("UPDATE save_profiles SET selected_snapshot_id=$3,revision=revision+1 WHERE user_id=$1 AND id=$2")
        .bind(identity.user_id).bind(id).bind(new_id).execute(&mut *tx).await?;
    let result = snapshot(&mut tx, identity.user_id, new_id).await?;
    tx.commit().await?;
    Ok(Json(result))
}
pub async fn delete_snapshot(
    State(app): State<App>,
    identity: Identity,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    let mut tx = lock(&app, identity.user_id).await?;
    let row = sqlx::query("SELECT object_id FROM save_snapshots WHERE user_id=$1 AND id=$2")
        .bind(identity.user_id)
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
    let Some(row) = row else {
        tx.commit().await?;
        return Ok(Json(json!({"deleted":true})));
    };
    let referenced:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM save_profiles WHERE user_id=$1 AND selected_snapshot_id=$2) OR EXISTS(SELECT 1 FROM save_snapshot_parents WHERE user_id=$1 AND parent_id=$2) OR EXISTS(SELECT 1 FROM save_uploads WHERE user_id=$1 AND NOT completed AND expires_at>now() AND request->'parentIds' @> $3::jsonb)")
        .bind(identity.user_id).bind(id).bind(json!([id])).fetch_one(&mut *tx).await?;
    if referenced {
        return Err(ApiError::conflict("当前版本、父版本或待上传引用不能删除"));
    }
    let object: Uuid = row.get("object_id");
    sqlx::query("DELETE FROM save_snapshot_parents WHERE user_id=$1 AND snapshot_id=$2")
        .bind(identity.user_id)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM save_snapshots WHERE user_id=$1 AND id=$2")
        .bind(identity.user_id)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    // 完成幂等记录保留；再次 complete 已删除的快照返回不存在，不会重新创建。
    let object=sqlx::query("DELETE FROM save_objects o WHERE user_id=$1 AND id=$2 AND NOT EXISTS(SELECT 1 FROM save_snapshots s WHERE s.user_id=o.user_id AND s.object_id=o.id) RETURNING object_key,bytes")
        .bind(identity.user_id).bind(object).fetch_optional(&mut *tx).await?;
    if let Some(object) = object {
        let key: String = object.get("object_key");
        let bytes: i64 = object.get("bytes");
        // 已签发的下载地址最多十分钟；延后物理删除避免截断正在下载的历史版本。
        sqlx::query("INSERT INTO save_object_gc(object_key,user_id,bytes,not_before) VALUES($1,$2,$3,now()+interval '15 minutes') ON CONFLICT(object_key) DO UPDATE SET user_id=excluded.user_id,bytes=excluded.bytes")
            .bind(key).bind(identity.user_id).bind(bytes).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(Json(json!({"deleted":true})))
}
