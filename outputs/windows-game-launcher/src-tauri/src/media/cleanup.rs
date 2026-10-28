//! 显式清理无资源条目；记录删除必须经预览及单独确认。
use super::{removal, store, State};
use rusqlite::{Connection, TransactionBehavior};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use tauri::{Emitter, Manager};
#[cfg(test)]
mod tests;

fn rows(conn: &Connection, sql: &str, ids: &str) -> Result<Vec<Value>, String> {
    let mut statement = conn.prepare(sql).map_err(|e| e.to_string())?;
    let names: Vec<String> = statement
        .column_names()
        .into_iter()
        .map(str::to_owned)
        .collect();
    let values = statement
        .query_map([ids], |row| {
            let mut object = serde_json::Map::new();
            for (i, name) in names.iter().enumerate() {
                use rusqlite::types::ValueRef;
                let value = match row.get_ref(i)? {
                    ValueRef::Null => Value::Null,
                    ValueRef::Integer(n) => json!(n),
                    ValueRef::Real(n) => json!(n),
                    ValueRef::Text(s) => json!(String::from_utf8_lossy(s)),
                    ValueRef::Blob(_) => return Err(rusqlite::Error::InvalidQuery),
                };
                object.insert(name.clone(), value);
            }
            Ok(Value::Object(object))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string());
    values
}
fn scalar(conn: &Connection, sql: &str, ids: &str) -> Result<i64, String> {
    conn.query_row(sql, [ids], |r| r.get(0))
        .map_err(|e| e.to_string())
}
fn entry(node: &Value, scope: &str) -> Value {
    json!({"id":node["id"],"parentId":node["parent_id"],"title":node["title"],"kind":node["kind"],"scope":scope})
}

fn preview(conn: &Connection, id: &str) -> Result<Value, String> {
    let mut nodes=rows(conn,"WITH RECURSIVE tree(id)AS(SELECT id FROM media_items WHERE id=?1 UNION SELECT m.id FROM media_items m JOIN tree t ON m.parent_id=t.id)SELECT m.* FROM media_items m JOIN tree t ON t.id=m.id ORDER BY m.id",id)?;
    let root = nodes
        .iter()
        .find(|node| node["id"] == id)
        .cloned()
        .ok_or("条目已不存在，请刷新影视库")?;
    let mut included: HashSet<String> = nodes
        .iter()
        .map(|node| store::text(node, "id").to_string())
        .collect();
    let mut entries: Vec<Value> = nodes
        .iter()
        .map(|node| {
            entry(
                node,
                if node["id"] == id {
                    "selected"
                } else {
                    "child"
                },
            )
        })
        .collect();
    // 只把无其他子项、没有自身记录的空父级纳入清理；父级有记录时保留。
    let mut parent = root["parent_id"].as_str().map(str::to_owned);
    while let Some(id) = parent {
        if included.contains(&id) {
            return Err("条目层级存在循环，无法安全清理".into());
        }
        let parent_nodes = rows(conn, "SELECT * FROM media_items WHERE id=?1", &id)?;
        let Some(node) = parent_nodes.into_iter().next() else {
            break;
        };
        let children = rows(
            conn,
            "SELECT id FROM media_items WHERE parent_id=?1 ORDER BY id",
            &id,
        )?;
        if children
            .iter()
            .any(|child| !included.contains(store::text(child, "id")))
            || removal::recorded(conn, &id)?
            || scalar(
                conn,
                "SELECT COUNT(*) FROM resource_links WHERE item_id=?1",
                &id,
            )? > 0
        {
            break;
        }
        parent = node["parent_id"].as_str().map(str::to_owned);
        included.insert(id);
        entries.push(entry(&node, "emptyParent"));
        nodes.push(node);
    }
    nodes.sort_by(|a, b| store::text(a, "id").cmp(store::text(b, "id")));
    entries.sort_by(|a, b| {
        store::text(a, "scope")
            .cmp(store::text(b, "scope"))
            .then_with(|| store::text(a, "title").cmp(store::text(b, "title")))
            .then_with(|| store::text(a, "id").cmp(store::text(b, "id")))
    });
    let ids = json!(included.into_iter().collect::<BTreeSet<_>>()).to_string();
    let mut snapshot = BTreeMap::new();
    snapshot.insert("items", nodes);
    for (table, order) in [
        ("personal", "item_id"),
        ("external_ids", "provider,provider_type,external_id"),
        ("resource_links", "resource_id,item_id"),
        ("view_sessions", "id"),
        ("collection_items", "collection_id,item_id"),
        ("media_item_tags", "item_id,tag_id"),
        ("douban_records", "account_key,subject_id"),
        ("media_match_errors", "item_id"),
        ("media_match_retry", "item_id"),
        ("media_item_metadata", "item_id"),
    ] {
        snapshot.insert(table,rows(conn,&format!("SELECT * FROM {table} WHERE item_id IN(SELECT value FROM json_each(?1)) ORDER BY {order}"),&ids)?);
    }
    let subjects: BTreeSet<String> = snapshot["external_ids"]
        .iter()
        .filter(|v| v["provider"] == "douban" && v["provider_type"] == "subject")
        .map(|v| store::text(v, "external_id").to_string())
        .chain(
            snapshot["douban_records"]
                .iter()
                .map(|v| store::text(v, "subject_id").to_string()),
        )
        .collect();
    let source_rows=rows(conn,"SELECT * FROM douban_source_data WHERE subject_id IN(SELECT value FROM json_each(?1)) ORDER BY account_key,subject_id",&json!(subjects).to_string())?;
    snapshot.insert("douban_source_data", source_rows);
    let linked=scalar(conn,"SELECT COUNT(DISTINCT resource_id) FROM resource_links WHERE item_id IN(SELECT value FROM json_each(?1))",&ids)?;
    let personal=scalar(conn,"SELECT COUNT(*) FROM personal WHERE item_id IN(SELECT value FROM json_each(?1)) AND (watched<>0 OR favorite<>0 OR rating IS NOT NULL OR TRIM(comment)<>'' OR marked_at<>'' OR last_played<>'' OR record_source<>'' OR manual_fields_json<>'[]')",&ids)?;
    let count = |predicate: &str| -> Result<i64, String> {
        scalar(conn,&format!("SELECT COUNT(*) FROM personal WHERE item_id IN(SELECT value FROM json_each(?1)) AND ({predicate})"),&ids)
    };
    let summary = json!({"items":entries.len(),"parents":entries.iter().filter(|v|v["scope"]=="emptyParent").count(),"personal":personal,"watched":count("watched<>0")?,"favorites":count("favorite<>0")?,"ratings":count("rating IS NOT NULL")?,"comments":count("TRIM(comment)<>''")?,"sessions":snapshot["view_sessions"].len(),"doubanRecords":snapshot["douban_records"].len(),"collectionLinks":snapshot["collection_items"].len(),"tagLinks":snapshot["media_item_tags"].len(),"linkedResources":linked});
    let has_records = personal > 0
        || [
            "view_sessions",
            "douban_records",
            "collection_items",
            "media_item_tags",
        ]
        .iter()
        .any(|table| !snapshot[*table].is_empty());
    let fingerprint = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&snapshot).map_err(|e| e.to_string())?)
    );
    Ok(
        json!({"itemId":id,"allowed":linked==0,"entries":entries,"summary":summary,"hasRecords":has_records,"hasDoubanSource":!subjects.is_empty(),"fingerprint":fingerprint,"blockedReason":if linked>0{"有关联文件资源（包含离线或已忽略的文件），请先解除文件关联再清理"}else{""}}),
    )
}
fn apply(conn: &mut Connection, input: &Value) -> Result<Value, String> {
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    let current = preview(&tx, store::text(input, "itemId"))?;
    if current["allowed"] != true {
        return Err(store::text(&current, "blockedReason").into());
    }
    if input["fingerprint"].as_str().is_none() || current["fingerprint"] != input["fingerprint"] {
        return Err("条目、子项或相关记录已变化，请重新预览后确认".into());
    }
    if current["hasRecords"] == true && input["deleteRecords"] != true {
        return Err("请先勾选同时删除相关记录，尚未清理任何条目".into());
    }
    let entries = current["entries"].as_array().unwrap();
    let ids: Vec<String> = entries
        .iter()
        .map(|v| store::text(v, "id").to_string())
        .collect();
    let json_ids = json!(ids).to_string();
    let subjects:Vec<String>=rows(&tx,"SELECT external_id AS subject_id FROM external_ids WHERE provider='douban' AND provider_type='subject' AND item_id IN(SELECT value FROM json_each(?1)) UNION SELECT subject_id FROM douban_records WHERE item_id IN(SELECT value FROM json_each(?1)) ORDER BY subject_id",&json_ids)?.iter().map(|v|store::text(v,"subject_id").to_string()).collect();
    for table in [
        "view_sessions",
        "douban_records",
        "collection_items",
        "media_item_tags",
    ] {
        tx.execute(
            &format!("DELETE FROM {table} WHERE item_id IN(SELECT value FROM json_each(?1))"),
            [&json_ids],
        )
        .map_err(|e| e.to_string())?;
    }
    let mut remaining: BTreeSet<String> = ids.into_iter().collect();
    while !remaining.is_empty() {
        let leaves: Vec<String> = remaining
            .iter()
            .filter(|id| {
                !entries.iter().any(|entry| {
                    entry["parentId"] == **id && remaining.contains(store::text(entry, "id"))
                })
            })
            .cloned()
            .collect();
        if leaves.is_empty() {
            return Err("条目层级存在循环，尚未清理任何条目".into());
        }
        for id in leaves {
            removal::delete_item(&tx, &id)?;
            remaining.remove(&id);
        }
    }
    // 来源原始资料只有在不再被其他条目或账号记录引用时才清理。
    tx.execute("DELETE FROM douban_source_data WHERE subject_id IN(SELECT value FROM json_each(?1)) AND NOT EXISTS(SELECT 1 FROM external_ids e WHERE e.provider='douban' AND e.external_id=douban_source_data.subject_id) AND NOT EXISTS(SELECT 1 FROM douban_records d WHERE d.subject_id=douban_source_data.subject_id)",[json!(subjects).to_string()]).map_err(|e|e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(json!({"removed":current["summary"]["items"],"summary":current["summary"]}))
}
pub async fn command(app: tauri::AppHandle, input: Value) -> Result<Value, String> {
    let operation = super::operation(&app).await?;
    let is_apply = input["action"] == "apply";
    if is_apply && super::playback::is_active(&app) {
        return Err("请先停止影视播放，再清理条目".into());
    }
    let dir = app.state::<State>().dir.clone();
    let action = store::text(&input, "action").to_string();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let _operation = operation;
        let mut conn = store::open(&dir)?;
        match action.as_str() {
            "preview" => {
                let tx = conn.transaction().map_err(|e| e.to_string())?;
                let result = preview(&tx, store::text(&input, "itemId"))?;
                tx.commit().map_err(|e| e.to_string())?;
                Ok(result)
            }
            "apply" => apply(&mut conn, &input),
            _ => Err("清理操作无效".into()),
        }
    })
    .await
    .map_err(|_| "清理条目失败")??;
    if is_apply {
        let _ = app.emit("media-changed", ());
    }
    Ok(result)
}
