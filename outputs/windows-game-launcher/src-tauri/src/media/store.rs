use rusqlite::{params, Connection, OptionalExtension};
pub(super) mod douban_source;
mod snapshot;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    path::Path,
    time::Duration,
};

pub fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}
pub fn open(dir: &Path) -> Result<Connection, String> {
    std::fs::create_dir_all(dir).map_err(|_| "无法创建影视数据目录")?;
    let conn = Connection::open(dir.join("media.sqlite")).map_err(|_| "无法打开影视数据库")?;
    conn.busy_timeout(Duration::from_secs(5))
        .map_err(|e| e.to_string())?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;
      CREATE TABLE IF NOT EXISTS media_items(id TEXT PRIMARY KEY,kind TEXT NOT NULL DEFAULT 'other',parent_id TEXT REFERENCES media_items(id),title TEXT NOT NULL,year TEXT NOT NULL DEFAULT '',overview TEXT NOT NULL DEFAULT '',poster_url TEXT NOT NULL DEFAULT '',backdrop_url TEXT NOT NULL DEFAULT '',genres_json TEXT NOT NULL DEFAULT '[]',recognized INTEGER NOT NULL DEFAULT 0,independent INTEGER NOT NULL DEFAULT 0,season_number INTEGER,episode_number INTEGER,created_at TEXT NOT NULL,updated_at TEXT NOT NULL);
      CREATE INDEX IF NOT EXISTS media_parent ON media_items(parent_id);
      CREATE TABLE IF NOT EXISTS external_ids(provider TEXT NOT NULL,provider_type TEXT NOT NULL,external_id TEXT NOT NULL,item_id TEXT NOT NULL REFERENCES media_items(id),PRIMARY KEY(provider,provider_type,external_id));
      CREATE INDEX IF NOT EXISTS media_external_item ON external_ids(item_id);
      CREATE TABLE IF NOT EXISTS personal(item_id TEXT PRIMARY KEY REFERENCES media_items(id),watched INTEGER NOT NULL DEFAULT 0,favorite INTEGER NOT NULL DEFAULT 0,rating REAL,comment TEXT NOT NULL DEFAULT '',marked_at TEXT NOT NULL DEFAULT '',record_source TEXT NOT NULL DEFAULT '',manual_fields_json TEXT NOT NULL DEFAULT '[]',last_played TEXT NOT NULL DEFAULT '');
      CREATE TABLE IF NOT EXISTS sources(id TEXT PRIMARY KEY,path TEXT NOT NULL UNIQUE,label TEXT NOT NULL,kind TEXT NOT NULL DEFAULT 'auto',smart_folders INTEGER NOT NULL DEFAULT 1,available INTEGER NOT NULL DEFAULT 1,last_scan TEXT NOT NULL DEFAULT '',error TEXT NOT NULL DEFAULT '',created_at TEXT NOT NULL);
      CREATE TABLE IF NOT EXISTS resources(id TEXT PRIMARY KEY,source_id TEXT NOT NULL REFERENCES sources(id),path TEXT NOT NULL UNIQUE,size INTEGER NOT NULL DEFAULT 0,modified_at TEXT NOT NULL DEFAULT '',available INTEGER NOT NULL DEFAULT 1,missing INTEGER NOT NULL DEFAULT 0,technical_json TEXT NOT NULL DEFAULT '{}',position_seconds REAL NOT NULL DEFAULT 0,duration_seconds REAL NOT NULL DEFAULT 0,updated_at TEXT NOT NULL);
      CREATE INDEX IF NOT EXISTS media_resource_source ON resources(source_id,missing);
      CREATE INDEX IF NOT EXISTS media_resource_path_nocase ON resources(path COLLATE NOCASE);
      CREATE TABLE IF NOT EXISTS resource_links(resource_id TEXT NOT NULL REFERENCES resources(id),item_id TEXT NOT NULL REFERENCES media_items(id),method TEXT NOT NULL DEFAULT 'scan',locked INTEGER NOT NULL DEFAULT 0,PRIMARY KEY(resource_id,item_id));
      CREATE INDEX IF NOT EXISTS media_links_item ON resource_links(item_id);
      CREATE TABLE IF NOT EXISTS view_sessions(id TEXT PRIMARY KEY,item_id TEXT NOT NULL REFERENCES media_items(id),resource_id TEXT NOT NULL REFERENCES resources(id),started_at TEXT NOT NULL,ended_at TEXT NOT NULL DEFAULT '',seconds REAL NOT NULL DEFAULT 0,position_seconds REAL NOT NULL DEFAULT 0);
      CREATE INDEX IF NOT EXISTS media_sessions_time ON view_sessions(started_at);
      CREATE TABLE IF NOT EXISTS media_collections(id TEXT PRIMARY KEY,name TEXT NOT NULL,sort_order INTEGER NOT NULL DEFAULT 0);
      CREATE TABLE IF NOT EXISTS collection_items(collection_id TEXT NOT NULL REFERENCES media_collections(id) ON DELETE CASCADE,item_id TEXT NOT NULL REFERENCES media_items(id),PRIMARY KEY(collection_id,item_id));
      CREATE TABLE IF NOT EXISTS media_tags(id TEXT PRIMARY KEY,name TEXT NOT NULL UNIQUE);
      CREATE TABLE IF NOT EXISTS media_item_tags(item_id TEXT NOT NULL REFERENCES media_items(id),tag_id TEXT NOT NULL REFERENCES media_tags(id) ON DELETE CASCADE,PRIMARY KEY(item_id,tag_id));
      CREATE TABLE IF NOT EXISTS settings(key TEXT PRIMARY KEY,value TEXT NOT NULL);
      CREATE TABLE IF NOT EXISTS jobs(id TEXT PRIMARY KEY,kind TEXT NOT NULL,state TEXT NOT NULL,completed INTEGER NOT NULL DEFAULT 0,total INTEGER NOT NULL DEFAULT 0,message TEXT NOT NULL DEFAULT '',error TEXT NOT NULL DEFAULT '',created_at TEXT NOT NULL,updated_at TEXT NOT NULL);
      CREATE TABLE IF NOT EXISTS douban_records(account_key TEXT NOT NULL,subject_id TEXT NOT NULL,item_id TEXT NOT NULL REFERENCES media_items(id),stars REAL,marked_at TEXT NOT NULL DEFAULT '',comment TEXT NOT NULL DEFAULT '',tags_json TEXT NOT NULL DEFAULT '[]',imported_at TEXT NOT NULL,PRIMARY KEY(account_key,subject_id));")
        .map_err(|e|e.to_string())?;
    conn.execute_batch("CREATE TABLE IF NOT EXISTS douban_source_data(account_key TEXT NOT NULL,subject_id TEXT NOT NULL,raw_json TEXT NOT NULL,PRIMARY KEY(account_key,subject_id)); CREATE TABLE IF NOT EXISTS media_match_errors(item_id TEXT PRIMARY KEY REFERENCES media_items(id) ON DELETE CASCADE,error TEXT NOT NULL,checked_at TEXT NOT NULL);
      CREATE TABLE IF NOT EXISTS media_playback_coverage(resource_id TEXT PRIMARY KEY,segments_json TEXT NOT NULL DEFAULT '[]')").map_err(|e|e.to_string())?;
    conn.execute_batch("CREATE INDEX IF NOT EXISTS media_collection_item ON collection_items(item_id);
      CREATE INDEX IF NOT EXISTS media_douban_subject ON douban_source_data(subject_id);
      CREATE INDEX IF NOT EXISTS media_sessions_resource ON view_sessions(resource_id,started_at)").map_err(|e|e.to_string())?;
    super::people::initialize(&conn)?;
    super::metadata::initialize_retry(&conn)?;
    // 可重新获取的补充资料独立保存，不改变业务条目和个人记录的表结构。
    conn.execute_batch("CREATE TABLE IF NOT EXISTS media_item_metadata(item_id TEXT PRIMARY KEY REFERENCES media_items(id) ON DELETE CASCADE,air_date TEXT NOT NULL DEFAULT '',season_count INTEGER)").map_err(|e| e.to_string())?;
    Ok(conn)
}
pub fn setting(conn: &Connection, key: &str) -> Result<Value, String> {
    let raw: Option<String> = conn
        .query_row("SELECT value FROM settings WHERE key=?1", [key], |r| {
            r.get(0)
        })
        .optional()
        .map_err(|e| e.to_string())?;
    Ok(raw
        .and_then(|v| serde_json::from_str(&v).ok())
        .unwrap_or(Value::Null))
}
pub fn set_setting(conn: &Connection, key: &str, value: &Value) -> Result<(), String> {
    conn.execute("INSERT INTO settings(key,value)VALUES(?1,?2)ON CONFLICT(key)DO UPDATE SET value=excluded.value",
params![key,
value.to_string()]).map(|_|()).map_err(|e|e.to_string())
}
pub fn credential(dir: &Path, name: &str) -> Result<String, String> {
    if !matches!(name, "tmdbApiKey" | "tmdbReadAccessToken") {
        return Err("凭据类型无效".into());
    }
    let profile = format!("{:x}", Sha256::digest(dir.to_string_lossy().as_bytes()));
    #[cfg(windows)]
    {
        let entry = keyring::Entry::new("dev.youji.media", &format!("{}:{}", &profile[..20], name))
            .map_err(|_| "无法访问影视凭据")?;
        match entry.get_password() {
            Ok(v) => Ok(v),
            Err(keyring::Error::NoEntry) => Ok(String::new()),
            Err(_) => Err("无法读取影视凭据，请重新保存".into()),
        }
    }
    #[cfg(not(windows))]
    {
        let _ = profile;
        Ok(String::new())
    }
}
pub fn save_credential(dir: &Path, name: &str, value: &str) -> Result<(), String> {
    if !matches!(name, "tmdbApiKey" | "tmdbReadAccessToken") {
        return Err("凭据类型无效".into());
    }
    let profile = format!("{:x}", Sha256::digest(dir.to_string_lossy().as_bytes()));
    #[cfg(windows)]
    {
        let entry = keyring::Entry::new("dev.youji.media", &format!("{}:{}", &profile[..20], name))
            .map_err(|_| "无法访问影视凭据")?;
        if value.trim().is_empty() {
            match entry.delete_credential() {
                Ok(_) | Err(keyring::Error::NoEntry) => Ok(()),
                Err(_) => Err("无法清除影视凭据".into()),
            }
        } else {
            entry
                .set_password(value.trim())
                .map_err(|_| "无法保存影视凭据".into())
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (profile, value);
        Err("影视凭据保存仅支持 Windows".into())
    }
}
pub fn new_item(conn: &Connection, data: &Value) -> Result<String, String> {
    let title = data["title"].as_str().unwrap_or("").trim();
    if title.is_empty() {
        return Err("请填写影视名称".into());
    }
    let kind = data["kind"].as_str().unwrap_or("other");
    if !matches!(kind, "movie" | "series" | "season" | "episode" | "other") {
        return Err("影视类型无效".into());
    }
    let id = uuid::Uuid::new_v4().to_string();
    let stamp = now();
    conn.execute("INSERT INTO media_items(id,kind,parent_id,title,year,overview,poster_url,backdrop_url,genres_json,recognized,independent,season_number,episode_number,created_at,updated_at)VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?14)",
params![id,
kind,
data["parentId"].as_str().filter(|s|!s.is_empty()),
title,
text(data,
"year"),
text(data,
"overview"),
text(data,
"posterUrl"),
text(data,
"backdropUrl"),
data.get("genres").cloned().unwrap_or(json!([])).to_string(),
data["recognized"].as_bool().unwrap_or(false),
data["independent"].as_bool().unwrap_or(false),
data["seasonNumber"].as_i64(),
data["episodeNumber"].as_i64(),
stamp]).map_err(|e|e.to_string())?;

    conn.execute("INSERT INTO personal(item_id)VALUES(?1)", [&id])
        .map_err(|e| e.to_string())?;
    Ok(id)
}
pub fn upsert_provider_item(
    conn: &Connection,
    provider: &str,
    provider_type: &str,
    external_id: &str,
    data: &Value,
    independent: bool,
) -> Result<String, String> {
    if external_id.is_empty() {
        return Err("外部编号不能为空".into());
    }
    let existing:Option<String>=conn.query_row("SELECT item_id FROM external_ids WHERE provider=?1 AND provider_type=?2 AND external_id=?3",
params![provider,
provider_type,
external_id],
|r|r.get(0)).optional().map_err(|e|e.to_string())?;

    let id = if let Some(id) = existing {
        id
    } else {
        let mut data = data.clone();
        data["recognized"] = json!(provider == "tmdb");
        data["independent"] = json!(independent);
        let id = new_item(conn, &data)?;
        conn.execute("INSERT INTO external_ids(provider,provider_type,external_id,item_id)VALUES(?1,?2,?3,?4)",
params![provider,
provider_type,
external_id,
id]).map_err(|e|e.to_string())?;

        id
    };
    conn.execute("UPDATE media_items SET recognized=MAX(recognized,?4),independent=MAX(independent,?2),updated_at=?3 WHERE id=?1",
params![id,
independent,
now(), provider == "tmdb"]).map_err(|e|e.to_string())?;

    if provider == "tmdb" {
        conn.execute("DELETE FROM media_match_errors WHERE item_id=?1", [&id])
            .map_err(|e| e.to_string())?;
    }
    for (key, column) in [
        ("title", "title"),
        ("year", "year"),
        ("overview", "overview"),
        ("posterUrl", "poster_url"),
        ("backdropUrl", "backdrop_url"),
    ] {
        if let Some(value) = data[key].as_str().filter(|s| !s.is_empty()) {
            conn.execute(
                &format!("UPDATE media_items SET {column}=?2 WHERE id=?1"),
                params![id, value],
            )
            .map_err(|e| e.to_string())?;
        }
    }
    if let Some(value) = data["kind"]
        .as_str()
        .filter(|k| matches!(*k, "movie" | "series" | "season" | "episode"))
    {
        conn.execute(
            "UPDATE media_items SET kind=?2 WHERE id=?1",
            params![id, value],
        )
        .map_err(|e| e.to_string())?;
    }
    for (key, column) in [
        ("seasonNumber", "season_number"),
        ("episodeNumber", "episode_number"),
    ] {
        if let Some(value) = data[key].as_i64() {
            conn.execute(
                &format!("UPDATE media_items SET {column}=?2 WHERE id=?1"),
                params![id, value],
            )
            .map_err(|e| e.to_string())?;
        }
    }
    if let Some(parent) = data["parentId"].as_str().filter(|s| !s.is_empty()) {
        conn.execute(
            "UPDATE media_items SET parent_id=?2 WHERE id=?1 AND id<>?2",
            params![id, parent],
        )
        .map_err(|e| e.to_string())?;
    }
    if data["genres"].is_array() {
        conn.execute(
            "UPDATE media_items SET genres_json=?2 WHERE id=?1",
            params![id, data["genres"].to_string()],
        )
        .map_err(|e| e.to_string())?;
    }
    if data.get("airDate").is_some() || data.get("seasonCount").is_some() {
        conn.execute("INSERT INTO media_item_metadata(item_id,air_date,season_count)VALUES(?1,?2,?3)
            ON CONFLICT(item_id) DO UPDATE SET air_date=excluded.air_date,season_count=excluded.season_count",
            params![id,text(data,"airDate"),data["seasonCount"].as_i64()]).map_err(|e|e.to_string())?;
    }
    Ok(id)
}
pub fn update_record(
    conn: &Connection,
    item_id: &str,
    patch: &Value,
    manual: bool,
) -> Result<(), String> {
    if !patch.is_object() {
        return Err("观影记录格式无效".into());
    }
    let mut patch = patch.clone();
    let exists: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM media_items WHERE id=?1)",
            [item_id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if !exists {
        return Err("影视条目不存在".into());
    }
    conn.execute(
        "INSERT OR IGNORE INTO personal(item_id)VALUES(?1)",
        [item_id],
    )
    .map_err(|e| e.to_string())?;
    let (raw, marked_at, record_source): (String, String, String) = conn
        .query_row(
            "SELECT manual_fields_json,marked_at,record_source FROM personal WHERE item_id=?1",
            [item_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(|e| e.to_string())?;
    let mut fields: HashSet<String> = serde_json::from_str(&raw).unwrap_or_default();
    // 已看只有一个状态，手动标记、播放完成和来源更新均按本次操作写入。
    // 评分、短评等其他字段仍保留人工修改保护。
    if patch["watched"] == true && patch.get("markedAt").is_none() && marked_at.is_empty() {
        patch["markedAt"] = json!(chrono::Local::now().date_naive().to_string());
    }
    if patch.get("recordSource").is_none()
        && record_source.is_empty()
        && (manual || patch.get("watched").is_some())
    {
        patch["recordSource"] = json!(if manual { "manual" } else { "playback" });
    }
    for (key, column) in [
        ("watched", "watched"),
        ("favorite", "favorite"),
        ("rating", "rating"),
        ("comment", "comment"),
        ("markedAt", "marked_at"),
        ("lastPlayed", "last_played"),
    ] {
        let Some(value) = patch.get(key) else {
            continue;
        };
        if !manual && key != "watched" && fields.contains(key) {
            continue;
        }
        match key {
            "watched" | "favorite" => {
                let value = value.as_bool().ok_or("观看和收藏状态须为布尔值")?;
                conn.execute(
                    &format!("UPDATE personal SET {column}=?2 WHERE item_id=?1"),
                    params![item_id, value],
                )
                .map_err(|e| e.to_string())?;
            }
            "rating" => {
                let value = if value.is_null() {
                    None
                } else {
                    let v = value.as_f64().ok_or("评分无效")?;
                    if !v.is_finite() || !(1.0..=5.0).contains(&v) {
                        return Err("个人评分须为 1 至 5 星".into());
                    }
                    Some(v)
                };
                conn.execute(
                    "UPDATE personal SET rating=?2 WHERE item_id=?1",
                    params![item_id, value],
                )
                .map_err(|e| e.to_string())?;
            }
            _ => {
                let v = value.as_str().ok_or("观影记录文本无效")?;
                if v.len() > 16000 {
                    return Err("观影记录过长".into());
                }
                if key == "markedAt"
                    && !v.is_empty()
                    && chrono::NaiveDate::parse_from_str(v, "%Y-%m-%d").is_err()
                    && chrono::DateTime::parse_from_rfc3339(v).is_err()
                {
                    return Err("观影日期格式无效".into());
                }
                conn.execute(
                    &format!("UPDATE personal SET {column}=?2 WHERE item_id=?1"),
                    params![item_id, v],
                )
                .map_err(|e| e.to_string())?;
            }
        }
        if manual && !matches!(key, "watched" | "lastPlayed") {
            fields.insert(key.to_string());
        }
    }
    if let Some(source) = patch["recordSource"].as_str() {
        conn.execute(
            "UPDATE personal SET record_source=?2 WHERE item_id=?1",
            params![item_id, source],
        )
        .map_err(|e| e.to_string())?;
    }
    conn.execute(
        "UPDATE personal SET manual_fields_json=?2 WHERE item_id=?1",
        params![
            item_id,
            serde_json::to_string(&fields).unwrap_or_else(|_| "[]".into())
        ],
    )
    .map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE media_items SET updated_at=?2 WHERE id=?1",
        params![item_id, now()],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
pub fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key].as_str().unwrap_or("")
}
pub fn resources_for(conn: &Connection, item_id: &str) -> Result<Vec<Value>, String> {
    let mut stmt=conn.prepare("WITH RECURSIVE children(id)AS(SELECT ?1 UNION SELECT m.id FROM media_items m JOIN children c ON m.parent_id=c.id)SELECT DISTINCT r.id FROM resources r JOIN resource_links l ON l.resource_id=r.id JOIN children c ON c.id=l.item_id ORDER BY r.path").map_err(|e|e.to_string())?;

    let ids = stmt
        .query_map([item_id], |r| r.get::<_, String>(0))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    ids.iter().map(|id| resource(conn, id)).collect()
}
pub fn resource(conn: &Connection, id: &str) -> Result<Value, String> {
    let mut value=conn.query_row("SELECT id,source_id,path,size,modified_at,available,missing,technical_json,position_seconds,duration_seconds,updated_at FROM resources WHERE id=?1",
[id],
|r|{

        let path:String=r.get(2)?;let technical:String=r.get(7)?;
        Ok(json!({
            "id":r.get::<_, String>(0)?,
            "sourceId":r.get::<_, String>(1)?,
            "name":Path::new(&path).file_name().map(|n|n.to_string_lossy().to_string()).unwrap_or_default(),
            "path":path,
            "size":r.get::<_, i64>(3)?,
            "modifiedAt":r.get::<_, String>(4)?,
            "available":r.get::<_, bool>(5)?,
            "missing":r.get::<_, bool>(6)?,
            "technical":serde_json::from_str::<Value>(&technical).unwrap_or(json!({ } )),
            "positionSeconds":r.get::<_, f64>(8)?,
            "durationSeconds":r.get::<_, f64>(9)?,
            "updatedAt":r.get::<_, String>(10)?,
        }))
    }).map_err(|_|"媒体资源不存在")?;
    value["playbackCompleted"] = json!(playback_complete(
        value["positionSeconds"].as_f64().unwrap_or(0.0),
        value["durationSeconds"].as_f64().unwrap_or(0.0)
    ));
    value["ignored"] = json!(super::ignored::contains(
        conn,
        text(&value, "sourceId"),
        text(&value, "path")
    )?);
    if value["ignored"] == true {
        value["available"] = json!(false);
    }
    let mut stmt = conn
        .prepare("SELECT item_id FROM resource_links WHERE resource_id=?1 ORDER BY item_id")
        .map_err(|e| e.to_string())?;
    value["itemIds"] = json!(stmt
        .query_map([id], |r| r.get::<_, String>(0))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?);
    Ok(value)
}
pub fn playback_complete(position: f64, duration: f64) -> bool {
    position.is_finite()
        && duration.is_finite()
        && duration > 0.0
        && position >= duration * 0.95
}
pub fn items(conn: &Connection) -> Result<Vec<Value>, String> { snapshot::read(conn,None) }
pub fn items_by_ids(conn: &Connection, ids: &[String]) -> Result<Vec<Value>, String> {
    snapshot::read_many(conn, ids)
}
pub fn item(conn: &Connection,id:&str)->Result<Value,String>{
    let started=std::time::Instant::now();
    let result=snapshot::read(conn,Some(id)).and_then(|mut rows|rows.pop().ok_or("影视条目不存在".into()));
    super::metadata::diagnostics::database_read(started.elapsed());
    result
}
pub fn detail(conn: &Connection, id: &str) -> Result<Value, String> {
    let current = item(conn, id)?;
    let mut parents = Vec::new();
    let mut parent = current["parentId"].as_str().map(str::to_owned);
    let mut seen = HashSet::new();
    while let Some(p) = parent {
        if !seen.insert(p.clone()) {
            break;
        }
        let value = match item(conn, &p) {
            Ok(value) => value,
            Err(error) if error == "影视条目不存在" => break,
            Err(error) => return Err(error),
        };
        parent = value["parentId"].as_str().map(str::to_owned);
        parents.push(value);
    }
    parents.reverse();
    let mut statement = conn
        .prepare("SELECT id FROM media_items WHERE parent_id=?1 ORDER BY title COLLATE NOCASE")
        .map_err(|e| e.to_string())?;
    let children_ids = statement
        .query_map([id], |row| row.get::<_, String>(0))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    let mut children = children_ids
        .iter()
        .map(|child| item(conn, child))
        .collect::<Result<Vec<_>, _>>()?;
    children.sort_by_key(|v| {
        (
            v["seasonNumber"].as_i64().unwrap_or(0),
            v["episodeNumber"].as_i64().unwrap_or(0),
        )
    });
    Ok(
        json!({"item":current,"parents":parents,"children":children,"resources":resources_for(conn,id)?}),
    )
}
// 关联时本机个人记录优先于豆瓣；相同优先级保留目标条目，空值由另一边补齐。
fn merged_record_value<'a>(
    left: &'a Value,
    right: &'a Value,
    key: &str,
    left_manual: &HashSet<String>,
    right_manual: &HashSet<String>,
) -> &'a Value {
    let present = |v: &Value| !v.is_null() && v.as_str() != Some("");
    let rank = |item: &Value, manual: &HashSet<String>| {
        if manual.contains(key) {
            2
        } else if item["recordSource"] != "douban" {
            1
        } else {
            0
        }
    };
    if present(&left[key])
        && (!present(&right[key]) || rank(left, left_manual) > rank(right, right_manual))
    {
        &left[key]
    } else {
        &right[key]
    }
}
pub fn merge_items(conn: &Connection, source: &str, target: &str) -> Result<(), String> {
    if source == target {
        return Ok(());
    }
    let left = item(conn, source)?;
    let right = item(conn, target)?;
    let ancestor:bool=conn.query_row("WITH RECURSIVE ancestors(id)AS(SELECT ?1 UNION SELECT m.parent_id FROM media_items m JOIN ancestors a ON a.id=m.id WHERE m.parent_id IS NOT NULL)SELECT EXISTS(SELECT 1 FROM ancestors WHERE id=?2)",
params![source,
target],
|r|r.get(0)).map_err(|e|e.to_string())?;

    let descendant:bool=conn.query_row("WITH RECURSIVE ancestors(id)AS(SELECT ?1 UNION SELECT m.parent_id FROM media_items m JOIN ancestors a ON a.id=m.id WHERE m.parent_id IS NOT NULL)SELECT EXISTS(SELECT 1 FROM ancestors WHERE id=?2)",
params![target,
source],
|r|r.get(0)).map_err(|e|e.to_string())?;

    if ancestor || descendant {
        return Err("不能将父条目与子条目合并".into());
    }
    let watched = left["watched"] == true || right["watched"] == true;
    conn.execute_batch("SAVEPOINT media_merge")
        .map_err(|e| e.to_string())?;
    let result = (|| {
        let manual_fields = |id: &str| -> Result<HashSet<String>, String> {
            let raw: String = conn
                .query_row(
                    "SELECT manual_fields_json FROM personal WHERE item_id=?1",
                    [id],
                    |r| r.get(0),
                )
                .map_err(|e| e.to_string())?;
            Ok(serde_json::from_str(&raw).unwrap_or_default())
        };
        let left_manual = manual_fields(source)?;
        let right_manual = manual_fields(target)?;
        let value = |key| merged_record_value(&left, &right, key, &left_manual, &right_manual);
        conn.execute(
            "UPDATE personal SET rating=?2,comment=?3,marked_at=?4,last_played=?5 WHERE item_id=?1",
            params![
                target,
                value("rating").as_f64(),
                value("comment").as_str().unwrap_or(""),
                value("markedAt").as_str().unwrap_or(""),
                text(&left, "lastPlayed").max(text(&right, "lastPlayed"))
            ],
        )
        .map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE personal SET favorite=MAX(favorite,?2)WHERE item_id=?1",
            params![target, left["favorite"].as_bool().unwrap_or(false)],
        )
        .map_err(|e| e.to_string())?;
        // 合并保留已有的已看事实，不生成新的观看日期。
        let marked_at: String = conn
            .query_row(
                "SELECT marked_at FROM personal WHERE item_id=?1",
                [target],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())?;
        update_record(
            conn,
            target,
            &json!({"watched":watched,"markedAt":marked_at}),
            false,
        )?;
        if right["recordSource"].as_str() == Some("")
            || (right["recordSource"] == "douban"
                && left["recordSource"]
                    .as_str()
                    .is_some_and(|s| !s.is_empty() && s != "douban"))
        {
            conn.execute(
                "UPDATE personal SET record_source=?2 WHERE item_id=?1",
                params![target, text(&left, "recordSource")],
            )
            .map_err(|e| e.to_string())?;
        }
        for (table, column) in [
            ("external_ids", "item_id"),
            ("view_sessions", "item_id"),
            ("douban_records", "item_id"),
        ] {
            conn.execute(
                &format!("UPDATE {table} SET {column}=?2 WHERE {column}=?1"),
                params![source, target],
            )
            .map_err(|e| e.to_string())?;
        }
        conn.execute("INSERT OR IGNORE INTO resource_links(resource_id,item_id,method,locked)SELECT resource_id,?2,method,locked FROM resource_links WHERE item_id=?1",
params![source,
target]).map_err(|e|e.to_string())?;

        conn.execute("DELETE FROM resource_links WHERE item_id=?1", [source])
            .map_err(|e| e.to_string())?;
        conn.execute("INSERT OR IGNORE INTO collection_items(collection_id,item_id)SELECT collection_id,?2 FROM collection_items WHERE item_id=?1",
params![source,
target]).map_err(|e|e.to_string())?;

        conn.execute("INSERT OR IGNORE INTO media_item_tags(item_id,tag_id)SELECT ?2,tag_id FROM media_item_tags WHERE item_id=?1",
params![source,
target]).map_err(|e|e.to_string())?;

        let l: String = conn
            .query_row(
                "SELECT manual_fields_json FROM personal WHERE item_id=?1",
                [source],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        let r: String = conn
            .query_row(
                "SELECT manual_fields_json FROM personal WHERE item_id=?1",
                [target],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        let mut fields: HashSet<String> = serde_json::from_str(&l).unwrap_or_default();
        fields.extend(serde_json::from_str::<HashSet<String>>(&r).unwrap_or_default());
        conn.execute(
            "UPDATE personal SET manual_fields_json=?2 WHERE item_id=?1",
            params![target, serde_json::to_string(&fields).unwrap()],
        )
        .map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE media_items SET parent_id=?2 WHERE parent_id=?1",
            params![source, target],
        )
        .map_err(|e| e.to_string())?;
        conn.execute("UPDATE media_items SET independent=MAX(independent,?2),updated_at=?3 WHERE id=?1",
params![target,
left["independent"].as_bool().unwrap_or(false),
now()]).map_err(|e|e.to_string())?;

        conn.execute("DELETE FROM personal WHERE item_id=?1", [source])
            .map_err(|e| e.to_string())?;
        conn.execute("DELETE FROM collection_items WHERE item_id=?1", [source])
            .map_err(|e| e.to_string())?;
        conn.execute("DELETE FROM media_item_tags WHERE item_id=?1", [source])
            .map_err(|e| e.to_string())?;
        conn.execute("DELETE FROM media_items WHERE id=?1", [source])
            .map_err(|e| e.to_string())?;
        Ok(())
    })();
    if result.is_ok() {
        conn.execute_batch("RELEASE media_merge")
            .map_err(|e| e.to_string())?;
    } else {
        let _ = conn.execute_batch("ROLLBACK TO media_merge;RELEASE media_merge");
    }
    result
}
pub fn job_start(conn: &Connection, kind: &str, total: i64) -> Result<String, String> {
    let id = uuid::Uuid::new_v4().to_string();
    let stamp = now();
    conn.execute("INSERT INTO jobs(id,kind,state,total,created_at,updated_at)VALUES(?1,?2,'running',?3,?4,?4)",params![id,kind,total,stamp]).map_err(|e|e.to_string())?;
    Ok(id)
}
pub fn job_update(
    conn: &Connection,
    id: &str,
    state: &str,
    completed: i64,
    total: i64,
    message: &str,
    error: &str,
) -> Result<(), String> {
    conn.execute("UPDATE jobs SET state=?2,completed=?3,total=?4,message=?5,error=?6,updated_at=?7 WHERE id=?1",
params![id,
state,
completed,
total,
message,
error,
now()]).map(|_|()).map_err(|e|e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn items_have_no_hidden_state_and_keep_season_and_record_columns() {
        let dir = tempfile::tempdir().unwrap();
        let conn = open(dir.path()).unwrap();
        let hidden_columns: i64 = conn
            .query_row("SELECT COUNT(*) FROM pragma_table_info('media_items') WHERE name='visible'", [], |row| row.get(0))
            .unwrap();
        assert_eq!(hidden_columns, 0);
        let id = new_item(&conn, &json!({"title":"第三季第七集","kind":"episode","seasonNumber":3,"episodeNumber":7})).unwrap();
        update_record(&conn, &id, &json!({"watched":true,"favorite":true,"rating":4,"comment":"观影记录","markedAt":"2024-01-02"}), true).unwrap();
        let value = item(&conn, &id).unwrap();
        assert!(value.get("visible").is_none());
        assert_eq!(value["seasonNumber"], 3);
        assert_eq!(value["episodeNumber"], 7);
        assert_eq!(value["watched"], true);
        assert_eq!(value["favorite"], true);
        assert_eq!(value["rating"], 4.0);
        assert_eq!(value["comment"], "观影记录");
        assert_eq!(value["markedAt"], "2024-01-02");
        assert!(value["createdAt"].is_string());
        assert!(value["updatedAt"].is_string());
    }
    #[test]
    fn watched_uses_one_state_and_keeps_personal_fields_and_date() {
        let dir = tempfile::tempdir().unwrap();
        let conn = open(dir.path()).unwrap();
        let id = new_item(&conn, &json!({"title":"统一已看"})).unwrap();
        update_record(
            &conn,
            &id,
            &json!({"watched":false,"rating":5,"comment":"个人短评","markedAt":"2020-01-02"}),
            true,
        )
        .unwrap();
        update_record(&conn, &id, &json!({"watched":true}), false).unwrap();
        let value = item(&conn, &id).unwrap();
        assert_eq!(value["watched"], true);
        assert_eq!(value["markedAt"], "2020-01-02");
        assert_eq!(value["rating"], 5.0);
        assert_eq!(value["comment"], "个人短评");
        update_record(&conn, &id, &json!({"watched":false}), true).unwrap();
        assert_eq!(item(&conn, &id).unwrap()["watched"], false);
        update_record(
            &conn,
            &id,
            &json!({"watched":true,"rating":2,"comment":"导入短评","recordSource":"douban"}),
            false,
        )
        .unwrap();
        let value = item(&conn, &id).unwrap();
        assert_eq!(value["watched"], true);
        assert_eq!(value["rating"], 5.0);
        assert_eq!(value["comment"], "个人短评");
        let fields: String = conn
            .query_row(
                "SELECT manual_fields_json FROM personal WHERE item_id=?1",
                [&id],
                |r| r.get(0),
            )
            .unwrap();
        assert!(!serde_json::from_str::<HashSet<String>>(&fields)
            .unwrap()
            .contains("watched"));
        let automatic = new_item(&conn, &json!({"title":"播放完成"})).unwrap();
        update_record(&conn, &automatic, &json!({"watched":true}), false).unwrap();
        let value = item(&conn, &automatic).unwrap();
        assert_eq!(
            value["markedAt"],
            chrono::Local::now().date_naive().to_string()
        );
        assert_eq!(value["recordSource"], "playback");
    }
    #[test]
    fn provider_identity_and_manual_data_are_independent() {
        let dir = tempfile::tempdir().unwrap();
        let c = open(dir.path()).unwrap();
        let id = upsert_provider_item(&c, "douban", "subject", "1", &json!({"title":"片名"}), true)
            .unwrap();
        assert_eq!(
            id,
            upsert_provider_item(
                &c,
                "douban",
                "subject",
                "1",
                &json!({"title":"更新名称"}),
                true
            )
            .unwrap()
        );
        update_record(&c, &id, &json!({"rating":5,"comment":"我写的"}), true).unwrap();
        update_record(
            &c,
            &id,
            &json!({"rating":2,"comment":"来源更新","watched":true}),
            false,
        )
        .unwrap();
        let v = item(&c, &id).unwrap();
        assert_eq!(v["rating"], 5.0);
        assert_eq!(v["comment"], "我写的");
        assert_eq!(v["watched"], true);
        assert!(v["playable"] == false);
    }
    #[test]
    fn merge_with_different_local_records_keeps_target_without_prompt() {
        let dir = tempfile::tempdir().unwrap();
        let c = open(dir.path()).unwrap();
        let a = new_item(&c, &json!({"title":"a"})).unwrap();
        let b = new_item(&c, &json!({"title":"b"})).unwrap();
        update_record(&c, &a, &json!({"rating":5}), true).unwrap();
        update_record(&c, &b, &json!({"rating":1}), true).unwrap();
        merge_items(&c, &a, &b).unwrap();
        assert_eq!(items(&c).unwrap().len(), 1);
        assert_eq!(item(&c, &b).unwrap()["rating"], 1.0);
    }
    #[test]
    fn playback_completion_requires_only_valid_position_at_95_percent() {
        assert!(!playback_complete(94.99, 100.0));
        assert!(playback_complete(95.0, 100.0));
        assert!(playback_complete(100.0, 100.0));
        assert!(!playback_complete(0.0, 100.0));
        assert!(!playback_complete(0.0, 0.0));
        assert!(!playback_complete(100.0, -1.0));
        assert!(!playback_complete(f64::INFINITY, 100.0));
        assert!(!playback_complete(f64::NAN, 100.0));
        assert!(!playback_complete(100.0, f64::INFINITY));
    }
    #[test]
    fn merge_prefers_local_fields_over_douban_in_both_directions() {
        for reverse in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let conn = open(dir.path()).unwrap();
            let local = new_item(&conn, &json!({"title":"本地"})).unwrap();
            let imported = new_item(&conn, &json!({"title":"豆瓣"})).unwrap();
            update_record(&conn,&local,&json!({"rating":5,"comment":"本地短评","markedAt":"2026-01-01","recordSource":"manual"}),true).unwrap();
            update_record(&conn,&imported,&json!({"rating":2,"comment":"豆瓣短评","markedAt":"2020-01-01","watched":true,"recordSource":"douban"}),false).unwrap();
            let (source, target) = if reverse {
                (&imported, &local)
            } else {
                (&local, &imported)
            };
            merge_items(&conn, source, target).unwrap();
            let value = item(&conn, target).unwrap();
            assert_eq!(value["rating"], 5.0);
            assert_eq!(value["comment"], "本地短评");
            assert_eq!(value["markedAt"], "2026-01-01");
            assert_eq!(value["watched"], true);
            update_record(
                &conn,
                target,
                &json!({"rating":1,"comment":"再次导入","recordSource":"douban"}),
                false,
            )
            .unwrap();
            assert_eq!(item(&conn, target).unwrap()["rating"], 5.0);
        }
    }
    #[test]
    fn parent_progress_uses_one_version_and_does_not_imply_watched() {
        let dir = tempfile::tempdir().unwrap();
        let conn = open(dir.path()).unwrap();
        let parent = new_item(&conn, &json!({"title":"剧","kind":"series"})).unwrap();
        let episode = new_item(
            &conn,
            &json!({"title":"集","kind":"episode","parentId":parent}),
        )
        .unwrap();
        conn.execute(
            "INSERT INTO sources(id,path,label,created_at)VALUES('s','C:/Media','Media',?1)",
            [now()],
        )
        .unwrap();
        conn.execute("INSERT INTO resources(id,source_id,path,position_seconds,duration_seconds,updated_at)VALUES('a','s','C:/Media/a.mkv',180,200,?1),('b','s','C:/Media/b.mkv',50,100,?1)",
[now()]).unwrap();

        conn.execute(
            "INSERT INTO resource_links(resource_id,item_id)VALUES('a',?1),('b',?1)",
            [&episode],
        )
        .unwrap();
        conn.execute("INSERT INTO view_sessions(id,item_id,resource_id,started_at)VALUES('first',?1,'a','2026-01-01T01:00:00Z'),('latest',?1,'b','2026-01-02T01:00:00Z')",
[&episode]).unwrap();

        let value = item(&conn, &parent).unwrap();
        assert_eq!(value["positionSeconds"], 50.0);
        assert_eq!(value["durationSeconds"], 100.0);
        assert_eq!(value["resumeResourceId"], "b");
        assert_eq!(value["lastPlayed"], "2026-01-02T01:00:00Z");
        assert_eq!(value["watched"], false);
        assert_eq!(value["resourceCount"], 2);
    }
    #[test]
    fn merging_an_ancestor_is_rejected_before_changing_hierarchy() {
        let dir = tempfile::tempdir().unwrap();
        let conn = open(dir.path()).unwrap();
        let series = new_item(&conn, &json!({"title":"剧","kind":"series"})).unwrap();
        let season = new_item(
            &conn,
            &json!({"title":"季","kind":"season","parentId":series}),
        )
        .unwrap();
        let episode = new_item(
            &conn,
            &json!({"title":"集","kind":"episode","parentId":season}),
        )
        .unwrap();
        assert!(merge_items(&conn, &series, &episode).is_err());
        assert_eq!(items(&conn).unwrap().len(), 3);
    }
    #[test]
    fn unplayed_series_starts_with_first_episode_instead_of_resource_uuid() {
        let dir = tempfile::tempdir().unwrap();
        let conn = open(dir.path()).unwrap();
        let series = new_item(&conn, &json!({"title":"剧","kind":"series"})).unwrap();
        let season = new_item(
            &conn,
            &json!({"title":"第一季","kind":"season","parentId":series,"seasonNumber":1}),
        )
        .unwrap();
        let first = new_item(
            &conn,
            &json!({"title":"第一集","kind":"episode","parentId":season,"episodeNumber":1}),
        )
        .unwrap();
        let later = new_item(
            &conn,
            &json!({"title":"第十八集","kind":"episode","parentId":season,"episodeNumber":18}),
        )
        .unwrap();
        conn.execute(
            "INSERT INTO sources(id,path,label,created_at)VALUES('s','C:/Media','Media',?1)",
            [now()],
        )
        .unwrap();
        conn.execute("INSERT INTO resources(id,source_id,path,duration_seconds,updated_at)VALUES('z-first','s','C:/Media/z-first.mkv',120,?1),('a-eighteen','s','C:/Media/a-eighteen.mkv',180,?1)",
[now()]).unwrap();

        conn.execute(
            "INSERT INTO resource_links(resource_id,item_id)VALUES('z-first',?1),('a-eighteen',?2)",
            params![first, later],
        )
        .unwrap();
        let item = item(&conn, &series).unwrap();
        assert_eq!(item["resumeResourceId"], "z-first");
        assert_eq!(item["durationSeconds"], 120.0);
        assert_eq!(item["positionSeconds"], 0.0);
    }
    #[test]
    fn playable_version_precedes_offline_progress_without_mixing_durations() {
        let dir = tempfile::tempdir().unwrap();
        let conn = open(dir.path()).unwrap();
        let movie = new_item(&conn, &json!({"title":"多版本电影"})).unwrap();
        conn.execute(
            "INSERT INTO sources(id,path,label,created_at)VALUES('s','C:/Media','Media',?1)",
            [now()],
        )
        .unwrap();
        conn.execute("INSERT INTO resources(id,source_id,path,available,position_seconds,duration_seconds,updated_at)VALUES('offline','s','C:/Media/a.mkv',0,90,100,?1),('online','s','C:/Media/z.mkv',1,30,200,?1)",
[now()]).unwrap();

        conn.execute(
            "INSERT INTO resource_links(resource_id,item_id)VALUES('offline',?1),('online',?1)",
            [&movie],
        )
        .unwrap();
        conn.execute("INSERT INTO view_sessions(id,item_id,resource_id,started_at)VALUES('latest',?1,'offline','2026-10-25T01:00:00Z'),('older',?1,'online','2026-10-24T01:00:00Z')",
[&movie]).unwrap();

        let online = item(&conn, &movie).unwrap();
        assert_eq!(online["resumeResourceId"], "online");
        assert_eq!(online["positionSeconds"], 30.0);
        assert_eq!(online["durationSeconds"], 200.0);
        assert_eq!(online["playable"], true);
        conn.execute("UPDATE resources SET available=0", [])
            .unwrap();
        let offline = item(&conn, &movie).unwrap();
        assert_eq!(offline["playable"], false);
        assert!(offline["resumeResourceId"].is_null());
        assert_eq!(offline["positionSeconds"], 90.0);
        assert_eq!(offline["durationSeconds"], 100.0);
    }
}
