//! 只提交资源实际变化；目录枚举和任务心跳与业务行更新分开。
use super::{ignored, require_enabled, stamp, store};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde_json::{json, Value};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

struct Resource {
    id: String,
    source: String,
    size: i64,
    modified: String,
    available: bool,
    missing: bool,
    linked: bool,
}
impl Resource {
    fn file_changed(&self, size: i64, modified: &str) -> bool {
        self.size != size || self.modified != modified
    }
    fn needs_update(&self, source: &str, size: i64, modified: &str) -> bool {
        self.source != source
            || self.file_changed(size, modified)
            || !self.available
            || self.missing
            || !self.linked
    }
}
fn resource(conn: &Connection, path: &str) -> Result<Option<Resource>, String> {
    conn.query_row(
        "SELECT r.id,r.source_id,r.size,r.modified_at,r.available,r.missing,
      EXISTS(SELECT 1 FROM resource_links l WHERE l.resource_id=r.id)
      FROM resources r WHERE r.path=?1 COLLATE NOCASE",
        [path],
        |r| {
            Ok(Resource {
                id: r.get(0)?,
                source: r.get(1)?,
                size: r.get(2)?,
                modified: r.get(3)?,
                available: r.get(4)?,
                missing: r.get(5)?,
                linked: r.get(6)?,
            })
        },
    )
    .optional()
    .map_err(|e| e.to_string())
}

struct FinalizePlan {
    missing: Vec<String>,
    complete: bool,
    marker_unreadable: bool,
}

fn plan_finalization(
    conn: &Connection,
    source_id: &str,
    seen: &HashSet<String>,
    complete: bool,
    mut is_ignored: impl FnMut(&Connection, &str, &str) -> Result<bool, String>,
) -> Result<FinalizePlan, String> {
    let old = {
        let mut stmt = conn
            .prepare(
                "SELECT id,path FROM resources WHERE source_id=?1 AND (available<>0 OR missing<>1)",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([source_id], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?
    };
    // 标记可能在该文件处理完以后才出现，seen 并不代表它在收尾时仍可收录。
    // 先检查整个待确认集合，再处理普通缺失；权限失败不能因检查顺序误标其他资源。
    let mut marker_unreadable = false;
    let checked: Vec<_> = old
        .into_iter()
        .map(|(id, path)| {
            let ignored = match is_ignored(conn, source_id, &path) {
                Ok(value) => Some(value),
                Err(_) => {
                    marker_unreadable = true;
                    None
                }
            };
            (id, path, ignored)
        })
        .collect();
    let complete = complete && !marker_unreadable;
    let mut missing = Vec::new();
    for (id, path, ignored) in checked {
        let excluded = ignored == Some(true)
            || (ignored == Some(false) && complete && !seen.contains(&path.to_ascii_lowercase()));
        if excluded {
            missing.push(id);
        }
    }
    Ok(FinalizePlan {
        missing,
        complete,
        marker_unreadable,
    })
}

fn apply_finalization(
    conn: &Connection,
    source_id: &str,
    plan: &FinalizePlan,
) -> Result<bool, String> {
    let mut affected = Vec::new();
    for id in &plan.missing {
        let still_owned: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM resources WHERE id=?1 AND source_id=?2 AND (available<>0 OR missing<>1))",params![id,source_id],|row|row.get(0)).map_err(|e|e.to_string())?;
        if still_owned {
            affected.push(id.clone());
        }
    }
    if affected.is_empty() {
        return Ok(false);
    }
    super::removal::resources(conn, &affected)?;
    Ok(true)
}

pub(super) fn apply(
    conn: &mut Connection,
    source_id: &str,
    job_id: &str,
    files: &[(PathBuf, fs::Metadata)],
    complete: bool,
) -> Result<Value, String> {
    let mut seen = HashSet::new();
    let mut changed = Vec::new();
    let mut new_items = Vec::new();
    let mut included = 0;
    let mut progress_at = Instant::now();
    for (index, (path, meta)) in files.iter().enumerate() {
        let path = path.to_string_lossy().to_string();
        require_enabled(conn, source_id)?;
        if ignored::contains(conn, source_id, &path)? {
            continue;
        }
        seen.insert(path.to_ascii_lowercase());
        included += 1;
        let size = meta.len() as i64;
        let modified = stamp(meta);
        // 常见的不变文件只读检查，不竞争 SQLite 写锁或更新 updated_at。
        let previous = resource(conn, &path)?;
        if previous
            .as_ref()
            .is_none_or(|r| r.needs_update(source_id, size, &modified))
        {
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(|e| e.to_string())?;
            require_enabled(&tx, source_id)?;
            // 等待写锁期间，停用、忽略或其他任务可能已提交，必须重新检查。
            if ignored::contains(&tx, source_id, &path)? {
                included -= 1;
                seen.remove(&path.to_ascii_lowercase());
                continue;
            }
            let previous = resource(&tx, &path)?;
            let fresh = previous
                .as_ref()
                .is_none_or(|r| r.file_changed(size, &modified));
            let id = previous
                .as_ref()
                .map(|r| r.id.clone())
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
            let resource_changed = previous
                .as_ref()
                .is_none_or(|r| r.source != source_id || fresh || !r.available || r.missing);
            if resource_changed {
                // 特征变更只清当前资源的技术资料和进度，不动观影记录与历史会话。
                if fresh && previous.is_some() {
                    tx.execute(
                        "DELETE FROM media_playback_coverage WHERE resource_id=?1",
                        [&id],
                    )
                    .map_err(|e| e.to_string())?;
                }
                tx.execute("INSERT INTO resources(id,source_id,path,size,modified_at,available,missing,updated_at)VALUES(?1,?2,?3,?4,?5,1,0,?6)
                  ON CONFLICT(id)DO UPDATE SET source_id=excluded.source_id,size=excluded.size,modified_at=excluded.modified_at,available=1,missing=0,updated_at=excluded.updated_at,
                  position_seconds=CASE WHEN ?7 THEN 0 ELSE resources.position_seconds END,
                  duration_seconds=CASE WHEN ?7 THEN 0 ELSE resources.duration_seconds END,
                  technical_json=CASE WHEN ?7 THEN '{}' ELSE resources.technical_json END",
                  params![id,source_id,path,size,modified,store::now(),fresh]).map_err(|e|e.to_string())?;
            }
            if previous.as_ref().is_none_or(|r| !r.linked) {
                let title = Path::new(&path)
                    .file_name()
                    .map(|v| v.to_string_lossy().to_string())
                    .unwrap_or_default();
                let item_id = store::new_item(&tx, &json!({"title":title,"kind":"other"}))?;
                tx.execute(
                    "INSERT INTO resource_links(resource_id,item_id)VALUES(?1,?2)",
                    params![id, item_id],
                )
                .map_err(|e| e.to_string())?;
                new_items.push(item_id);
            }
            tx.commit().map_err(|e| e.to_string())?;
            if fresh {
                changed.push(id);
            }
        }
        // 进度与业务更新无关，按时间节流，快速的不变扫描无需逐十项写心跳。
        if progress_at.elapsed() >= Duration::from_millis(250) {
            store::job_update(
                conn,
                job_id,
                "running",
                (index + 1) as i64,
                files.len() as i64,
                "正在扫描媒体文件",
                "",
            )?;
            progress_at = Instant::now();
        }
    }
    // 文件系统标记与数据库不能原子提交；先完成只读检查，避免逐级 stat 占用写锁。
    // 各播放／探测／识别入口仍会动态检查标记，计划后新增的标记也不能绕过门禁。
    let finalized = plan_finalization(conn, source_id, &seen, complete, ignored::contains)?;
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    require_enabled(&tx, source_id)?;
    let complete = finalized.complete;
    apply_finalization(&tx, source_id, &finalized)?;
    tx.execute(
        "UPDATE sources SET available=1,last_scan=?2,error=?3 WHERE id=?1",
        params![
            source_id,
            store::now(),
            if finalized.marker_unreadable {
                "部分目录忽略标记不可读，未清理未确认的缺失资源"
            } else if complete {
                ""
            } else {
                "部分目录不可读或达到扫描预算，未清理缺失资源"
            }
        ],
    )
    .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(
        json!({"sourceId":source_id,"files":included,"complete":complete,"changedResourceIds":changed,"newItemIds":new_items}),
    )
}

#[cfg(test)]
mod tests;
