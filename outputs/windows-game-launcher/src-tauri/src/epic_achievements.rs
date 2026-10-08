use crate::{
    db, epic_auth,
    epic_library::Error,
    lock_db,
    model::{AchievementDefinition, Game},
    AppState,
};
use serde_json::Value;
use std::collections::HashSet;
use tauri::Manager;

pub(crate) const SCHEMA: &str = "Epic 官方成就";
pub(crate) const EMPTY_SCHEMA: &str = "Epic 官方成就（暂无成就）";
const DEFINITIONS: &str = r#"query Achievement($sandboxId: String!, $locale: String!) {
 Achievement { productAchievementsRecordBySandbox(sandboxId: $sandboxId, locale: $locale) {
 sandboxId totalAchievements achievements { achievement {
 name hidden unlockedDisplayName lockedDisplayName unlockedDescription lockedDescription
 unlockedIconLink lockedIconLink XP tier { name } rarity { percent }
 } } } } }"#;
const PLAYER: &str = r#"query PlayerAchievement($epicAccountId: String!, $sandboxId: String!) {
 PlayerAchievement { playerAchievementGameRecordsBySandbox(epicAccountId: $epicAccountId, sandboxId: $sandboxId) {
 records { playerAchievements { playerAchievement {
 sandboxId epicAccountId unlocked unlockDate achievementName
 } } } } } }"#;

struct Snapshot {
    definitions: Vec<AchievementDefinition>,
    details: Value,
    unlocks: Vec<(String, String)>,
}
fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or("").trim()
}
fn response(data: &Value) -> Result<(), String> {
    if data
        .get("errors")
        .and_then(Value::as_array)
        .is_some_and(|errors| !errors.is_empty())
    {
        return Err("Epic 成就接口返回错误，请检查账号授权或稍后重试".into());
    }
    Ok(())
}
fn definitions(data: &Value, namespace: &str) -> Result<Snapshot, String> {
    response(data)?;
    let record = data
        .pointer("/data/Achievement/productAchievementsRecordBySandbox")
        .filter(|record| record.is_object())
        .ok_or("此游戏尚未公开 Epic 商店成就资料，无法获取成就列表")?;
    if text(record, "sandboxId") != namespace {
        return Err("Epic 返回的成就游戏身份不一致".into());
    }
    let rows = record
        .get("achievements")
        .and_then(Value::as_array)
        .ok_or("Epic 成就列表格式无效")?;
    if record.get("totalAchievements").and_then(Value::as_u64) != Some(rows.len() as u64) {
        return Err("Epic 成就列表不完整，请稍后重试".into());
    }
    let mut snapshot = Snapshot {
        definitions: Vec::new(),
        details: serde_json::json!({}),
        unlocks: Vec::new(),
    };
    let mut seen = HashSet::new();
    for row in rows {
        let item = row
            .get("achievement")
            .filter(|item| item.is_object())
            .ok_or("Epic 成就详情格式无效")?;
        let id = text(item, "name");
        if id.is_empty() || !seen.insert(id.to_owned()) {
            return Err("Epic 成就标识缺失或重复".into());
        }
        let name = text(item, "lockedDisplayName");
        let description = text(item, "lockedDescription");
        let icon = text(item, "lockedIconLink");
        snapshot.definitions.push(AchievementDefinition {
            api_name: id.into(),
            name: if name.is_empty() {
                text(item, "unlockedDisplayName")
            } else {
                name
            }
            .into(),
            description: if description.is_empty() {
                text(item, "unlockedDescription")
            } else {
                description
            }
            .into(),
            icon: if icon.is_empty() {
                text(item, "unlockedIconLink")
            } else {
                icon
            }
            .into(),
            hidden: item
                .get("hidden")
                .and_then(Value::as_bool)
                .ok_or("Epic 成就隐藏状态无效")?,
        });
        snapshot.details[id] = serde_json::json!({"xp":item.get("XP").and_then(Value::as_u64),
            "tier":item.pointer("/tier/name").and_then(Value::as_str),"rarity":item.pointer("/rarity/percent").and_then(Value::as_f64),
            "unlockedName":text(item,"unlockedDisplayName"),"unlockedDescription":text(item,"unlockedDescription"),"unlockedIcon":text(item,"unlockedIconLink")});
    }
    Ok(snapshot)
}
fn player(
    data: &Value,
    namespace: &str,
    account: &str,
    snapshot: &mut Snapshot,
) -> Result<(), String> {
    response(data)?;
    let records = data
        .pointer("/data/PlayerAchievement/playerAchievementGameRecordsBySandbox/records")
        .and_then(Value::as_array)
        .ok_or("Epic 未返回有效的账号成就记录，请检查账号资料可见性")?;
    let known: HashSet<_> = snapshot
        .definitions
        .iter()
        .map(|item| item.api_name.as_str())
        .collect();
    let mut seen = HashSet::new();
    for record in records {
        let rows = record
            .get("playerAchievements")
            .and_then(Value::as_array)
            .ok_or("Epic 账号成就列表格式无效")?;
        for row in rows {
            let item = row
                .get("playerAchievement")
                .ok_or("Epic 账号成就记录格式无效")?;
            if text(item, "sandboxId") != namespace || text(item, "epicAccountId") != account {
                return Err("Epic 返回的成就账号或游戏身份不一致".into());
            }
            let id = text(item, "achievementName");
            let unlocked = item
                .get("unlocked")
                .and_then(Value::as_bool)
                .ok_or("Epic 成就解锁状态无效")?;
            if !unlocked {
                continue;
            }
            if !known.contains(id) || !seen.insert(id.to_owned()) {
                return Err("Epic 解锁记录与成就列表不一致，请稍后重试".into());
            }
            let timestamp = chrono::DateTime::parse_from_rfc3339(text(item, "unlockDate"))
                .map_err(|_| "Epic 成就解锁时间无效")?;
            snapshot
                .unlocks
                .push((id.to_owned(), timestamp.to_rfc3339()));
        }
    }
    Ok(())
}
async fn query(token: &str, query: &str, variables: Value) -> Result<Value, Error> {
    let response = epic_auth::client()?
        .post("https://launcher.store.epicgames.com/graphql")
        .bearer_auth(token)
        .json(&serde_json::json!({"query":query,"variables":variables}))
        .send()
        .await
        .map_err(epic_auth::network_error)?;
    if response.status().as_u16() == 401 {
        return Err(Error::Unauthorized);
    }
    if !response.status().is_success() {
        return Err(Error::Message(
            "无法读取 Epic 成就，请检查授权、网络或稍后重试".into(),
        ));
    }
    let data: Value = response
        .json()
        .await
        .map_err(|_| Error::Message("Epic 成就响应格式无效".into()))?;
    if data
        .get("errors")
        .and_then(Value::as_array)
        .is_some_and(|errors| {
            errors.iter().any(|error| {
                error.pointer("/extensions/code").and_then(Value::as_str) == Some("UNAUTHENTICATED")
            })
        })
    {
        return Err(Error::Unauthorized);
    }
    Ok(data)
}
async fn fetch(session: &epic_auth::Session, namespace: &str) -> Result<Snapshot, Error> {
    let mut snapshot = definitions(
        &query(
            &session.access_token,
            DEFINITIONS,
            serde_json::json!({"sandboxId":namespace,"locale":"zh-CN"}),
        )
        .await?,
        namespace,
    )?;
    if !snapshot.definitions.is_empty() {
        player(
            &query(
                &session.access_token,
                PLAYER,
                serde_json::json!({"sandboxId":namespace,"epicAccountId":session.account_id}),
            )
            .await?,
            namespace,
            &session.account_id,
            &mut snapshot,
        )?;
    }
    Ok(snapshot)
}
fn save(conn: &mut rusqlite::Connection, game: &Game, snapshot: &Snapshot) -> Result<(), String> {
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    let current = db::game(&tx, &game.id)?.ok_or("游戏已被移除")?;
    let old: Value = serde_json::from_str(&game.metadata_json).map_err(|_| "Epic 游戏身份无效")?;
    let mut metadata: Value =
        serde_json::from_str(&current.metadata_json).map_err(|_| "Epic 游戏资料无效")?;
    if current.source != "epic" || metadata.get("epicNamespace") != old.get("epicNamespace") {
        return Err("Epic 游戏身份已更换，已丢弃旧资料".into());
    }
    let key = format!("epic:{}", game.id);
    let previous = db::achievements(&tx, &game.id)?;
    // 仅在完整响应通过校验后替换 Epic 快照，失败时保留已有缓存。
    tx.execute("DELETE FROM achievements WHERE appid=?1", [&key])
        .map_err(|e| e.to_string())?;
    for item in &snapshot.definitions {
        tx.execute("INSERT INTO achievements(appid,api_name,name,description,icon,hidden) VALUES(?1,?2,?3,?4,?5,?6)",
            rusqlite::params![key,item.api_name,item.name,item.description,item.icon,item.hidden]).map_err(|e| e.to_string())?;
    }
    tx.execute(
        "DELETE FROM unlocks WHERE game_id=?1 AND source='epic'",
        [&game.id],
    )
    .map_err(|e| e.to_string())?;
    for (id, time) in &snapshot.unlocks {
        tx.execute("INSERT INTO unlocks(game_id,api_name,source,unlocked_at,evidence) VALUES(?1,?2,'epic',?3,'Epic 官方成就') ON CONFLICT(game_id,api_name) DO UPDATE SET source='epic',unlocked_at=excluded.unlocked_at,evidence=excluded.evidence", rusqlite::params![game.id,id,time]).map_err(|e| e.to_string())?;
    }
    metadata["epicAchievementDetails"] = snapshot.details.clone();
    db::save_metadata(&tx, &game.id, &metadata)?;
    tx.execute(
        "UPDATE games SET schema_source=?1 WHERE id=?2",
        rusqlite::params![
            if snapshot.definitions.is_empty() {
                EMPTY_SCHEMA
            } else {
                SCHEMA
            },
            game.id
        ],
    )
    .map_err(|e| e.to_string())?;
    db::update_scan(
        &tx,
        &game.id,
        if snapshot.definitions.is_empty() {
            "此游戏暂无 Epic 成就"
        } else {
            "Epic 成就已同步"
        },
        SCHEMA,
    )?;
    db::set_setting(
        &tx,
        &format!("achievement_definition_error:{}", game.id),
        "",
    )?;
    crate::statistics::store::official_changes(&tx, &current, &previous)?;
    tx.commit().map_err(|e| e.to_string())
}
pub(crate) async fn sync(app: &tauri::AppHandle, game: &Game) -> Result<String, String> {
    let metadata: Value =
        serde_json::from_str(&game.metadata_json).map_err(|_| "Epic 游戏资料无效，请重新导入")?;
    let namespace = text(&metadata, "epicNamespace");
    if namespace.is_empty() {
        return Err("Epic 游戏身份缺失，请重新导入账号游戏库".into());
    }
    let _guard = epic_auth::AUTH_LOCK.lock().await;
    let session = epic_auth::session(app, false).await?;
    let snapshot = match fetch(&session, namespace).await {
        Ok(snapshot) => snapshot,
        Err(Error::Unauthorized) => {
            let session = epic_auth::session(app, true).await?;
            fetch(&session, namespace).await.map_err(|e| match e {
                Error::Unauthorized => "Epic 授权已失效，请重新登录".into(),
                Error::Message(message) => message,
            })?
        }
        Err(Error::Message(message)) => return Err(message),
    };
    save(&mut *lock_db(&app.state::<AppState>())?, game, &snapshot)?;
    Ok(if snapshot.definitions.is_empty() {
        "此游戏暂无 Epic 成就".into()
    } else {
        format!(
            "已更新 Epic 成就：{} 项，已解锁 {} 项",
            snapshot.definitions.len(),
            snapshot.unlocks.len()
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn data() -> Value {
        serde_json::json!({"data":{"Achievement":{"productAchievementsRecordBySandbox":{"sandboxId":"ns","totalAchievements":1,"achievements":[{"achievement":{"name":"FIRST","hidden":false,"lockedDisplayName":"起点","lockedDescription":"完成第一章","lockedIconLink":"https://example.com/locked.png","XP":10,"rarity":{"percent":12.5}}}]}}}})
    }
    #[test]
    fn parses_details_and_rejects_unavailable_or_partial_responses() {
        let snapshot = definitions(&data(), "ns").unwrap();
        assert_eq!(snapshot.definitions[0].name, "起点");
        assert_eq!(snapshot.details["FIRST"]["xp"], 10);
        assert!(definitions(&data(), "wrong").is_err());
        let mut partial = data();
        partial["data"]["Achievement"]["productAchievementsRecordBySandbox"]["totalAchievements"] =
            serde_json::json!(2);
        assert!(definitions(&partial, "ns").is_err());
        assert!(definitions(&serde_json::json!({"errors":[{}]}), "ns").is_err());
        assert!(definitions(&serde_json::json!({"data":{"Achievement":{"productAchievementsRecordBySandbox":null}}}),"ns").is_err());
    }
    #[test]
    fn validates_account_and_unlock_timestamp_before_accepting_snapshot() {
        let mut snapshot = definitions(&data(), "ns").unwrap();
        let data = serde_json::json!({"data":{"PlayerAchievement":{"playerAchievementGameRecordsBySandbox":{"records":[{"playerAchievements":[{"playerAchievement":{"sandboxId":"ns","epicAccountId":"account","achievementName":"FIRST","unlocked":true,"unlockDate":"2026-10-01T00:00:00Z"}}]}]}}}});
        player(&data, "ns", "account", &mut snapshot).unwrap();
        assert_eq!(snapshot.unlocks.len(), 1);
        let mut fresh = definitions(&super::tests::data(), "ns").unwrap();
        assert!(player(&data, "ns", "other", &mut fresh).is_err());
        let mut invalid = data;
        invalid["data"]["PlayerAchievement"]["playerAchievementGameRecordsBySandbox"]["records"]
            [0]["playerAchievements"][0]["playerAchievement"]["unlockDate"] =
            serde_json::json!("invalid");
        assert!(player(&invalid, "ns", "account", &mut fresh).is_err());
    }
    #[test]
    fn saves_platform_scoped_cache_and_empty_snapshot() {
        let mut conn = db::open(std::path::Path::new(":memory:")).unwrap();
        let game = Game {
            id: "epic-test".into(),
            source: "epic".into(),
            metadata_json: r#"{"epicNamespace":"ns"}"#.into(),
            ..Default::default()
        };
        db::upsert_game(&conn, &game).unwrap();
        let mut snapshot = definitions(&data(), "ns").unwrap();
        snapshot
            .unlocks
            .push(("FIRST".into(), "2026-10-01T00:00:00Z".into()));
        save(&mut conn, &game, &snapshot).unwrap();
        let achievements = db::achievements(&conn, &game.id).unwrap();
        assert_eq!(achievements.len(), 1);
        assert_eq!(achievements[0].unlock_source.as_deref(), Some("epic"));
        let mut changed = data();
        changed["data"]["Achievement"]["productAchievementsRecordBySandbox"]["achievements"][0]
            ["achievement"]["lockedDescription"] = serde_json::json!("更新的说明");
        save(&mut conn, &game, &definitions(&changed, "ns").unwrap()).unwrap();
        assert_eq!(
            db::achievements(&conn, &game.id).unwrap()[0].description,
            "更新的说明"
        );
        assert!(db::achievements(&conn, &game.id).unwrap()[0]
            .unlocked_at
            .is_none());
        save(
            &mut conn,
            &game,
            &Snapshot {
                definitions: vec![],
                details: serde_json::json!({}),
                unlocks: vec![],
            },
        )
        .unwrap();
        assert!(db::achievements(&conn, &game.id).unwrap().is_empty());
        assert_eq!(
            db::game(&conn, &game.id).unwrap().unwrap().schema_source,
            EMPTY_SCHEMA
        );
    }
}
