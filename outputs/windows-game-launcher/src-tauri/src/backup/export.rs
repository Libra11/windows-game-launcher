use super::{model::*, schema};
use crate::db;
use rusqlite::{types::ValueRef, Connection};
use serde_json::Value;

pub fn capture(conn: &mut Connection, appearance: Appearance, time: &str) -> Result<(Dataset, Preferences), String> {
    appearance.validate()?;
    let tx = conn.transaction().map_err(|error| error.to_string())?;
    let mut data = Dataset { snapshot_at:time.into(), ..Default::default() };
    for table in schema::TABLES {
        let mut stmt = tx.prepare(&format!("SELECT {} FROM {}", table.columns.join(","), table.name)).map_err(|error| error.to_string())?;
        let mut rows = stmt.query([]).map_err(|error| error.to_string())?;
        let mut saved = Vec::new();
        while let Some(row) = rows.next().map_err(|error| error.to_string())? {
            let mut values = Vec::new();
            for index in 0..table.columns.len() {
                values.push(match row.get_ref(index).map_err(|error| error.to_string())? {
                    ValueRef::Null => Value::Null,
                    ValueRef::Integer(value) => value.into(),
                    ValueRef::Text(text) => std::str::from_utf8(text).map_err(|_| "记录包含无效字符")?.into(),
                    _ => return Err("记录包含不支持的数值或二进制字段".into()),
                });
            }
            if table.name == "games" {
                values[8] = "待重新检测".into(); values[9] = "".into(); values[10] = "".into();
                let mut meta: Value = serde_json::from_str(values[7].as_str().unwrap()).map_err(|_| "游戏资料无效")?;
                if let Some(object) = meta.as_object_mut() { object.remove("localArtwork"); object.remove("epicAchievementSyncError"); }
                values[7] = meta.to_string().into();
            }
            // 只结束备份副本的会话，不改变正在运行的游戏或原记录。
            if table.name == "play_sessions" && values[3].as_str() == Some("") { values[3] = time.into(); }
            saved.push(values);
        }
        data.tables.insert(table.name.into(), saved);
    }
    {
        let ids: std::collections::HashSet<_> = data.tables["games"].iter().filter_map(|row| row[0].as_str()).map(str::to_owned).collect();
        let mut stmt = tx.prepare("SELECT key,value FROM settings").map_err(|error| error.to_string())?;
        let rows = stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))).map_err(|error| error.to_string())?;
        for row in rows {
            let (key, mut value) = row.map_err(|error| error.to_string())?;
            if !schema::portable_setting(&key) { continue; }
            if key.strip_prefix("achievement_platform:").is_some_and(|id| !ids.contains(id)) { continue; }
            if let Some(cover) = key.strip_prefix("cover_cache:") {
                if !cover.rsplit_once(':').is_some_and(|(id, role)| ids.contains(id) && ["portrait","hero"].contains(&role)) { continue; }
            }
            if key.starts_with("statistics_official_baseline:") && !ids.iter().any(|id| key.starts_with(&format!("statistics_official_baseline:{id}:"))) { continue; }
            if key.starts_with("steam_playtime:") || key.starts_with("steam_family_playtime:") {
                let mut cache: Value = serde_json::from_str(&value).map_err(|_| "个人时长缓存无效")?;
                cache["error"] = "".into(); value = cache.to_string();
            }
            if key.starts_with("achievement_platform:") {
                let mut profile: Value = serde_json::from_str(&value).map_err(|_| "成就平台关联无效")?;
                profile["definitionError"] = "".into(); profile["recordConflicts"] = serde_json::json!([]);
                value = profile.to_string();
            }
            data.settings.insert(key, value);
        }
    }
    let mut behavior = std::collections::BTreeMap::new();
    for key in schema::BEHAVIOR { behavior.insert((*key).into(), db::setting(&tx, key)?); }
    let preferences = Preferences { appearance, behavior };
    schema::validate(&data, &preferences)?;
    tx.commit().map_err(|error| error.to_string())?;
    Ok((data, preferences))
}
