use super::*;
use axum::extract::Path;
use youji_cloud_protocol::save_policy::{retained, DirectoryRule, Version};
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Settings {
    directory_rule: Option<DirectoryRule>,
    auto_retention: bool,
}
pub async fn settings(
    State(app): State<App>,
    identity: Identity,
    Path(id): Path<Uuid>,
    Json(input): Json<Settings>,
) -> Result<Json<Value>> {
    if let Some(rule) = &input.directory_rule {
        rule.validate().map_err(ApiError::bad)?;
    }
    let mut tx = lock(&app, identity.user_id).await?;
    profile(&mut tx, identity.user_id, id).await?;
    let rule = input
        .directory_rule
        .map(serde_json::to_value)
        .transpose()
        .map_err(|_| internal())?;
    let row=sqlx::query("UPDATE save_profiles SET directory_rule=$3,auto_retention=$4 WHERE user_id=$1 AND id=$2 RETURNING *")
        .bind(identity.user_id).bind(id).bind(rule).bind(input.auto_retention).fetch_one(&mut *tx).await?;
    tx.commit().await?;
    Ok(Json(profile_json(&row)))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pin {
    pinned: bool,
}
pub async fn pin(
    State(app): State<App>,
    identity: Identity,
    Path(id): Path<Uuid>,
    Json(input): Json<Pin>,
) -> Result<Json<Value>> {
    let mut tx = lock(&app, identity.user_id).await?;
    let row=sqlx::query("UPDATE save_snapshots s SET pinned=$3 WHERE user_id=$1 AND id=$2 AND EXISTS(SELECT 1 FROM save_objects o WHERE o.user_id=s.user_id AND o.id=s.object_id AND o.available)")
        .bind(identity.user_id).bind(id).bind(input.pinned).execute(&mut *tx).await?;
    if row.rows_affected() != 1 {
        return Err(ApiError::bad("快照不存在或归档已清理"));
    }
    tx.commit().await?;
    Ok(Json(json!({"pinned":input.pinned})))
}
pub async fn prune(
    State(app): State<App>,
    identity: Identity,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    let mut tx = lock(&app, identity.user_id).await?;
    profile(&mut tx, identity.user_id, id).await?;
    let count = prune_locked(&mut tx, identity.user_id).await?;
    tx.commit().await?;
    Ok(Json(json!({"reclaimedObjects":count})))
}
pub(super) async fn prune_locked(tx: &mut Transaction<'_, Postgres>, user: Uuid) -> Result<u64> {
    let rows=sqlx::query("SELECT s.*,o.available,p.auto_retention,(s.pinned OR p.selected_snapshot_id=s.id OR NOT EXISTS(SELECT 1 FROM save_snapshot_parents e WHERE e.user_id=s.user_id AND e.parent_id=s.id) OR EXISTS(SELECT 1 FROM save_uploads u WHERE u.user_id=s.user_id AND NOT u.completed AND u.expires_at>now() AND (u.snapshot_id=s.id OR u.request->'parentIds' @> jsonb_build_array(s.id)))) AS protected FROM save_snapshots s JOIN save_profiles p ON p.user_id=s.user_id AND p.id=s.profile_id JOIN save_objects o ON o.user_id=s.user_id AND o.id=s.object_id WHERE s.user_id=$1")
        .bind(user).fetch_all(&mut **tx).await?;
    let mut groups = std::collections::HashMap::<Uuid, Vec<Version>>::new();
    for r in &rows {
        if !r.get::<bool, _>("available") {
            continue;
        }
        groups
            .entry(r.get("profile_id"))
            .or_default()
            .push(Version {
                id: r.get::<Uuid, _>("id").to_string(),
                unix_micros: r
                    .get::<chrono::DateTime<chrono::Utc>, _>("created_at")
                    .timestamp_micros(),
                protected: !r.get::<bool, _>("auto_retention")
                    || r.get::<Option<bool>, _>("protected").unwrap_or(false),
            });
    }
    let kept: std::collections::HashSet<String> = groups
        .values()
        .flat_map(|v| retained(v, chrono::Utc::now().timestamp_micros()))
        .collect();
    let mut protected_objects = std::collections::HashSet::new();
    let mut candidates = std::collections::HashSet::new();
    for r in &rows {
        let object: Uuid = r.get("object_id");
        if kept.contains(&r.get::<Uuid, _>("id").to_string()) {
            protected_objects.insert(object);
        } else if r.get::<bool, _>("available") {
            candidates.insert(object);
        }
    }
    let mut count = 0;
    for object in candidates.difference(&protected_objects) {
        let row=sqlx::query("UPDATE save_objects SET available=false WHERE user_id=$1 AND id=$2 AND available RETURNING object_key,bytes").bind(user).bind(object).fetch_optional(&mut **tx).await?;
        if let Some(r) = row {
            sqlx::query("INSERT INTO save_object_gc(object_key,user_id,bytes,not_before) VALUES($1,$2,$3,now()+interval '15 minutes') ON CONFLICT(object_key) DO NOTHING")
                .bind(r.get::<String,_>("object_key")).bind(user).bind(r.get::<i64,_>("bytes")).execute(&mut **tx).await?;
            count += 1;
        }
    }
    Ok(count)
}
