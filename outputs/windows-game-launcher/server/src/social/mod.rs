mod avatar;
mod covers;
mod presence;
mod space;
#[cfg(test)]
mod tests;
use crate::{auth::Identity, ApiError, App, Result};
use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;
use youji_cloud_protocol::social::{Playing, Privacy};
type Tx<'a> = Transaction<'a, Postgres>;
pub fn routes() -> Router<App> {
    Router::new()
        .route("/v1/social/covers", get(covers::manifest).post(covers::upload).layer(axum::extract::DefaultBodyLimit::max(12*1024*1024)))
        .route("/v1/social/covers/{id}", get(covers::read))
        .route("/v1/social/me", get(me).post(save_me))
        .route("/v1/social/friends", get(friends))
        .route("/v1/social/requests", post(request))
        .route("/v1/social/friends/{id}", post(relation))
        .route("/v1/social/presence", post(presence::heartbeat))
        .route("/v1/social/events", get(presence::events))
        .route("/v1/social/spaces/{id}", get(space::space))
        .route("/v1/social/avatar", post(avatar::upload))
        .route("/v1/social/avatar/{id}", get(avatar::read))
}
async fn begin(app: &App, user: Uuid) -> Result<Tx<'_>> {
    let mut tx = app.pool.begin().await?;
    // Social writes and event cursors share commit order; no sequence holes on concurrent devices.
    sqlx::query("SELECT pg_advisory_xact_lock(674321009)")
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO social_profiles(user_id,friend_code,nickname) VALUES($1,$2,$3) ON CONFLICT(user_id) DO NOTHING")
 .bind(user).bind(user.simple().to_string()).bind(format!("玩家{}",&user.simple().to_string()[..6])).execute(&mut *tx).await?;
    Ok(tx)
}
async fn rate(tx: &mut Tx<'_>, user: Uuid, kind: &str, max: i32) -> Result<()> {
    let minute = chrono::Utc::now().timestamp() / 60;
    let n:i32=sqlx::query_scalar("INSERT INTO social_rate_limits VALUES($1,$2,$3,1) ON CONFLICT(user_id,kind) DO UPDATE SET minute=excluded.minute,count=CASE WHEN social_rate_limits.minute=excluded.minute THEN social_rate_limits.count+1 ELSE 1 END RETURNING count")
 .bind(user).bind(kind).bind(minute).fetch_one(&mut **tx).await?;
    if n > max {
        return Err(ApiError(
            axum::http::StatusCode::TOO_MANY_REQUESTS,
            "操作过于频繁，请稍后重试".into(),
        ));
    }
    Ok(())
}
async fn limited<'a>(app: &'a App, user: Uuid, kind: &str, max: i32) -> Result<Tx<'a>> {
    let mut tx = begin(app, user).await?;
    let result = rate(&mut tx, user, kind, max).await;
    tx.commit().await?;
    result?;
    begin(app, user).await
}
async fn allowed(tx: &mut Tx<'_>, viewer: Uuid, owner: Uuid) -> Result<()> {
    if viewer == owner {
        return Ok(());
    }
    let yes:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM social_relations a JOIN social_relations b ON a.owner=b.peer AND a.peer=b.owner WHERE a.owner=$1 AND a.peer=$2 AND a.state='accepted' AND b.state='accepted')")
 .bind(viewer).bind(owner).fetch_one(&mut **tx).await?;
    if !yes {
        return Err(ApiError(
            axum::http::StatusCode::NOT_FOUND,
            "空间不可访问".into(),
        ));
    }
    Ok(())
}
async fn privacy(tx: &mut Tx<'_>, user: Uuid) -> Result<Privacy> {
    let v: Value = sqlx::query_scalar("SELECT privacy FROM social_profiles WHERE user_id=$1")
        .bind(user)
        .fetch_one(&mut **tx)
        .await?;
    serde_json::from_value(v).map_err(|_| ApiError::bad("隐私设置无效"))
}
async fn profile(tx: &mut Tx<'_>, user: Uuid) -> Result<Value> {
    let r = sqlx::query("SELECT user_id,nickname,bio,avatar FROM social_profiles WHERE user_id=$1")
        .bind(user)
        .fetch_one(&mut **tx)
        .await?;
    Ok(
        json!({"id":r.get::<Uuid,_>("user_id"),"nickname":r.get::<String,_>("nickname"),"bio":r.get::<String,_>("bio"),"avatar":r.get::<String,_>("avatar")}),
    )
}
async fn me(State(app): State<App>, i: Identity) -> Result<Json<Value>> {
    let mut tx = begin(&app, i.user_id).await?;
    let mut v = profile(&mut tx, i.user_id).await?;
    v["privacy"] = serde_json::to_value(privacy(&mut tx, i.user_id).await?).unwrap();
    v["friendCode"] = json!(
        sqlx::query_scalar::<_, String>("SELECT friend_code FROM social_profiles WHERE user_id=$1")
            .bind(i.user_id)
            .fetch_one(&mut *tx)
            .await?
    );
    tx.commit().await?;
    Ok(Json(v))
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Edit {
    nickname: String,
    bio: String,
    avatar: String,
    privacy: Privacy,
}
async fn save_me(State(app): State<App>, i: Identity, Json(e): Json<Edit>) -> Result<Json<Value>> {
    if e.nickname.trim().is_empty()
        || e.nickname.chars().count() > 40
        || e.nickname.chars().any(char::is_control)
        || e.bio.chars().count() > 500
        || e.bio.chars().any(|c| c.is_control() && c != '\n')
        || e.privacy.hidden_games.len() > 5000
        || e.privacy
            .hidden_games
            .iter()
            .any(|s| s.len() > 300 || s.chars().any(char::is_control))
        || !["builtin:leaf", "builtin:moon", "builtin:star", "custom"].contains(&e.avatar.as_str())
    {
        return Err(ApiError::bad("个人资料格式无效"));
    }
    let mut tx = limited(&app, i.user_id, "profile", 30).await?;
    if e.avatar == "custom" {
        let old = profile(&mut tx, i.user_id).await?;
        if old["avatar"] != "custom" {
            return Err(ApiError::bad("请先上传头像"));
        }
    }
    sqlx::query("UPDATE social_profiles SET nickname=$2,bio=$3,avatar=$4,privacy=$5,updated_at=now() WHERE user_id=$1").bind(i.user_id).bind(e.nickname.trim()).bind(e.bio).bind(e.avatar).bind(json!(e.privacy)).execute(&mut *tx).await?;
    // Pending notifications must not reveal a game after it is hidden or invisibility is enabled.
    sqlx::query("DELETE FROM social_events WHERE actor=$1 AND ($2 OR game_key=ANY($3))")
        .bind(i.user_id)
        .bind(e.privacy.invisible)
        .bind(&e.privacy.hidden_games)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Json(json!({"ok":true})))
}
async fn friends(State(app): State<App>, i: Identity) -> Result<Json<Value>> {
    let mut tx = begin(&app, i.user_id).await?;
    let rows =
        sqlx::query("SELECT peer,state,muted FROM social_relations WHERE owner=$1 ORDER BY peer")
            .bind(i.user_id)
            .fetch_all(&mut *tx)
            .await?;
    let mut result = vec![];
    for r in rows {
        let id: Uuid = r.get("peer");
        let mut p = profile(&mut tx, id).await?;
        let state: String = r.get("state");
        p["relation"] = json!(state);
        p["muted"] = json!(r.get::<bool, _>("muted"));
        if state == "accepted" {
            p["presence"] = presence::status(&mut tx, id).await?;
        }
        result.push(p);
    }
    tx.commit().await?;
    Ok(Json(json!({"friends":result})))
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Request {
    code: String,
}
async fn request(
    State(app): State<App>,
    i: Identity,
    Json(v): Json<Request>,
) -> Result<Json<Value>> {
    let mut tx = limited(&app, i.user_id, "request", 10).await?;
    let peer: Option<Uuid> =
        sqlx::query_scalar("SELECT user_id FROM social_profiles WHERE friend_code=$1")
            .bind(v.code.trim().to_lowercase())
            .fetch_optional(&mut *tx)
            .await?;
    let peer = peer
        .filter(|p| *p != i.user_id)
        .ok_or_else(|| ApiError::bad("好友码无效或不能添加自己"))?;
    let blocked:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM social_relations WHERE ((owner=$1 AND peer=$2) OR (owner=$2 AND peer=$1)) AND state='blocked')").bind(i.user_id).bind(peer).fetch_one(&mut *tx).await?;
    if blocked {
        return Err(ApiError::bad("无法发送申请"));
    }
    for (a, b, s) in [(i.user_id, peer, "outgoing"), (peer, i.user_id, "incoming")] {
        sqlx::query("INSERT INTO social_relations(owner,peer,state) VALUES($1,$2,$3) ON CONFLICT DO NOTHING").bind(a).bind(b).bind(s).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(Json(json!({"ok":true})))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Action {
    action: String,
}
async fn relation(
    State(app): State<App>,
    i: Identity,
    Path(peer): Path<Uuid>,
    Json(v): Json<Action>,
) -> Result<Json<Value>> {
    if peer == i.user_id {
        return Err(ApiError::bad("不能操作自己"));
    }
    let mut tx = limited(&app, i.user_id, "relation", 60).await?;
    let state: Option<String> =
        sqlx::query_scalar("SELECT state FROM social_relations WHERE owner=$1 AND peer=$2")
            .bind(i.user_id)
            .bind(peer)
            .fetch_optional(&mut *tx)
            .await?;
    match v.action.as_str() {
        "accept" => {
            if state.as_deref() != Some("incoming") {
                return Err(ApiError::bad("申请已失效"));
            }
            sqlx::query("UPDATE social_relations SET state='accepted' WHERE (owner=$1 AND peer=$2) OR (owner=$2 AND peer=$1)").bind(i.user_id).bind(peer).execute(&mut *tx).await?;
        }
        "reject" | "remove" => {
            sqlx::query("DELETE FROM social_relations WHERE ((owner=$1 AND peer=$2) OR (owner=$2 AND peer=$1)) AND state<>'blocked'").bind(i.user_id).bind(peer).execute(&mut *tx).await?;
        }
        "block" => {
            if state.is_none() {
                return Err(ApiError::bad("好友不存在"));
            }
            sqlx::query(
                "DELETE FROM social_relations WHERE owner=$2 AND peer=$1 AND state<>'blocked'",
            )
            .bind(i.user_id)
            .bind(peer)
            .execute(&mut *tx)
            .await?;
            sqlx::query("UPDATE social_relations SET state='blocked' WHERE owner=$1 AND peer=$2")
                .bind(i.user_id)
                .bind(peer)
                .execute(&mut *tx)
                .await?;
        }
        "unblock" => {
            sqlx::query(
                "DELETE FROM social_relations WHERE owner=$1 AND peer=$2 AND state='blocked'",
            )
            .bind(i.user_id)
            .bind(peer)
            .execute(&mut *tx)
            .await?;
        }
        "mute" | "unmute" => {
            allowed(&mut tx, i.user_id, peer).await?;
            sqlx::query("UPDATE social_relations SET muted=$3 WHERE owner=$1 AND peer=$2")
                .bind(i.user_id)
                .bind(peer)
                .bind(v.action == "mute")
                .execute(&mut *tx)
                .await?;
        }
        _ => return Err(ApiError::bad("不支持的好友操作")),
    }
    sqlx::query("DELETE FROM social_events WHERE (actor=$1 AND recipient=$2) OR (actor=$2 AND recipient=$1)").bind(i.user_id).bind(peer).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Json(json!({"ok":true})))
}
