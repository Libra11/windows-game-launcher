//! 可重新获取的完整演职员索引；不依赖详情横排的精选名单，不进入备份。
use super::{metadata, store, State};
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::{collections::BTreeMap, path::Path, time::Duration};
use tauri::{Emitter, Manager};

pub(super) fn initialize(conn: &Connection) -> Result<(), String> {
    conn.execute_batch("CREATE TABLE IF NOT EXISTS media_credit_cache(
        provider_type TEXT NOT NULL,external_id TEXT NOT NULL,people_json TEXT NOT NULL DEFAULT '[]',
        fetched_at TEXT NOT NULL DEFAULT '',checked_at TEXT NOT NULL DEFAULT '',error TEXT NOT NULL DEFAULT '',
        PRIMARY KEY(provider_type,external_id))")
        .map_err(|e| e.to_string())
}

fn push_unique(values: &mut Value, value: &str) {
    if !value.trim().is_empty() && !values.as_array().unwrap().iter().any(|v| v == value) {
        values.as_array_mut().unwrap().push(json!(value));
    }
}

pub(super) fn normalize(raw: &Value) -> Vec<Value> {
    let mut people = BTreeMap::new();
    for department in ["cast", "crew"] {
        for person in raw[department].as_array().into_iter().flatten() {
            let Some(id) = person["id"].as_u64().filter(|id| *id > 0) else {
                continue;
            };
            let name = store::text(person, "name");
            if name.trim().is_empty() {
                continue;
            }
            let entry = people.entry(id).or_insert_with(|| json!({
                "id":id,"name":name,"aliases":[],"profileUrl":super::presentation::image(&person["profile_path"],"w185"),
                "departments":[],"roles":[],
            }));
            push_unique(&mut entry["aliases"], name);
            push_unique(&mut entry["aliases"], store::text(person, "original_name"));
            for alias in person["also_known_as"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
            {
                push_unique(&mut entry["aliases"], alias);
            }
            push_unique(&mut entry["departments"], department);
            if department == "cast" {
                push_unique(&mut entry["roles"], store::text(person, "character"));
                for role in person["roles"].as_array().into_iter().flatten() {
                    push_unique(&mut entry["roles"], store::text(role, "character"));
                }
            } else {
                push_unique(&mut entry["roles"], store::text(person, "job"));
                for role in person["jobs"].as_array().into_iter().flatten() {
                    push_unique(&mut entry["roles"], store::text(role, "job"));
                }
            }
        }
    }
    people.into_values().collect()
}

pub(super) fn path(kind: &str, id: &str) -> Result<String, String> {
    let parts: Vec<_> = id.split(':').collect();
    if parts
        .iter()
        .any(|v| v.is_empty() || v.len() > 20 || !v.bytes().all(|c| c.is_ascii_digit()))
    {
        return Err("演职员资料编号无效".into());
    }
    match (kind, parts.as_slice()) {
        ("movie" | "tv", [id]) => Ok(format!("/{kind}/{id}")),
        ("season", [id, number]) => Ok(format!("/tv/{id}/season/{number}")),
        _ => Err("演职员资料类型无效".into()),
    }
}

pub(super) fn save(conn: &Connection, kind: &str, id: &str, raw: &Value) -> Result<(), String> {
    path(kind, id)?;
    let valid_id = if kind == "season" {
        raw["id"].as_u64().is_some_and(|n| n > 0)
    } else {
        raw["id"].as_u64().map(|value| value.to_string()).as_deref() == Some(id)
    };
    if !valid_id || !raw["cast"].is_array() || !raw["crew"].is_array() {
        return Err("演职员资料不完整，已保留原索引".into());
    }
    conn.execute("INSERT INTO media_credit_cache(provider_type,external_id,people_json,fetched_at,checked_at)
        VALUES(?1,?2,?3,?4,?4) ON CONFLICT(provider_type,external_id) DO UPDATE SET
        people_json=excluded.people_json,fetched_at=excluded.fetched_at,checked_at=excluded.checked_at,error=''",
        params![kind,id,serde_json::to_string(&normalize(raw)).map_err(|e|e.to_string())?,store::now()])
        .map(|_|()).map_err(|e|e.to_string())
}

pub(super) fn index_status(conn: &Connection) -> Result<Value, String> {
    let (total, completed, failed): (i64, i64, i64) = conn.query_row(
        "SELECT COUNT(*), COALESCE(SUM(CASE WHEN COALESCE(c.fetched_at,'')<>'' THEN 1 ELSE 0 END),0),
         COALESCE(SUM(CASE WHEN COALESCE(c.error,'')<>'' THEN 1 ELSE 0 END),0)
         FROM external_ids e JOIN media_items i ON i.id=e.item_id
         LEFT JOIN media_credit_cache c ON c.provider_type=e.provider_type AND c.external_id=e.external_id
         WHERE e.provider='tmdb' AND e.provider_type IN('movie','season')
         AND i.recognized=1 AND i.kind IN('movie','season')",
        [], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    ).map_err(|e| e.to_string())?;
    Ok(json!({"total":total,"completed":completed,"failed":failed}))
}

pub(super) fn snapshot(conn: &Connection) -> Result<Value, String> {
    // 每季使用自己的演职员名单，不从整剧名单推断。
    let mut stmt = conn.prepare("SELECT e.item_id,e.provider_type,e.external_id,c.people_json,c.fetched_at,c.error
        FROM external_ids e JOIN media_items i ON i.id=e.item_id
        LEFT JOIN media_credit_cache c ON c.provider_type=e.provider_type AND c.external_id=e.external_id
        WHERE e.provider='tmdb' AND e.provider_type IN('movie','season')
        AND i.recognized=1 AND i.kind IN('movie','season') ORDER BY e.item_id")
        .map_err(|e|e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    let mut people = BTreeMap::<u64, Value>::new();
    let (mut total, mut completed, mut failed) = (0, 0, 0);
    for row in rows {
        let (item_id, kind, _, raw, fetched, error) = row.map_err(|e| e.to_string())?;
        total += 1;
        if fetched.is_some_and(|v| !v.is_empty()) {
            completed += 1;
        }
        if error.is_some_and(|v| !v.is_empty()) {
            failed += 1;
        }
        let entries: Vec<Value> = raw
            .and_then(|v| serde_json::from_str(&v).ok())
            .unwrap_or_default();
        for person in entries {
            let Some(id) = person["id"].as_u64() else {
                continue;
            };
            let entry = people.entry(id).or_insert_with(|| {
                json!({
                    "id":id,"name":person["name"],"profileUrl":person["profileUrl"],
                    "aliases":[],"departments":[],"credits":[],
                })
            });
            for field in ["aliases", "departments"] {
                for value in person[field]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                {
                    push_unique(&mut entry[field], value);
                }
            }
            if store::text(entry, "profileUrl").is_empty() {
                entry["profileUrl"] = person["profileUrl"].clone();
            }
            entry["credits"].as_array_mut().unwrap().push(json!({
                "itemId":item_id,"scope":kind,"roles":person["roles"],"departments":person["departments"],
            }));
        }
    }
    Ok(json!({"people":people.into_values().collect::<Vec<_>>(),
        "peopleIndex":{"total":total,"completed":completed,"failed":failed}}))
}

pub(super) fn profile(conn: &Connection, person_id: u64, item_id: &str) -> Result<String, String> {
    let mut stmt = conn
        .prepare(
            "SELECT c.people_json FROM media_credit_cache c JOIN external_ids e
        ON e.provider='tmdb' AND e.provider_type=c.provider_type AND e.external_id=c.external_id
        JOIN media_items i ON i.id=e.item_id WHERE e.item_id=?1 AND i.recognized=1",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([item_id], |row| row.get::<_, String>(0))
        .map_err(|e| e.to_string())?;
    for row in rows {
        let people: Vec<Value> =
            serde_json::from_str(&row.map_err(|e| e.to_string())?).unwrap_or_default();
        if let Some(person) = people.iter().find(|person| person["id"] == person_id) {
            return Ok(store::text(person, "profileUrl").to_owned());
        }
    }
    Ok(String::new())
}

async fn update_next(dir: &Path) -> Result<bool, String> {
    if store::credential(dir, "tmdbReadAccessToken")?.is_empty()
        && store::credential(dir, "tmdbApiKey")?.is_empty()
    {
        return Ok(false);
    }
    let target = {
        let conn = store::open(dir)?;
        let mut stmt = conn.prepare("SELECT DISTINCT e.provider_type,e.external_id FROM external_ids e
            JOIN media_items i ON i.id=e.item_id LEFT JOIN media_credit_cache c
            ON c.provider_type=e.provider_type AND c.external_id=e.external_id
            WHERE e.provider='tmdb' AND e.provider_type IN('movie','season')
            AND i.recognized=1 AND i.kind IN('movie','season')
            AND (c.fetched_at IS NULL OR c.fetched_at='' OR julianday('now')-julianday(c.fetched_at)>=7)
            AND (c.checked_at IS NULL OR c.checked_at='' OR julianday('now')-julianday(c.checked_at)>=1.0/96)
            ORDER BY COALESCE(c.checked_at,'') LIMIT 1").map_err(|e|e.to_string())?;
        let mut rows = stmt.query([]).map_err(|e| e.to_string())?;
        rows.next()
            .map_err(|e| e.to_string())?
            .map(|row| {
                Ok::<_, String>((
                    row.get::<_, String>(0).map_err(|e| e.to_string())?,
                    row.get::<_, String>(1).map_err(|e| e.to_string())?,
                ))
            })
            .transpose()?
    };
    let Some((kind, id)) = target else {
        return Ok(false);
    };
    let endpoint = if kind == "season" {
        "aggregate_credits"
    } else {
        "credits"
    };
    let result = metadata::api(dir, &format!("{}/{endpoint}", path(&kind, &id)?), &[]).await;
    let conn = store::open(dir)?;
    let result = result.and_then(|raw| save(&conn, &kind, &id, &raw));
    if let Err(error) = result {
        conn.execute("INSERT INTO media_credit_cache(provider_type,external_id,checked_at,error) VALUES(?1,?2,?3,?4)
            ON CONFLICT(provider_type,external_id) DO UPDATE SET checked_at=excluded.checked_at,error=excluded.error",
            params![kind,id,store::now(),error]).map_err(|e|e.to_string())?;
    }
    Ok(true)
}

pub(super) fn start(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(3)).await;
            let Ok(_operation) = super::operation(&app).await else {
                continue;
            };
            let dir = app.state::<State>().dir.clone();
            if update_next(&dir).await.unwrap_or(false) {
                let _ = app.emit("media-changed", ());
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    fn database() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE media_items(id TEXT PRIMARY KEY,kind TEXT,recognized INTEGER);
            CREATE TABLE external_ids(item_id TEXT,provider TEXT,provider_type TEXT,external_id TEXT);
            INSERT INTO media_items VALUES('movie','movie',1),('series','series',1),('season','season',1);
            INSERT INTO external_ids VALUES('movie','tmdb','movie','1'),('series','tmdb','tv','1'),('season','tmdb','season','1:1');").unwrap();
        initialize(&conn).unwrap();
        conn
    }
    #[test]
    fn full_credits_keep_all_people_roles_and_distinct_identities() {
        let mut cast: Vec<Value> = (1..=20)
            .map(|id| json!({"id":id,"name":format!("演员{id}"),"character":"角色"}))
            .collect();
        cast[0] = json!({"id":1,"name":"同名","original_name":"Original","roles":[{"character":"甲"},{"character":"乙"}],"profile_path":"/person.jpg"});
        cast[1]["name"] = json!("同名");
        let people = normalize(
            &json!({"cast":cast,"crew":[{"id":1,"name":"同名","jobs":[{"job":"Director"},{"job":"Writer"}]}]}),
        );
        assert_eq!(people.len(), 20);
        assert_eq!(
            people[0]["roles"],
            json!(["甲", "乙", "Director", "Writer"])
        );
        assert_eq!(people[0]["departments"], json!(["cast", "crew"]));
        assert!(people[0]["aliases"]
            .as_array()
            .unwrap()
            .contains(&json!("Original")));
    }
    #[test]
    fn snapshot_joins_current_identity_and_does_not_invent_season_credits() {
        let conn = database();
        let raw = json!({"id":1,"cast":[{"id":9,"name":"演员","character":"角色"}],"crew":[]});
        save(&conn, "movie", "1", &raw).unwrap();
        save(&conn, "tv", "1", &raw).unwrap();
        let value = snapshot(&conn).unwrap();
        assert_eq!(index_status(&conn).unwrap(), value["peopleIndex"]);
        assert_eq!(value["people"].as_array().unwrap().len(), 1);
        assert_eq!(value["people"][0]["credits"].as_array().unwrap().len(), 1);
        assert_eq!(value["peopleIndex"]["completed"], 1);
        save(&conn, "season", "1:1", &raw).unwrap();
        let value = snapshot(&conn).unwrap();
        assert_eq!(index_status(&conn).unwrap(), value["peopleIndex"]);
        assert_eq!(value["people"][0]["credits"].as_array().unwrap().len(), 2);
        assert_eq!(value["peopleIndex"]["completed"], 2);
        conn.execute("DELETE FROM external_ids WHERE item_id='movie'", [])
            .unwrap();
        conn.execute("DELETE FROM media_items WHERE id='season'", [])
            .unwrap();
        assert_eq!(snapshot(&conn).unwrap()["people"], json!([]));
    }
    #[test]
    fn invalid_responses_preserve_cache_and_success_replaces_relationships() {
        let conn = database();
        save(
            &conn,
            "movie",
            "1",
            &json!({"id":1,"cast":[{"id":9,"name":"旧演员"}],"crew":[]}),
        )
        .unwrap();
        assert!(save(&conn, "movie", "1", &json!({"id":2,"cast":[],"crew":[]})).is_err());
        assert!(save(&conn, "movie", "1", &json!({"id":1})).is_err());
        assert_eq!(snapshot(&conn).unwrap()["people"][0]["name"], "旧演员");
        save(&conn, "movie", "1", &json!({"id":1,"cast":[],"crew":[]})).unwrap();
        assert_eq!(snapshot(&conn).unwrap()["people"], json!([]));
        assert_eq!(snapshot(&conn).unwrap()["peopleIndex"]["completed"], 1);
    }
    #[test]
    fn profile_must_belong_to_requested_library_item() {
        let conn = database();
        save(&conn,"movie","1",&json!({"id":1,"cast":[{"id":9,"name":"演员","profile_path":"/person.jpg"},{"id":10,"name":"坏图","profile_path":"/../secret"}],"crew":[]})).unwrap();
        assert!(profile(&conn, 9, "movie").unwrap().ends_with("/person.jpg"));
        assert_eq!(profile(&conn, 9, "series").unwrap(), "");
        assert_eq!(profile(&conn, 10, "movie").unwrap(), "");
        conn.execute("UPDATE media_items SET recognized=0 WHERE id='movie'", [])
            .unwrap();
        assert_eq!(profile(&conn, 9, "movie").unwrap(), "");
    }
}
