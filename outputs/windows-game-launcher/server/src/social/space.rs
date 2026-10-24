use super::*;
use axum::extract::Query;
use std::collections::BTreeMap;
#[derive(Deserialize)]
pub(super) struct Page {
    #[serde(default)]
    offset: usize,
    game: Option<String>,
}
pub(super) fn project(rows: &[(String, String, Value)], p: &Privacy, own: bool) -> Vec<Value> {
    let mut result = vec![];
    for (_, id, g) in rows.iter().filter(|r| r.0 == "game") {
        let key = format!(
            "{}:{}",
            g["source"].as_str().unwrap_or(""),
            g["sourceIdentity"].as_str().unwrap_or("")
        );
        if !own && p.hidden_games.contains(&key) {
            continue;
        }
        let value = |kind: &str| {
            rows.iter()
                .find(|r| r.0 == kind && r.1 == *id)
                .map(|r| &r.2)
        };
        let title = value("title")
            .and_then(|v| v["value"].as_str())
            .or(g["displayTitle"].as_str())
            .unwrap_or("未命名游戏");
        let meta = value("metadata_reference")
            .and_then(|v| v["value"].as_str())
            .or(g["metadataReference"].as_str())
            .unwrap_or("");
        let cover = if !meta.is_empty() && meta.bytes().all(|c| c.is_ascii_digit()) {
            format!("https://cdn.cloudflare.steamstatic.com/steam/apps/{meta}/library_600x900.jpg")
        } else {
            String::new()
        };
        let ns = value("achievement_reference")
            .and_then(|v| v["value"].as_str())
            .or(g["achievementReference"].as_str())
            .unwrap_or("");
        let mut local = value("historical_baseline")
            .and_then(|v| v["seconds"].as_u64())
            .unwrap_or(0);
        let mut last = String::new();
        for (_, _, v) in rows
            .iter()
            .filter(|r| r.0 == "play_session" && r.2["gameId"] == *id)
        {
            local = local.saturating_add(v["seconds"].as_u64().unwrap_or(0));
            let end = v["endedAt"]
                .as_str()
                .or(v["startedAt"].as_str())
                .unwrap_or("");
            if end > last.as_str() {
                last = end.into();
            }
        }
        let official = value("official_playtime").and_then(|v| v["seconds"].as_u64());
        let mut unlocked: BTreeMap<String, (&str, Value)> = BTreeMap::new();
        if own || p.achievements {
            for (kind, _, v) in rows.iter().filter(|r| {
                ["manual_unlock", "automatic_unlock"].contains(&r.0.as_str())
                    && r.2["gameId"] == *id
                    && r.2["namespace"] == ns
                    && r.2["present"] == true
            }) {
                let api = v["apiName"].as_str().unwrap_or("").to_string();
                let manual = kind == "manual_unlock";
                if !manual || !unlocked.contains_key(&api) {
                    unlocked.insert(
                        api,
                        (
                            if manual { "manual" } else { "automatic" },
                            v["unlockedAt"].clone(),
                        ),
                    );
                }
            }
            if let Some(v) = value("official_snapshot").filter(|v| v["namespace"] == ns) {
                if let Some(m) = v["unlocks"].as_object() {
                    for (api, t) in m {
                        unlocked.insert(api.clone(), ("official", t.clone()));
                    }
                }
            }
        }
        let defs = value("definition_cache")
            .filter(|v| v["namespace"] == ns)
            .and_then(|v| v["definitions"].as_array());
        let mut achievements = vec![];
        if own || p.achievements {
            for d in defs.into_iter().flatten() {
                let a = d["apiName"].as_str().unwrap_or("");
                let u = unlocked.get(a);
                let secret = d["hidden"] == true && u.is_none();
                achievements.push(json!({"name":if secret{"隐藏成就"}else{d["name"].as_str().unwrap_or(a)},"description":if secret{"解锁后显示"}else{d["description"].as_str().unwrap_or("")},"icon":if secret {""} else {d["icon"].as_str().filter(|url| url.starts_with("https://")).unwrap_or("")},"unlocked":u.is_some(),"source":u.map(|x|x.0),"unlockedAt":u.map(|x|&x.1)}));
            }
        }
        result.push(json!({"key":key,"title":title,"source":g["source"],"cover":cover,"hidden":p.hidden_games.contains(&key),"lastPlayed":if last.is_empty(){None}else{Some(last)},"localSeconds":if own||p.playtime{Some(local)}else{None},"officialSeconds":if own||p.playtime{official}else{None},"achievementCount":if own||p.achievements{Some(achievements.len())}else{None},"unlockedCount":if own||p.achievements{Some(unlocked.len())}else{None},"achievements":if own||p.achievements{Some(achievements)}else{None}}));
    }
    result.sort_by(|a, b| a["title"].as_str().cmp(&b["title"].as_str()));
    result
}
pub(super) async fn space(
    State(app): State<App>,
    i: Identity,
    Path(owner): Path<Uuid>,
    Query(q): Query<Page>,
) -> Result<Json<Value>> {
    let mut tx = begin(&app, i.user_id).await?;
    allowed(&mut tx, i.user_id, owner).await?;
    let p = privacy(&mut tx, owner).await?;
    let own = owner == i.user_id;
    let profile = profile(&mut tx, owner).await?;
    let presence = presence::status(&mut tx, owner).await?;
    let rows=sqlx::query("SELECT kind,id,payload,updated_at FROM sync_resources WHERE user_id=$1 AND NOT deleted AND kind=ANY($2)")
 .bind(owner).bind(vec!["game","title","metadata_reference","achievement_reference","historical_baseline","play_session","official_playtime","automatic_unlock","manual_unlock","official_snapshot","definition_cache"]).fetch_all(&mut *tx).await?;
    let updated = rows
        .iter()
        .map(|r| r.get::<chrono::DateTime<chrono::Utc>, _>("updated_at"))
        .max();
    let initialized: bool = sqlx::query_scalar(
        "SELECT COALESCE((SELECT initialized FROM user_sync_state WHERE user_id=$1),false)",
    )
    .bind(owner)
    .fetch_one(&mut *tx)
    .await?;
    let data = rows
        .into_iter()
        .map(|r| (r.get("kind"), r.get("id"), r.get("payload")))
        .collect::<Vec<_>>();
    let mut games = project(&data, &p, own);
    let covers=sqlx::query("SELECT game_key,source_hash FROM social_covers WHERE user_id=$1").bind(owner).fetch_all(&mut *tx).await?;
    for g in &mut games { if let Some(r)=covers.iter().find(|r|g["key"].as_str()==Some(r.get::<String,_>("game_key").as_str())) {g["coverRevision"]=json!(r.get::<String,_>("source_hash"));} }

    let recent: Vec<_> = games
        .iter()
        .filter(|g| !g["lastPlayed"].is_null())
        .cloned()
        .collect();
    let mut recent = recent;
    recent.sort_by(|a, b| b["lastPlayed"].as_str().cmp(&a["lastPlayed"].as_str()));
    recent.truncate(10);
    let game_detail = q
        .game
        .as_ref()
        .and_then(|key| games.iter().find(|g| g["key"] == *key))
        .cloned()
        .filter(|_| own || p.library);
    for g in games.iter_mut().chain(recent.iter_mut()) {
        g.as_object_mut().unwrap().remove("achievements");
    }
    let count = games.len();
    let local_total: u64 = games
        .iter()
        .filter_map(|g| g["localSeconds"].as_u64())
        .sum();
    let page = if own || p.library {
        games
            .into_iter()
            .skip(q.offset.min(count))
            .take(24)
            .collect::<Vec<_>>()
    } else {
        vec![]
    };
    tx.commit().await?;
    Ok(Json(
        json!({"profile":profile,"presence":presence,"initialized":initialized,"updatedAt":updated,"libraryVisible":own||p.library,"total":if own||p.library{Some(count)}else{None},"localSeconds":if own||p.playtime{Some(local_total)}else{None},"recent":recent,"games":page,"nextOffset":if (own||p.library)&&q.offset+24<count{Some(q.offset+24)}else{None},"game":game_detail}),
    ))
}
