//! 资源移除、扫描确认缺失与路径忽略共用的业务清理；调用方持有事务。
use super::store;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use std::collections::{BTreeSet, HashSet};

pub(super) fn recorded(conn: &Connection, id: &str) -> Result<bool, String> {
    conn.query_row("SELECT EXISTS(SELECT 1 FROM media_items WHERE id=?1 AND independent=1)
      OR EXISTS(SELECT 1 FROM personal WHERE item_id=?1 AND (watched<>0 OR favorite<>0 OR rating IS NOT NULL OR TRIM(comment)<>'' OR marked_at<>'' OR last_played<>'' OR record_source<>'' OR manual_fields_json<>'[]'))
      OR EXISTS(SELECT 1 FROM collection_items WHERE item_id=?1)
      OR EXISTS(SELECT 1 FROM media_item_tags WHERE item_id=?1)
      OR EXISTS(SELECT 1 FROM view_sessions WHERE item_id=?1)
      OR EXISTS(SELECT 1 FROM external_ids WHERE item_id=?1 AND provider<>'tmdb')
      OR EXISTS(SELECT 1 FROM douban_records WHERE item_id=?1)
      OR EXISTS(SELECT 1 FROM resource_links l JOIN resources r ON r.id=l.resource_id WHERE l.item_id=?1 AND (r.position_seconds>0 OR EXISTS(SELECT 1 FROM media_playback_coverage c WHERE c.resource_id=r.id AND c.segments_json<>'[]')))",
      [id], |row| row.get(0)).map_err(|e|e.to_string())
}

fn item(conn: &Connection, id: &str) -> Result<Option<(String, Option<String>)>, String> {
    conn.query_row(
        "SELECT kind,parent_id FROM media_items WHERE id=?1",
        [id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .optional()
    .map_err(|e| e.to_string())
}

fn children(conn: &Connection, id: &str) -> Result<Vec<String>, String> {
    let mut stmt = conn.prepare("WITH RECURSIVE tree(id)AS(SELECT ?1 UNION SELECT m.id FROM media_items m JOIN tree t ON m.parent_id=t.id)SELECT id FROM tree").map_err(|e|e.to_string())?;
    let rows = stmt
        .query_map([id], |row| row.get(0))
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

fn active_resource(conn: &Connection, id: &str) -> Result<bool, String> {
    conn.query_row("SELECT EXISTS(SELECT 1 FROM resource_links l JOIN resources r ON r.id=l.resource_id WHERE l.item_id=?1 AND r.missing=0)",[id],|row|row.get(0)).map_err(|e|e.to_string())
}

pub(super) fn delete_item(conn: &Connection, id: &str) -> Result<(), String> {
    for table in ["media_match_errors", "resource_links", "personal", "external_ids"] {
        conn.execute(&format!("DELETE FROM {table} WHERE item_id=?1"), [id])
            .map_err(|e| e.to_string())?;
    }
    conn.execute("DELETE FROM media_items WHERE id=?1", [id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn empty_parents(conn: &Connection, mut parent: Option<String>) -> Result<usize, String> {
    let mut seen = HashSet::new();
    let mut removed = 0;
    while let Some(id) = parent {
        if !seen.insert(id.clone()) {
            break;
        }
        let Some((_, next)) = item(conn, &id)? else {
            break;
        };
        let has_children: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM media_items WHERE parent_id=?1)",
                [&id],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())?;
        if has_children || recorded(conn, &id)? || active_resource(conn, &id)? {
            break;
        }
        delete_item(conn, &id)?;
        removed += 1;
        parent = next;
    }
    Ok(removed)
}

/// 仅清理指定条目及其所属季树、必要的空父级，不改资源状态，不扫描无关条目。
pub(super) fn orphaned(conn: &Connection, item_ids: &[String]) -> Result<usize, String> {
    clean_orphaned(conn, item_ids, true)
}
/// 人工重绑后只清理空壳，不重置同季其他文件的识别失败／退避状态。
pub(super) fn orphaned_after_relink(conn: &Connection, item_ids: &[String]) -> Result<usize, String> {
    clean_orphaned(conn, item_ids, false)
}
fn clean_orphaned(conn: &Connection, item_ids: &[String], clear_errors: bool) -> Result<usize, String> {
    let mut roots = BTreeSet::new();
    // 在删除前收集整个批次的根，避免共享父剧因先后顺序产生漏清理。
    for id in item_ids {
        let mut root = id.clone();
        let mut seen = HashSet::new();
        while seen.insert(root.clone()) {
            let Some((kind, parent)) = item(conn, &root)? else {
                break;
            };
            if kind == "episode" {
                if let Some(parent) = parent {
                    root = parent;
                    continue;
                }
            }
            roots.insert(root);
            break;
        }
    }
    let mut removed = 0;
    for root in roots {
        let Some((_, parent)) = item(conn, &root)? else {
            continue;
        };
        let tree = children(conn, &root)?;
        let mut keep = false;
        for id in &tree {
            if clear_errors {
                conn.execute("DELETE FROM media_match_errors WHERE item_id=?1", [id])
                    .map_err(|e| e.to_string())?;
            }
            if recorded(conn, id)? {
                keep = true;
                if clear_errors {
                    conn.execute(
                        "UPDATE media_items SET independent=1 WHERE id=?1 AND independent<>1",
                        [id],
                    )
                    .map_err(|e| e.to_string())?;
                }
            }
            keep |= active_resource(conn, id)?;
        }
        if keep {
            continue;
        }
        for id in tree.iter().rev() {
            delete_item(conn, id)?;
            removed += 1;
        }
        removed += empty_parents(conn, parent)?;
    }
    Ok(removed)
}

fn resource_recorded(conn: &Connection, id: &str) -> Result<bool, String> {
    conn.query_row("SELECT EXISTS(SELECT 1 FROM view_sessions WHERE resource_id=?1)
      OR EXISTS(SELECT 1 FROM resources WHERE id=?1 AND position_seconds>0)
      OR EXISTS(SELECT 1 FROM media_playback_coverage WHERE resource_id=?1 AND segments_json<>'[]')",[id],|row|row.get(0)).map_err(|e|e.to_string())
}

/// 调用方事务内先停用所有受影响资源，再清理无记录支撑的关联作品和资源。
/// 不读取路径或磁盘，已删除 ID 安全跳过，保留资源的重复调用不改时间戳。
pub(super) fn resources(conn: &Connection, ids: &[String]) -> Result<Value, String> {
    let mut affected = BTreeSet::new();
    for id in ids {
        let exists: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM resources WHERE id=?1)",
                [id],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())?;
        if exists {
            affected.insert(id.clone());
        }
    }
    let mut item_ids = BTreeSet::new();
    for id in &affected {
        conn.execute("UPDATE resources SET available=0,missing=1,updated_at=?2 WHERE id=?1 AND (available<>0 OR missing<>1)",params![id,store::now()]).map_err(|e|e.to_string())?;
        let mut stmt = conn
            .prepare("SELECT item_id FROM resource_links WHERE resource_id=?1")
            .map_err(|e| e.to_string())?;
        for item in stmt
            .query_map([id], |row| row.get::<_, String>(0))
            .map_err(|e| e.to_string())?
        {
            item_ids.insert(item.map_err(|e| e.to_string())?);
        }
    }
    let removed = orphaned(conn, &item_ids.into_iter().collect::<Vec<_>>())?;
    for id in &affected {
        let mut stmt = conn
            .prepare("SELECT item_id FROM resource_links WHERE resource_id=?1")
            .map_err(|e| e.to_string())?;
        let linked = stmt
            .query_map([id], |row| row.get::<_, String>(0))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        drop(stmt);
        let mut retained = resource_recorded(conn, id)?;
        for item in linked {
            retained |= recorded(conn, &item)?;
        }
        if !retained {
            conn.execute("DELETE FROM resource_links WHERE resource_id=?1", [id])
                .map_err(|e| e.to_string())?;
            conn.execute(
                "DELETE FROM media_playback_coverage WHERE resource_id=?1",
                [id],
            )
            .map_err(|e| e.to_string())?;
            conn.execute("DELETE FROM resources WHERE id=?1 AND NOT EXISTS(SELECT 1 FROM resource_links WHERE resource_id=?1) AND NOT EXISTS(SELECT 1 FROM view_sessions WHERE resource_id=?1)",[id]).map_err(|e|e.to_string())?;
        }
    }
    Ok(json!({"resources":affected.len(),"items":removed}))
}

#[cfg(test)]
mod tests;
