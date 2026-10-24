use super::*;
use axum::extract::Query;
use std::collections::{BTreeMap, HashSet};
async fn active(tx: &mut Tx<'_>, user: Uuid) -> Result<Vec<Value>> {
    let rows:Vec<Value>=sqlx::query_scalar("SELECT p.games FROM social_presence p JOIN devices d ON d.user_id=p.user_id AND d.id=p.device_id WHERE p.user_id=$1 AND p.updated_at>now()-interval '90 seconds' AND NOT d.revoked").bind(user).fetch_all(&mut **tx).await?;
    let mut games = BTreeMap::new();
    for row in rows {
        for g in row.as_array().into_iter().flatten() {
            let key = g["key"].as_str().unwrap_or("").to_owned();
            if games
                .get(&key)
                .is_none_or(|old: &Value| old["startedAt"].as_str() < g["startedAt"].as_str())
            {
                games.insert(key, g.clone());
            }
        }
    }
    let mut result: Vec<Value> = games.into_values().collect();
    result.sort_by(|a, b| b["startedAt"].as_str().cmp(&a["startedAt"].as_str()));
    Ok(result)
}
pub(super) async fn status(tx: &mut Tx<'_>, user: Uuid) -> Result<Value> {
    let p = privacy(tx, user).await?;
    if p.invisible {
        return Ok(json!({"online":false,"games":[]}));
    }
    let online:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM social_presence p JOIN devices d ON d.user_id=p.user_id AND d.id=p.device_id WHERE p.user_id=$1 AND p.updated_at>now()-interval '90 seconds' AND NOT d.revoked)").bind(user).fetch_one(&mut **tx).await?;
    let games: Vec<_> = active(tx, user)
        .await?
        .into_iter()
        .filter(|g| !p.hidden_games.iter().any(|k| g["key"] == *k))
        .map(|g| json!({"key":g["key"],"title":g["title"],"startedAt":g["startedAt"]}))
        .collect();
    Ok(json!({"online":online,"games":games}))
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Heartbeat {
    games: Vec<Playing>,
    #[serde(default)]
    offline: bool,
    #[serde(default)]
    announce: bool,
}
pub(super) async fn heartbeat(
    State(app): State<App>,
    i: Identity,
    Json(v): Json<Heartbeat>,
) -> Result<Json<Value>> {
    if v.games.len() > 16
        || v.games.iter().any(|g| !g.valid())
        || v.games.iter().map(|g| &g.key).collect::<HashSet<_>>().len() != v.games.len()
    {
        return Err(ApiError::bad("游戏状态无效"));
    }
    let mut tx = limited(&app, i.user_id, "heartbeat", 60).await?;
    sqlx::query("UPDATE social_event_state SET expired_through=GREATEST(expired_through,COALESCE((SELECT MAX(seq) FROM social_events WHERE created_at<now()-interval '24 hours'),0))").execute(&mut *tx).await?;
    sqlx::query("DELETE FROM social_events WHERE created_at<now()-interval '24 hours'")
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM social_presence WHERE updated_at<now()-interval '1 day'")
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM social_seen_sessions WHERE seen_at<now()-interval '7 days'")
        .execute(&mut *tx)
        .await?;
    let before = active(&mut tx, i.user_id).await?;
    if v.offline {
        sqlx::query("DELETE FROM social_presence WHERE user_id=$1 AND device_id=$2")
            .bind(i.user_id)
            .bind(i.device_id)
            .execute(&mut *tx)
            .await?;
    } else {
        let p = privacy(&mut tx, i.user_id).await?;
        let mut games = vec![];
        for g in v.games {
            let sid = Uuid::parse_str(&g.session_id).map_err(|_| ApiError::bad("会话无效"))?;
            let fresh=sqlx::query("INSERT INTO social_seen_sessions(user_id,device_id,session_id) VALUES($1,$2,$3) ON CONFLICT DO NOTHING").bind(i.user_id).bind(i.device_id).bind(sid).execute(&mut *tx).await?.rows_affected()>0;
            let started = before
                .iter()
                .find(|x| x["key"] == g.key)
                .map(|x| x["startedAt"].clone())
                .unwrap_or(json!(chrono::Utc::now().to_rfc3339()));
            if fresh
                && v.announce
                && !p.invisible
                && !p.hidden_games.contains(&g.key)
                && !before.iter().any(|x| x["key"] == g.key)
            {
                sqlx::query("INSERT INTO social_events(recipient,actor,game_key,game_title) SELECT a.owner,$1,$2,$3 FROM social_relations a JOIN social_relations b ON b.owner=a.peer AND b.peer=a.owner WHERE a.peer=$1 AND a.state='accepted' AND b.state='accepted' AND NOT a.muted").bind(i.user_id).bind(&g.key).bind(&g.title).execute(&mut *tx).await?;
            }
            games.push(
                json!({"key":g.key,"title":g.title,"sessionId":g.session_id,"startedAt":started}),
            );
        }
        sqlx::query("INSERT INTO social_presence(user_id,device_id,games) VALUES($1,$2,$3) ON CONFLICT(user_id,device_id) DO UPDATE SET games=excluded.games,updated_at=now()").bind(i.user_id).bind(i.device_id).bind(json!(games)).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(Json(json!({"ok":true})))
}
#[derive(Deserialize)]
pub(super) struct Cursor {
    cursor: Option<i64>,
}
pub(super) async fn events(
    State(app): State<App>,
    i: Identity,
    Query(q): Query<Cursor>,
) -> Result<Json<Value>> {
    let mut tx = begin(&app, i.user_id).await?;
    let high: i64 = sqlx::query_scalar("SELECT COALESCE(pg_sequence_last_value('social_events_seq_seq'::regclass),0)")
        .fetch_one(&mut *tx)
        .await?;
    let low: i64 = sqlx::query_scalar(
        "SELECT expired_through FROM social_event_state WHERE singleton",
    )
    .fetch_one(&mut *tx)
    .await?;
    let reset = q
        .cursor
        .is_none_or(|c| c < low || c > high || c < 0);
    let mut result = vec![];
    let mut next = high;
    if !reset {
        let rows=sqlx::query("SELECT e.seq,e.actor,e.game_key,e.game_title,p.nickname,p.privacy FROM social_events e JOIN social_profiles p ON p.user_id=e.actor JOIN social_relations a ON a.owner=e.recipient AND a.peer=e.actor JOIN social_relations b ON b.owner=e.actor AND b.peer=e.recipient WHERE e.recipient=$1 AND e.seq>$2 AND e.created_at>now()-interval '60 seconds' AND a.state='accepted' AND b.state='accepted' AND NOT a.muted ORDER BY e.seq LIMIT 100")
 .bind(i.user_id).bind(q.cursor.unwrap_or(high)).fetch_all(&mut *tx).await?;
        if rows.len()==100 {next=rows.last().unwrap().get("seq");}
        for r in rows {
            let p: Privacy = serde_json::from_value(r.get("privacy"))
                .map_err(|_| ApiError::bad("隐私设置无效"))?;
            let key: String = r.get("game_key");
            if p.invisible || p.hidden_games.contains(&key) {
                continue;
            }
            result.push(json!({"id":r.get::<i64,_>("seq"),"actor":r.get::<Uuid,_>("actor"),"nickname":r.get::<String,_>("nickname"),"key":key,"title":r.get::<String,_>("game_title")}));
        }
    }
    tx.commit().await?;
    Ok(Json(json!({"cursor":next,"reset":reset,"events":result})))
}
