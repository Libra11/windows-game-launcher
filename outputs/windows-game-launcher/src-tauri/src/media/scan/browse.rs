use super::*;

pub(super) fn browse_directory(conn: &Connection, input: &Value) -> Result<Value, String> {
    let source = catalog::source_list(conn)?
        .into_iter()
        .find(|source| source["id"] == input["sourceId"])
        .ok_or("媒体目录不存在")?;
    let root = normalized(Path::new(store::text(&source, "path")))?;
    let requested = store::text(input, "path");
    let path = if requested.is_empty() {
        root.clone()
    } else {
        let requested = Path::new(requested);
        normalized(&if requested.is_absolute() {
            requested.to_path_buf()
        } else {
            root.join(requested)
        })?
    };
    if !path.starts_with(&root) || !path.is_dir() {
        return Err("浏览路径不在选定媒体目录内".into());
    }
    if ignored::contains(conn, store::text(&source, "id"), &path.to_string_lossy())? {
        return Err("该目录已忽略，请取消忽略规则或移除 .youji-ignore 标记后重试".into());
    }
    let ignored = ignored::rules(conn, store::text(&source, "id"))?;
    // 智能目录只需要当前来源的身份层级，不读取整库的资料、进度和资源聚合。
    let values = if source["smartFolders"] == true {
        hierarchy(conn, store::text(&source, "id"))?
    } else {
        Vec::new()
    };
    let descendants = if source["smartFolders"] == true {
        directory_links(conn, store::text(&source, "id"), &path)?
    } else {
        HashMap::new()
    };
    let by_id: HashMap<&str, &Value> = values
        .iter()
        .map(|value| (store::text(value, "id"), value))
        .collect();
    let mut entries = Vec::new();
    for entry in fs::read_dir(&path).map_err(|_| "媒体目录暂时不可读")? {
        let entry = entry.map_err(|_| "目录项读取失败")?;
        let path = entry.path();
        if ignored
            .iter()
            .any(|rule| rule.matches(&path.to_string_lossy()))
        {
            continue;
        }
        let metadata = match fs::symlink_metadata(&path) {
            Ok(value) => value,
            Err(_) => continue,
        };
        if reparse(&metadata) {
            continue;
        }
        match ignored::directory_marked(&root, &path) {
            Ok(false) => {}
            Ok(true) | Err(_) => continue,
        }
        let name = entry.file_name().to_string_lossy().to_string();
        let path_text = path.to_string_lossy().to_string();
        if metadata.is_dir() {
            let item = if source["smartFolders"] == true {
                smart_item(
                    &path,
                    descendants.get(&path).map(Vec::as_slice).unwrap_or(&[]),
                    &by_id,
                )?
            } else {
                None
            };
            entries.push(json!({
                "id": path_text,
                "kind": if item.is_some() { "item" } else { "folder" },
                "name": item.as_ref().map(|value|store::text(value,"title")).filter(|value|!value.is_empty()).unwrap_or(&name),
                "path": path_text,
                "directory": true,
                "item": item,
            }));
        } else if metadata.is_file() {
            if !crate::media::file_filter::scan_candidate(&path).unwrap_or(false) {
                continue;
            }
            let resource_id: Option<String> = conn
                .query_row(
                    "SELECT id FROM resources WHERE path=?1 COLLATE NOCASE",
                    [&path_text],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?;
            if let Some(id) = resource_id {
                let resource = store::resource(conn, &id)?;
                let item = resource["itemIds"]
                    .as_array()
                    .and_then(|ids| ids.first())
                    .and_then(Value::as_str)
                    .map(|id| json!({"id":id}));
                entries.push(json!({
                    "id": id,
                    "kind": "file",
                    "name": name,
                    "path": path_text,
                    "directory": false,
                    "item": item,
                    "resource": resource,
                }));
            }
        }
    }
    let ids: Vec<String> = entries
        .iter()
        .filter_map(|entry| entry["item"]["id"].as_str().map(str::to_owned))
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    let items: HashMap<String, Value> = store::items_by_ids(conn, &ids)?
        .into_iter()
        .map(|item| (store::text(&item, "id").to_string(), item))
        .collect();
    for entry in &mut entries {
        if let Some(item) = entry["item"]["id"].as_str().and_then(|id| items.get(id)) {
            if entry["directory"] == true || item["recognized"] == true {
                entry["name"] = item["title"].clone();
            }
            if entry["directory"] == false {
                entry["kind"] = json!(if item["recognized"] == true {
                    "item"
                } else {
                    "file"
                });
            }
            entry["item"] = item.clone();
        }
    }
    entries.sort_by(|left, right| {
        right["directory"]
            .as_bool()
            .cmp(&left["directory"].as_bool())
            .then_with(|| store::text(left, "name").cmp(store::text(right, "name")))
    });
    let parent = if path == root {
        None
    } else {
        path.parent()
            .map(|value| value.to_string_lossy().to_string())
    };
    Ok(json!({"source":source,"path":path.to_string_lossy(),"parentPath":parent,"entries":entries}))
}

#[cfg(test)]
mod tests;
type LinkedFile = (String, Option<String>);

// 一次读取来源资源，按当前目录的直接子目录分组；每个智能目录只检查自己的资源。
fn directory_links(
    conn: &Connection,
    source_id: &str,
    parent: &Path,
) -> Result<HashMap<PathBuf, Vec<LinkedFile>>, String> {
    let mut statement = conn.prepare("SELECT r.path,l.item_id FROM resources r LEFT JOIN resource_links l ON l.resource_id=r.id WHERE r.source_id=?1 AND r.missing=0 ORDER BY r.path,l.item_id").map_err(|e| e.to_string())?;
    let rows = statement
        .query_map([source_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
        })
        .map_err(|e| e.to_string())?;
    let mut result = HashMap::<PathBuf, Vec<LinkedFile>>::new();
    for row in rows {
        let (file, id) = row.map_err(|e| e.to_string())?;
        let file_path = Path::new(&file);
        let Ok(relative) = file_path.strip_prefix(parent) else {
            continue;
        };
        let Some(first) = relative.components().next() else {
            continue;
        };
        let directory = parent.join(first);
        if directory != file_path {
            result.entry(directory).or_default().push((file, id));
        }
    }
    Ok(result)
}
fn hierarchy(conn: &Connection, source_id: &str) -> Result<Vec<Value>, String> {
    let mut statement = conn.prepare("WITH RECURSIVE scope(id)AS(SELECT l.item_id FROM resource_links l JOIN resources r ON r.id=l.resource_id WHERE r.source_id=?1 AND r.missing=0 UNION SELECT m.parent_id FROM media_items m JOIN scope s ON s.id=m.id WHERE m.parent_id IS NOT NULL) SELECT m.id,m.parent_id,m.kind,m.title,m.recognized,m.season_number FROM media_items m JOIN scope s ON s.id=m.id").map_err(|e| e.to_string())?;
    let rows = statement.query_map([source_id], |r| Ok(json!({"id":r.get::<_,String>(0)?,"parentId":r.get::<_,Option<String>>(1)?,"kind":r.get::<_,String>(2)?,"title":r.get::<_,String>(3)?,"recognized":r.get::<_,bool>(4)?,"seasonNumber":r.get::<_,Option<i64>>(5)?}))).map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}
#[cfg(test)]
pub(super) fn smart_directory(
    conn: &Connection,
    source_id: &str,
    path: &Path,
    by_id: &HashMap<&str, &Value>,
) -> Result<Option<Value>, String> {
    let links = directory_links(conn, source_id, path.parent().unwrap())?;
    smart_item(
        path,
        links.get(path).map(Vec::as_slice).unwrap_or(&[]),
        by_id,
    )
}
fn smart_item(
    path: &Path,
    rows: &[LinkedFile],
    by_id: &HashMap<&str, &Value>,
) -> Result<Option<Value>, String> {
    let mut chains: Vec<Vec<String>> = Vec::new();
    let mut first_file = None;
    for row in rows {
        let (file, id) = row;
        if first_file.is_none() {
            first_file = Some(file);
        }
        let Some(id) = id else { return Ok(None) };
        let Some(value) = by_id.get(id.as_str()) else {
            return Ok(None);
        };
        if value["recognized"] != true {
            return Ok(None);
        }
        let mut chain = vec![id.clone()];
        let mut value = *value;
        let mut seen = HashSet::new();
        while let Some(parent) = value["parentId"].as_str() {
            if !seen.insert(parent) {
                return Ok(None);
            }
            let Some(next) = by_id.get(parent) else {
                break;
            };
            chain.push(parent.to_string());
            value = next;
        }
        chains.push(chain);
    }
    let Some(first) = chains.first() else {
        return Ok(None);
    };
    let folder = model::parse_name(
        &path
            .file_name()
            .map(|name| name.to_string_lossy())
            .unwrap_or_default(),
    );
    let folder_name = model::normalize_title(&folder.title);
    let file_name = first_file
        .map(|file| model::parse_path(Path::new(file)))
        .map(|value| model::normalize_title(&value.title))
        .unwrap_or_default();
    for id in first {
        if !chains.iter().all(|chain| chain.contains(id)) {
            continue;
        }
        let Some(value) = by_id.get(id.as_str()) else {
            continue;
        };
        let same_name = folder_name == model::normalize_title(store::text(value, "title"))
            || folder_name == file_name;
        let eligible = match value["kind"].as_str() {
            Some("season") => folder
                .season
                .map(|number| Some(number as u64) == value["seasonNumber"].as_u64())
                .unwrap_or(false),
            Some("movie" | "series") => same_name,
            _ => false,
        };
        if eligible {
            return Ok(Some((*value).clone()));
        }
    }
    Ok(None)
}
