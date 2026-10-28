use crate::media::store;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};

pub(in crate::media) struct Source {
    pub account: String,
    pub subject: String,
    pub raw: Value,
}
pub(in crate::media) fn valid_imdb_id(id: &str) -> bool {
    id.strip_prefix("tt").is_some_and(|digits| {
        (7..=12).contains(&digits.len()) && digits.bytes().all(|c| c.is_ascii_digit())
    })
}
pub(in crate::media) fn imdb_id(raw: &Value) -> &str {
    store::text(&raw["detail"], "imdbId").trim()
}
pub(in crate::media) fn source(conn: &Connection, item: &str) -> Result<Option<Source>, String> {
    let row:Option<(String,String,String)>=conn.query_row(
        "SELECT d.account_key,d.subject_id,d.raw_json FROM douban_source_data d JOIN external_ids e ON e.provider='douban' AND e.provider_type='subject' AND e.external_id=d.subject_id WHERE e.item_id=?1 ORDER BY d.account_key,d.subject_id LIMIT 1",
        [item],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(|e|e.to_string())?;
    row.map(|(account, subject, raw)| {
        if subject.is_empty() || !subject.bytes().all(|c| c.is_ascii_digit()) {
            return Err("豆瓣条目编号无效".into());
        }
        Ok(Source {
            account,
            subject,
            raw: serde_json::from_str(&raw).map_err(|_| "豆瓣来源资料无效")?,
        })
    })
    .transpose()
}
pub(in crate::media) fn due(raw: &Value) -> bool {
    if valid_imdb_id(imdb_id(raw)) {
        return false;
    }
    let retry =
        if raw["detail"].is_object() && raw["detailError"].as_str().is_none_or(str::is_empty) {
            7 * 86400
        } else {
            900
        };
    store::text(raw, "detailCheckedAt")
        .parse::<DateTime<Utc>>()
        .ok()
        .is_none_or(|time| (Utc::now() - time).num_seconds() >= retry)
}
pub(in crate::media) fn save(
    conn: &Connection,
    item: &str,
    source: &Source,
    result: &Result<Value, String>,
) -> Result<(), String> {
    // 重新读取原始资料，避免网络等待期间覆盖其他补充结果；不更改列表或个人字段。
    let mut raw:Value=conn.query_row("SELECT raw_json FROM douban_source_data WHERE account_key=?1 AND subject_id=?2 AND EXISTS(SELECT 1 FROM external_ids WHERE provider='douban' AND external_id=?2 AND item_id=?3)",params![source.account,source.subject,item],|r|r.get::<_,String>(0)).optional().map_err(|e|e.to_string())?
        .map(|v|serde_json::from_str(&v).map_err(|_|"豆瓣来源资料无效")).transpose()?.ok_or("豆瓣来源身份已变化")?;
    if valid_imdb_id(imdb_id(&raw)) {
        return Ok(());
    }
    raw["detailCheckedAt"] = json!(store::now());
    raw["detailError"] = json!(result.as_ref().err().map(String::as_str).unwrap_or(""));
    if let Ok(detail) = result {
        raw["detail"] = detail.clone();
    }
    conn.execute(
        "UPDATE douban_source_data SET raw_json=?3 WHERE account_key=?1 AND subject_id=?2",
        params![source.account, source.subject, raw.to_string()],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// IMDb 是来源线索而非全局唯一条目身份：多季可能共享整剧的同一个 IMDb 编号。
pub(in crate::media) fn attach_imdb_ids(
    conn: &Connection,
    items: &mut [Value],
    item_ids_json: Option<&str>,
) -> Result<(), String> {
    let mut ids = std::collections::HashMap::<String, std::collections::HashSet<String>>::new();
    let mut sql="SELECT e.item_id,d.raw_json FROM douban_source_data d JOIN external_ids e ON e.provider='douban' AND e.provider_type='subject' AND e.external_id=d.subject_id".to_string();
    if item_ids_json.is_some() {
        sql.push_str(" WHERE e.item_id IN(SELECT value FROM json_each(?1))");
    }
    let mut statement = conn.prepare(&sql).map_err(|e| e.to_string())?;
    for row in statement
        .query_map(rusqlite::params_from_iter(item_ids_json), |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })
        .map_err(|e| e.to_string())?
    {
        let (item, raw) = row.map_err(|e| e.to_string())?;
        let raw: Value = serde_json::from_str(&raw).unwrap_or(Value::Null);
        let id = imdb_id(&raw);
        if valid_imdb_id(id) {
            ids.entry(item).or_default().insert(id.into());
        }
    }
    for item in items {
        item["imdbId"] = json!(ids
            .get(store::text(item, "id"))
            .filter(|set| set.len() == 1)
            .and_then(|set| set.iter().next())
            .map(String::as_str)
            .unwrap_or(""));
    }
    Ok(())
}
