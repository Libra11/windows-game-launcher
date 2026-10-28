use super::{store, State};
use rusqlite::params;
use serde_json::{json, Value};
use tauri::{Emitter, Manager};

pub fn settings(dir: &std::path::Path, conn: &rusqlite::Connection) -> Result<Value, String> {
    let mut data = json!({
        "playerPath":"",
        "playerKind":"potplayer",
        "potplayerPollIntervalMs":super::playback::DEFAULT_POTPLAYER_POLL_INTERVAL_MS,
        "ffprobePath":"",
        "ffmpegPath":"",
        "autoScanIntervalMinutes":30,
        "doubanAutoUpdate":false,
        "doubanUpdateIntervalMinutes":360,
    });
    let mut stmt = conn
        .prepare("SELECT key,value FROM settings")
        .map_err(|e| e.to_string())?;
    for entry in stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .map_err(|e| e.to_string())?
    {
        let (k, v) = entry.map_err(|e| e.to_string())?;
        if matches!(
            k.as_str(),
            "playerPath"
                | "playerKind"
                | "potplayerPollIntervalMs"
                | "ffprobePath"
                | "ffmpegPath"
                | "autoScanIntervalMinutes"
                | "doubanAutoUpdate"
                | "doubanUpdateIntervalMinutes"
        ) {
            data[&k] = serde_json::from_str(&v).unwrap_or(Value::Null);
        }
    }
    data["hasApiKey"] = json!(store::credential(dir, "tmdbApiKey")
        .map(|v| !v.is_empty())
        .unwrap_or(false));
    data["hasReadAccessToken"] = json!(store::credential(dir, "tmdbReadAccessToken")
        .map(|v| !v.is_empty())
        .unwrap_or(false));
    Ok(data)
}
pub fn organization_snapshot(conn: &rusqlite::Connection) -> Result<Value, String> {
    let mut stmt = conn
        .prepare("SELECT id,name,sort_order FROM media_collections ORDER BY sort_order,name")
        .map_err(|e| e.to_string())?;
    let mut collections = stmt
        .query_map([], |r| {
            Ok(json!({
                "id":r.get::<_, String>(0)?,
                "name":r.get::<_, String>(1)?,
                "sortOrder":r.get::<_, i64>(2)?,
                "itemIds":[],
            }))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    for entry in &mut collections {
        let mut links = conn
            .prepare("SELECT item_id FROM collection_items WHERE collection_id=?1")
            .map_err(|e| e.to_string())?;
        entry["itemIds"] = json!(links
            .query_map([store::text(entry, "id")], |r| r.get::<_, String>(0))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?);
    }
    let mut stmt = conn
        .prepare("SELECT id,name FROM media_tags ORDER BY name")
        .map_err(|e| e.to_string())?;
    let tags = stmt
        .query_map([], |r| {
            Ok(json!({"id":r.get::<_,String>(0)?,"name":r.get::<_,String>(1)?}))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(json!({"collections":collections,"tags":tags}))
}
pub fn source_list(conn: &rusqlite::Connection) -> Result<Vec<Value>, String> {
    let mut stmt=conn.prepare("SELECT id,path,label,kind,smart_folders,available,last_scan,error,created_at FROM sources s WHERE NOT EXISTS(SELECT 1 FROM settings x WHERE x.key='source.disabled.'||s.id AND x.value='true') ORDER BY created_at").map_err(|e|e.to_string())?;

    let values = stmt
        .query_map([], |r| {
            Ok(json!({
                "id":r.get::<_, String>(0)?,
                "path":r.get::<_, String>(1)?,
                "label":r.get::<_, String>(2)?,
                "kind":r.get::<_, String>(3)?,
                "smartFolders":r.get::<_, bool>(4)?,
                "available":r.get::<_, bool>(5)?,
                "lastScan":r.get::<_, String>(6)?,
                "error":r.get::<_, String>(7)?,
                "createdAt":r.get::<_, String>(8)?,
            }))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    values
        .into_iter()
        .map(|mut source| {
            source["ignoredCount"] =
                json!(super::ignored::rules(conn, store::text(&source, "id"))?.len());
            Ok(source)
        })
        .collect()
}
fn job_list(conn: &rusqlite::Connection) -> Result<Vec<Value>, String> {
    let mut statement = conn
        .prepare(
            "SELECT id,kind,state,completed,total,message,error,created_at,updated_at
         FROM jobs ORDER BY created_at DESC LIMIT 30",
        )
        .map_err(|e| e.to_string())?;
    let jobs = statement
        .query_map([], |row| {
            Ok(json!({
                "id": row.get::<_, String>(0)?,
                "kind": row.get::<_, String>(1)?,
                "state": row.get::<_, String>(2)?,
                "completed": row.get::<_, i64>(3)?,
                "total": row.get::<_, i64>(4)?,
                "message": row.get::<_, String>(5)?,
                "error": row.get::<_, String>(6)?,
                "createdAt": row.get::<_, String>(7)?,
                "updatedAt": row.get::<_, String>(8)?,
            }))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(jobs)
}

fn settings_snapshot_data(dir: &std::path::Path, conn: &rusqlite::Connection) -> Result<Value, String> {
    Ok(json!({
        "sources": source_list(conn)?,
        "jobs": job_list(conn)?,
        "settings": settings(dir, conn)?,
    }))
}

pub async fn settings_snapshot(app: tauri::AppHandle, _input: Value) -> Result<Value, String> {
    let dir = app.state::<State>().dir.clone();
    tauri::async_runtime::spawn_blocking(move || {
        settings_snapshot_data(&dir, &store::open(&dir)?)
    })
    .await
    .map_err(|_| "读取影视设置失败")?
}

pub async fn snapshot(app: tauri::AppHandle, input: Value) -> Result<Value, String> {
    let dir = app.state::<State>().dir.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let conn = store::open(&dir)?;
        let organization = organization_snapshot(&conn)?;
        let include_people = input["includePeople"].as_bool().unwrap_or(true);
        let people = if include_people { Some(super::people::snapshot(&conn)?) } else { None };
        let index = match &people {
            Some(value) => value["peopleIndex"].clone(),
            None => super::people::index_status(&conn)?,
        };
        let mut result = json!({
            "items": store::items(&conn)?,
            "sources": source_list(&conn)?,
            "collections": organization["collections"],
            "tags": organization["tags"],
            "jobs": job_list(&conn)?,
            "settings": settings(&dir, &conn)?,
            "peopleIndex": index,
        });
        if let Some(people) = people { result["people"] = people["people"].clone(); }
        Ok(result)
    })
    .await
    .map_err(|_| "读取影视库任务失败")?
}
pub async fn people_snapshot(app: tauri::AppHandle, _input: Value) -> Result<Value, String> {
    let dir = app.state::<State>().dir.clone();
    tauri::async_runtime::spawn_blocking(move || super::people::snapshot(&store::open(&dir)?))
        .await
        .map_err(|_| "读取演职员资料失败")?
}
pub async fn detail(app: tauri::AppHandle, input: Value) -> Result<Value, String> {
    let dir = app.state::<State>().dir.clone();
    tauri::async_runtime::spawn_blocking(move || {
        store::detail(&store::open(&dir)?, store::text(&input, "itemId"))
    })
    .await
    .map_err(|_| "读取影视详情失败")?
}
pub async fn create_item(app: tauri::AppHandle, input: Value) -> Result<Value, String> {
    let dir = app.state::<State>().dir.clone();
    let value = super::metadata::create_item(&dir, &input["candidate"]).await?;
    let _ = app.emit("media-changed", ());
    Ok(value)
}
pub async fn set_record(app: tauri::AppHandle, input: Value) -> Result<Value, String> {
    let dir = app.state::<State>().dir.clone();
    let value = tauri::async_runtime::spawn_blocking(move || {
        let mut conn = store::open(&dir)?;
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        let id = store::text(&input, "itemId");
        store::update_record(&tx, id, &input["patch"], true)?;
        tx.commit().map_err(|e| e.to_string())?;
        store::item(&conn, id)
    })
    .await
    .map_err(|_| "保存观影记录失败")??;
    let _ = app.emit("media-changed", ());
    Ok(value)
}
pub async fn organization(app: tauri::AppHandle, input: Value) -> Result<Value, String> {
    let dir = app.state::<State>().dir.clone();
    let value = tauri::async_runtime::spawn_blocking(move || {
        let mut conn = store::open(&dir)?;
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        apply_organization(&tx, &input)?;
        tx.commit().map_err(|e| e.to_string())?;
        organization_snapshot(&conn)
    })
    .await
    .map_err(|_| "保存影视整理失败")??;
    let _ = app.emit("media-changed", ());
    Ok(value)
}

fn organization_name(input: &Value) -> Result<&str, String> {
    let name = store::text(input, "name").trim();
    if name.is_empty() || name.chars().count() > 80 {
        return Err("名称须为 1 至 80 个字符".into());
    }
    Ok(name)
}

fn apply_organization(conn: &rusqlite::Connection, input: &Value) -> Result<(), String> {
    let action = store::text(input, "action");
    let id = input["collectionId"]
        .as_str()
        .or(input["id"].as_str())
        .unwrap_or("");
    match action {
        "createCollection" | "createTag" => {
            let name = organization_name(input)?;
            let new_id = uuid::Uuid::new_v4().to_string();
            if action == "createCollection" {
                conn.execute(
                    "INSERT INTO media_collections(id,name,sort_order)
                     VALUES(?1,?2,(SELECT COALESCE(MAX(sort_order),0)+1 FROM media_collections))",
                    params![new_id, name],
                )
                .map_err(|e| e.to_string())?;
            } else {
                conn.execute(
                    "INSERT INTO media_tags(id,name)VALUES(?1,?2)",
                    params![new_id, name],
                )
                .map_err(|e| e.to_string())?;
            }
        }
        "renameCollection" => {
            conn.execute(
                "UPDATE media_collections SET name=?2 WHERE id=?1",
                params![id, organization_name(input)?],
            )
            .map_err(|e| e.to_string())?;
        }
        "deleteCollection" => {
            conn.execute("DELETE FROM media_collections WHERE id=?1", [id])
                .map_err(|e| e.to_string())?;
        }
        "deleteTag" => {
            conn.execute("DELETE FROM media_tags WHERE id=?1", [id])
                .map_err(|e| e.to_string())?;
        }
        "setCollectionItems" => {
            let ids = input["itemIds"].as_array().ok_or("请选择影视条目")?;
            let mode = store::text(input, "mode");
            if !matches!(mode, "" | "add" | "remove") {
                return Err("收藏夹整理方式无效".into());
            }
            for item in ids {
                let item = item.as_str().ok_or("条目编号无效")?;
                let sql = if mode == "remove" {
                    "DELETE FROM collection_items WHERE collection_id=?1 AND item_id=?2"
                } else {
                    "INSERT OR IGNORE INTO collection_items(collection_id,item_id)VALUES(?1,?2)"
                };
                conn.execute(sql, params![id, item])
                    .map_err(|e| e.to_string())?;
            }
        }
        "setTags" => {
            let ids = input["itemIds"].as_array().ok_or("请选择影视条目")?;
            let tags = input["tagIds"].as_array().ok_or("标签格式无效")?;
            set_tags(conn, ids, tags, store::text(input, "mode"))?;
        }
        _ => return Err("不支持的影视整理操作".into()),
    }
    Ok(())
}
fn set_tags(
    conn: &rusqlite::Connection,
    items: &[Value],
    tags: &[Value],
    mode: &str,
) -> Result<(), String> {
    if !matches!(mode, "" | "replace" | "add" | "remove") {
        return Err("标签整理方式无效".into());
    }
    for item in items {
        let item = item.as_str().ok_or("条目编号无效")?;
        if matches!(mode, "" | "replace") {
            conn.execute("DELETE FROM media_item_tags WHERE item_id=?1", [item])
                .map_err(|e| e.to_string())?;
        }
        for tag in tags {
            let tag = tag.as_str().ok_or("标签编号无效")?;
            if mode == "remove" {
                conn.execute(
                    "DELETE FROM media_item_tags WHERE item_id=?1 AND tag_id=?2",
                    params![item, tag],
                )
                .map_err(|e| e.to_string())?;
            } else {
                conn.execute(
                    "INSERT OR IGNORE INTO media_item_tags(item_id,tag_id)VALUES(?1,?2)",
                    params![item, tag],
                )
                .map_err(|e| e.to_string())?;
            }
        }
    }
    Ok(())
}
pub async fn update_settings(app: tauri::AppHandle, input: Value) -> Result<Value, String> {
    let dir = app.state::<State>().dir.clone();
    let values = input["settings"]
        .as_object()
        .ok_or("影视设置格式无效")?
        .clone();
    let value = tauri::async_runtime::spawn_blocking(move || {
        let mut conn = store::open(&dir)?;
        for name in ["tmdbApiKey", "tmdbReadAccessToken"] {
            if let Some(value) = values.get(name).and_then(Value::as_str) {
                if value.len() > 8192 {
                    return Err("凭据长度无效".into());
                }
                store::save_credential(&dir, name, value)?;
            }
        }
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        for name in [
            "playerPath",
            "playerKind",
            "potplayerPollIntervalMs",
            "ffprobePath",
            "ffmpegPath",
            "autoScanIntervalMinutes",
            "doubanAutoUpdate",
            "doubanUpdateIntervalMinutes",
        ] {
            if let Some(value) = values.get(name) {
                if name == "potplayerPollIntervalMs" {
                    super::playback::potplayer_poll_interval_ms(Some(value))?;
                }
                if name.ends_with("Minutes") && value.as_u64().is_none_or(|v| v > 10080) {
                    return Err("更新间隔须为 0 至 10080 分钟".into());
                }
                if name == "doubanAutoUpdate" && !value.is_boolean() {
                    return Err("自动更新选项无效".into());
                }
                if name.ends_with("Path") && value.as_str().is_none() {
                    return Err("程序路径格式无效".into());
                }
                if name == "playerKind" && !matches!(value.as_str(), Some("potplayer" | "mpv")) {
                    return Err("播放器类型无效".into());
                }
                store::set_setting(&tx, name, value)?;
            }
        }
        tx.commit().map_err(|e| e.to_string())?;
        settings(&dir, &conn)
    })
    .await
    .map_err(|_| "保存影视设置失败")??;
    let _ = app.emit("media-changed", ());
    Ok(value)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn settings_snapshot_only_returns_settings_sources_and_jobs() {
        let dir = tempfile::tempdir().unwrap();
        let conn = store::open(dir.path()).unwrap();
        store::set_setting(&conn, "playerPath", &json!("C:/player.exe")).unwrap();
        conn.execute("INSERT INTO sources(id,path,label,created_at) VALUES('s','C:/media','测试','')", []).unwrap();
        store::job_start(&conn, "scan", 3).unwrap();
        store::new_item(&conn, &json!({"title":"不属于设置快照"})).unwrap();
        let result = settings_snapshot_data(dir.path(), &conn).unwrap();
        assert_eq!(result["settings"]["playerPath"], "C:/player.exe");
        assert_eq!(result["sources"][0]["id"], "s");
        assert_eq!(result["jobs"][0]["kind"], "scan");
        assert!(result.get("items").is_none());
        assert!(result.get("people").is_none());
        assert_eq!(result.as_object().unwrap().len(), 3);
    }
    #[test]
    fn media_tool_paths_are_exposed_independently_with_empty_automatic_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let conn = store::open(dir.path()).unwrap();
        for key in ["ffmpegPath", "ffprobePath"] {
            assert_eq!(settings(dir.path(), &conn).unwrap()[key], "");
            store::set_setting(&conn, key, &json!(format!("C:/Tools/{key}.exe"))).unwrap();
        }
        let value = settings(dir.path(), &conn).unwrap();
        assert_eq!(value["ffmpegPath"], "C:/Tools/ffmpegPath.exe");
        assert_eq!(value["ffprobePath"], "C:/Tools/ffprobePath.exe");
    }
    #[test]
    fn potplayer_poll_interval_is_in_settings_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        let conn = store::open(dir.path()).unwrap();
        assert_eq!(
            settings(dir.path(), &conn).unwrap()["potplayerPollIntervalMs"],
            500
        );
        store::set_setting(&conn, "potplayerPollIntervalMs", &json!(100)).unwrap();
        assert_eq!(
            settings(dir.path(), &conn).unwrap()["potplayerPollIntervalMs"],
            100
        );
    }
    #[test]
    fn organization_validates_names_and_collection_modes_consistently() {
        let dir = tempfile::tempdir().unwrap();
        let conn = store::open(dir.path()).unwrap();
        apply_organization(&conn, &json!({"action":"createCollection","name":"电影"})).unwrap();
        let id = organization_snapshot(&conn).unwrap()["collections"][0]["id"].clone();
        for action in ["createCollection", "createTag", "renameCollection"] {
            assert!(apply_organization(
                &conn,
                &json!({"action":action,"id":id,"name":"长".repeat(81)})
            )
            .is_err());
        }
        assert!(apply_organization(
            &conn,
            &json!({"action":"setCollectionItems","collectionId":id,"mode":"invalid","itemIds":[]})
        )
        .is_err());
        assert_eq!(
            organization_snapshot(&conn).unwrap()["collections"][0]["name"],
            "电影"
        );
    }
    #[test]
    fn adding_tags_preserves_unrelated_tags() {
        let dir = tempfile::tempdir().unwrap();
        let conn = store::open(dir.path()).unwrap();
        let id = store::new_item(&conn, &json!({"title":"片名"})).unwrap();
        conn.execute(
            "INSERT INTO media_tags(id,name)VALUES('a','原标签'),('b','新标签')",
            [],
        )
        .unwrap();
        set_tags(&conn, &[json!(&id)], &[json!("a")], "replace").unwrap();
        set_tags(&conn, &[json!(&id)], &[json!("b")], "add").unwrap();
        assert_eq!(
            store::item(&conn, &id).unwrap()["tags"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        set_tags(&conn, &[json!(&id)], &[json!("b")], "remove").unwrap();
        assert_eq!(store::item(&conn, &id).unwrap()["tags"][0]["id"], "a");
    }
    #[test]
    fn manual_favorite_preserves_imported_record_origin() {
        let dir = tempfile::tempdir().unwrap();
        let conn = store::open(dir.path()).unwrap();
        let id = store::upsert_provider_item(
            &conn,
            "douban",
            "subject",
            "123",
            &json!({"title":"来源电影"}),
            true,
        )
        .unwrap();
        store::update_record(
            &conn,
            &id,
            &json!({"watched":true,"rating":4,"recordSource":"douban"}),
            false,
        )
        .unwrap();
        store::update_record(&conn, &id, &json!({"favorite":true}), true).unwrap();
        let item = store::item(&conn, &id).unwrap();
        assert_eq!(item["recordSource"], "douban");
        assert_eq!(item["rating"], 4.0);
        assert_eq!(item["favorite"], true);
        store::update_record(
            &conn,
            &id,
            &json!({"favorite":false,"rating":3,"recordSource":"douban"}),
            false,
        )
        .unwrap();
        let item = store::item(&conn, &id).unwrap();
        assert_eq!(item["favorite"], true);
        assert_eq!(item["rating"], 3.0);
        let local = store::new_item(&conn, &json!({"title":"本地电影"})).unwrap();
        store::update_record(&conn, &local, &json!({"watched":true}), true).unwrap();
        assert_eq!(
            store::item(&conn, &local).unwrap()["recordSource"],
            "manual"
        );
    }
}
