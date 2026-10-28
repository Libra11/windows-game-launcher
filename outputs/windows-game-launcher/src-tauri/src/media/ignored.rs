//! 本机路径排除规则；不修改磁盘文件，不删除个人记录。
use super::{catalog, playback, scan, store, State};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;
use tauri::{Emitter, Manager};
mod marker;
#[cfg(test)]
mod tests;

pub(super) use marker::directory_marked;
pub(super) const MARKER_FILE: &str = ".youji-ignore";

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Rule {
    pub path: String,
    pub directory: bool,
}
pub(super) fn path_key(path: &str) -> String {
    let path = path.replace('\\', "/");
    let path = if path.to_ascii_lowercase().starts_with("//?/unc/") {
        format!("//{}", &path[8..])
    } else {
        path.strip_prefix("//?/").unwrap_or(&path).to_string()
    };
    path.trim_end_matches('/').to_lowercase()
}
impl Rule {
    pub fn matches(&self, path: &str) -> bool {
        let root = path_key(&self.path);
        let path = path_key(path);
        path == root || (self.directory && path.starts_with(&(root + "/")))
    }
}
pub(super) fn rules(conn: &Connection, source_id: &str) -> Result<Vec<Rule>, String> {
    let value = store::setting(conn, &format!("source.ignored.{source_id}"))?;
    if value.is_null() {
        return Ok(Vec::new());
    }
    serde_json::from_value(value).map_err(|_| "忽略路径规则无法读取".into())
}
pub(super) fn contains(conn: &Connection, source_id: &str, path: &str) -> Result<bool, String> {
    if rules(conn, source_id)?
        .iter()
        .any(|rule| rule.matches(path))
    {
        return Ok(true);
    }
    let root: String = conn
        .query_row("SELECT path FROM sources WHERE id=?1", [source_id], |row| {
            row.get(0)
        })
        .map_err(|_| "无法读取媒体目录的忽略标记")?;
    directory_marked(Path::new(&root), Path::new(path))
}
pub(super) fn metadata_allowed(conn: &Connection, id: &str) -> Result<bool, String> {
    let independent: Option<bool> = conn
        .query_row(
            "SELECT independent FROM media_items WHERE id=?1",
            [id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    let Some(independent) = independent else {
        return Ok(false);
    };
    let mut stmt=conn.prepare("WITH RECURSIVE tree(id)AS(SELECT ?1 UNION SELECT m.id FROM media_items m JOIN tree t ON m.parent_id=t.id)SELECT DISTINCT r.source_id,r.path,r.available,r.missing FROM resources r JOIN resource_links l ON l.resource_id=r.id JOIN tree t ON t.id=l.item_id").map_err(|e|e.to_string())?;
    let resources = stmt
        .query_map([id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, bool>(2)?,
                row.get::<_, bool>(3)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    drop(stmt);
    if resources.is_empty() {
        return Ok(independent);
    }
    for (source, path, available, missing) in resources {
        if !available
            || missing
            || store::setting(conn, &format!("source.disabled.{source}"))? == true
        {
            continue;
        }
        if !contains(conn, &source, &path)? {
            return Ok(true);
        }
    }
    Ok(false)
}
fn clean(conn: &Connection, source_id: &str, rule: &Rule) -> Result<Value, String> {
    let mut stmt = conn
        .prepare("SELECT id,path FROM resources WHERE source_id=?1")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([source_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| e.to_string())?;
    let mut ids = Vec::new();
    for row in rows {
        let (id, path) = row.map_err(|e| e.to_string())?;
        if rule.matches(&path) {
            ids.push(id);
        }
    }
    drop(stmt);
    super::removal::resources(conn, &ids)
}
pub(super) fn save(conn: &mut Connection, input: &Value) -> Result<Value, String> {
    let source_id = store::text(input, "sourceId");
    let source = catalog::source_list(conn)?
        .into_iter()
        .find(|v| v["id"] == source_id)
        .ok_or("媒体目录不存在")?;
    let removing = input["remove"] == true;
    let requested = store::text(input, "path");
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    let mut values = rules(&tx, source_id)?;
    let result = if removing {
        values.retain(|v| path_key(&v.path) != path_key(requested));
        json!({"ok":true})
    } else {
        let target = scan::normalized(Path::new(requested))?;
        let root = scan::normalized(Path::new(store::text(&source, "path")))?;
        let path = target.to_string_lossy().to_string();
        let boundary = Rule {
            path: root.to_string_lossy().to_string(),
            directory: true,
        };
        if path_key(&path) == path_key(&boundary.path) || !boundary.matches(&path) {
            return Err("请选择媒体目录内的文件或子目录".into());
        }
        let rule = Rule {
            path,
            directory: target.is_dir(),
        };
        if !rule.directory && !target.is_file() {
            return Err("路径不是文件或目录".into());
        }
        if !values.iter().any(|v| v.matches(&rule.path)) {
            values.push(rule.clone());
        }
        store::set_setting(&tx, &format!("source.ignored.{source_id}"), &json!(values))?;
        clean(&tx, source_id, &rule)?
    };
    store::set_setting(&tx, &format!("source.ignored.{source_id}"), &json!(values))?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(result)
}
pub async fn update(app: tauri::AppHandle, input: Value) -> Result<Value, String> {
    let operation = super::operation(&app).await?;
    let dir = app.state::<State>().dir.clone();
    if input["remove"] != true {
        let status = playback::status(app.clone(), json!({})).await?;
        if status["active"] == true {
            let conn = store::open(&dir)?;
            let rule = Rule {
                path: store::text(&input, "path").into(),
                directory: true,
            };
            if store::resource(&conn, store::text(&status, "resourceId"))
                .ok()
                .is_some_and(|r| rule.matches(store::text(&r, "path")))
            {
                return Err("该路径中的文件正在播放，请先停止播放再忽略".into());
            }
        }
    }
    let result = tauri::async_runtime::spawn_blocking(move || {
        let _operation = operation;
        save(&mut store::open(&dir)?, &input)
    })
    .await
    .map_err(|_| "更新忽略路径失败")??;
    let _ = app.emit("media-changed", ());
    Ok(result)
}
pub async fn list(app: tauri::AppHandle, input: Value) -> Result<Value, String> {
    let _operation = super::operation(&app).await?;
    let conn = store::open(&app.state::<State>().dir)?;
    let mut values = Vec::new();
    for source in catalog::source_list(&conn)? {
        let id = store::text(&source, "id");
        if input["sourceId"]
            .as_str()
            .is_some_and(|wanted| wanted != id)
        {
            continue;
        }
        let rules = rules(&conn, id)?;
        for rule in &rules {
            let covered = rules.iter().any(|other| {
                path_key(&other.path) != path_key(&rule.path) && other.matches(&rule.path)
            });
            values.push(json!({"sourceId":id,"sourceLabel":source["label"],"sourcePath":source["path"],"path":rule.path,"directory":rule.directory,"covered":covered}));
        }
    }
    Ok(json!(values))
}
