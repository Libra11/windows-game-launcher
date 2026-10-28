use super::*;
use crate::media::{catalog, ignored, scan};
use std::fs;

pub(super) fn collect(conn: &Connection, input: &Value) -> Result<Vec<Value>, String> {
    let source_id = store::text(input, "sourceId");
    let source = catalog::source_list(conn)?
        .into_iter()
        .find(|source| source["id"] == source_id)
        .ok_or("媒体目录不存在或已停用")?;
    let root = scan::normalized(Path::new(store::text(&source, "path")))?;
    let requested = store::text(input, "path");
    let path = if requested.is_empty() {
        root.clone()
    } else {
        scan::normalized(&root.join(requested))?
    };
    if !path.starts_with(&root) || !path.is_dir() {
        return Err("请选择媒体目录内的单季文件夹".into());
    }
    if ignored::contains(conn, source_id, &path.to_string_lossy())? {
        return Err("该目录已忽略，请先取消忽略".into());
    }
    let mut statement = conn.prepare("SELECT id,path,size,modified_at FROM resources WHERE source_id=?1 AND missing=0 AND available=1 ORDER BY path COLLATE NOCASE,id").map_err(|e|e.to_string())?;
    let rows = statement
        .query_map([source_id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, String>(3)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    let mut result = Vec::new();
    let boundary = ignored::Rule {
        path: path.to_string_lossy().into_owned(),
        directory: true,
    };
    let directory_key = ignored::path_key(&boundary.path);
    for row in rows {
        let (id, file, size, modified) = row.map_err(|e| e.to_string())?;
        let file_path = Path::new(&file);
        if !boundary.matches(&file)
            || (input["recursive"] != true
                && file_path.parent().is_none_or(|parent| {
                    ignored::path_key(&parent.to_string_lossy()) != directory_key
                }))
        {
            continue;
        }
        if ignored::contains(conn, source_id, &file)? {
            continue;
        }
        let mut error = String::new();
        match fs::symlink_metadata(file_path) {
            Ok(meta) if meta.is_file() && !scan::reparse(&meta) => {
                if meta.len() as i64 != size || scan::stamp(&meta) != modified {
                    error = "文件已变化，请重新扫描后预览".into();
                }
                if !scan::normalized(file_path).is_ok_and(|file| file.starts_with(&path)) {
                    error = "文件实际位置不在所选目录内".into();
                }
            }
            _ => {
                error = "文件离线或不是普通文件".into();
            }
        }
        let mut links = conn.prepare("SELECT l.item_id,l.method,l.locked,m.kind,m.title,m.parent_id,m.updated_at,p.title,g.title FROM resource_links l JOIN media_items m ON m.id=l.item_id LEFT JOIN media_items p ON p.id=m.parent_id LEFT JOIN media_items g ON g.id=p.parent_id WHERE l.resource_id=?1 ORDER BY l.item_id").map_err(|e|e.to_string())?;
        let links = links.query_map([&id],|r|Ok(json!({"itemId":r.get::<_,String>(0)?,"method":r.get::<_,String>(1)?,"locked":r.get::<_,bool>(2)?,"kind":r.get::<_,String>(3)?,"title":r.get::<_,String>(4)?,"parentId":r.get::<_,Option<String>>(5)?,"updatedAt":r.get::<_,String>(6)?,"parentTitle":r.get::<_,Option<String>>(7)?,"seriesTitle":r.get::<_,Option<String>>(8)?}))).map_err(|e|e.to_string())?.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?;
        let technical: String = conn
            .query_row(
                "SELECT technical_json FROM resources WHERE id=?1",
                [&id],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        let resource = json!({"path":file,"technical":serde_json::from_str::<Value>(&technical).unwrap_or_default()});
        let warning = if crate::media::file_filter::extra(file_path) {
            Some("样片、预告或附加片，请手动确认".to_string())
        } else if matches!(
            file_path
                .extension()
                .and_then(|v| v.to_str())
                .unwrap_or("")
                .to_ascii_lowercase()
                .as_str(),
            "iso" | "img" | "yuv"
        ) {
            Some("镜像或原始视频请手动确认对应分集".to_string())
        } else if resource["technical"]["state"] == "ready"
            && resource["technical"]["video"]
                .as_array()
                .is_some_and(Vec::is_empty)
        {
            Some("未发现视频流，请手动确认".to_string())
        } else {
            None
        };
        result.push(json!({"id":id,"path":file,"name":file_path.file_name().unwrap_or_default().to_string_lossy(),"size":size,"modifiedAt":modified,"links":links,"error":error,"suggestionWarning":warning}));
    }
    Ok(result)
}
