#[cfg(test)]
#[path = "scan/removal_tests.rs"]
mod removal_tests;
mod write;
mod browse;
#[cfg(windows)]
mod watch;
use browse::browse_directory;
#[cfg(test)]
use browse::smart_directory;
use super::{catalog, ignored, model, playback, probe, removal, store, State};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::Duration,
};
use tauri::{Emitter, Manager};

static RUNNING: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
const SOURCE_DISABLED: &str = "媒体目录已停用，扫描已取消";
const SCAN_RUNNING: &str = "该媒体目录正在扫描";

fn require_enabled(conn: &Connection, source_id: &str) -> Result<(), String> {
    let enabled: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sources s WHERE s.id=?1 AND NOT EXISTS(SELECT 1 FROM settings x WHERE x.key='source.disabled.'||s.id AND x.value='true'))",
        [source_id], |row| row.get(0),
    ).map_err(|e| e.to_string())?;
    if enabled {
        Ok(())
    } else {
        Err(SOURCE_DISABLED.into())
    }
}

fn disable_source(conn: &mut Connection, id: &str) -> Result<(), String> {
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    let source = tx
        .execute(
            "UPDATE sources SET available=0,error='已停用媒体目录' WHERE id=?1",
            [id],
        )
        .map_err(|e| e.to_string())?;
    if source == 0 {
        return Err("媒体目录不存在".into());
    }
    store::set_setting(&tx, &format!("source.disabled.{id}"), &json!(true))?;
    let resources = {
        let mut stmt = tx
            .prepare("SELECT id FROM resources WHERE source_id=?1")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([id], |row| row.get::<_, String>(0))
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?
    };
    removal::resources(&tx, &resources)?;
    tx.commit().map_err(|e| e.to_string())
}
pub(super) fn reparse(meta: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        meta.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        meta.file_type().is_symlink()
    }
}
pub(super) fn normalized(path: &Path) -> Result<PathBuf, String> {
    let original = fs::symlink_metadata(path).map_err(|_| "媒体目录当前无法访问")?;
    if reparse(&original) {
        return Err("不扫描重解析点或链接目录，请直接选择实际媒体目录".into());
    }
    let path = fs::canonicalize(path).map_err(|_| "媒体目录当前无法访问")?;
    let meta = fs::symlink_metadata(&path).map_err(|_| "无法读取媒体目录")?;
    if reparse(&meta) {
        return Err("不扫描重解析点或链接目录，请直接选择实际媒体目录".into());
    }
    Ok(path)
}
pub(super) fn stamp(meta: &fs::Metadata) -> String {
    meta.modified()
        .ok()
        .map(|time| {
            chrono::DateTime::<chrono::Utc>::from(time)
                .to_rfc3339_opts(chrono::SecondsFormat::Nanos, true)
        })
        .unwrap_or_default()
}
fn collect(
    root: &Path,
    conn: &Connection,
    source_id: &str,
) -> Result<(Vec<(PathBuf, fs::Metadata)>, bool), String> {
    if fs::symlink_metadata(root)
        .map(|meta| reparse(&meta))
        .unwrap_or(true)
    {
        return Err("媒体目录当前不可读或为重解析点".into());
    }
    if ignored::directory_marked(root, root)? {
        return Ok((Vec::new(), true));
    }
    fs::read_dir(root).map_err(|_| "媒体目录无法读取")?;
    let mut stack = vec![(root.to_path_buf(), 0)];
    let mut files = Vec::new();
    let mut complete = true;
    let mut entries = 0;
    while let Some((dir, depth)) = stack.pop() {
        match ignored::directory_marked(root, &dir) {
            Ok(true) => continue,
            Ok(false) => {}
            Err(_) => {
                complete = false;
                continue;
            }
        }
        // 每进入一个目录重新读取规则，忽略操作可取消尚未枚举的子树。
        let ignored = ignored::rules(conn, source_id)?;
        if ignored
            .iter()
            .any(|rule| rule.matches(&dir.to_string_lossy()))
        {
            continue;
        }
        let list = match fs::read_dir(&dir) {
            Ok(list) => list,
            Err(_) => {
                complete = false;
                continue;
            }
        };
        for entry in list {
            entries += 1;
            if entries > 100_000 {
                complete = false;
                break;
            }
            let entry = match entry {
                Ok(e) => e,
                Err(_) => {
                    complete = false;
                    continue;
                }
            };
            let path = entry.path();
            if ignored
                .iter()
                .any(|rule| rule.matches(&path.to_string_lossy()))
            {
                continue;
            }
            let meta = match fs::symlink_metadata(&path) {
                Ok(meta) => meta,
                Err(_) => {
                    complete = false;
                    continue;
                }
            };
            if reparse(&meta) {
                continue;
            }
            if meta.is_dir() {
                if depth < 32 {
                    stack.push((path, depth + 1));
                } else {
                    complete = false;
                }
            } else if meta.is_file() {
                match super::file_filter::scan_candidate(&path) {
                    Ok(true) => files.push((path, meta)),
                    Ok(false) => {}
                    Err(_) => complete = false,
                }
            }
        }
        if entries > 100_000 {
            break;
        }
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    Ok((files, complete))
}
pub fn scan_source(dir: &Path, source_id: &str, job_id: &str) -> Result<Value, String> {
    let mut conn = store::open(dir)?;
    require_enabled(&conn, source_id)?;
    let root: String = conn
        .query_row("SELECT path FROM sources WHERE id=?1", [source_id], |r| {
            r.get(0)
        })
        .map_err(|_| "媒体目录不存在")?;
    let (files, complete) = match collect(Path::new(&root), &conn, source_id) {
        Ok(value) => value,
        Err(error) => {
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(|e| e.to_string())?;
            require_enabled(&tx, source_id)?;
            tx.execute(
                "UPDATE sources SET available=0,error=?2 WHERE id=?1 AND (available<>0 OR error<>?2)",
                params![source_id, error],
            )
            .map_err(|e| e.to_string())?;
            tx.execute(
                "UPDATE resources SET available=0 WHERE source_id=?1 AND available<>0",
                [source_id],
            )
            .map_err(|e| e.to_string())?;
            tx.commit().map_err(|e| e.to_string())?;
            return Err(error);
        }
    };
    apply_scan(&mut conn, source_id, job_id, &files, complete)
}

fn apply_scan(
    conn: &mut Connection,
    source_id: &str,
    job_id: &str,
    files: &[(PathBuf, fs::Metadata)],
    complete: bool,
) -> Result<Value, String> {
    write::apply(conn, source_id, job_id, files, complete)
}
async fn start_scan(app: tauri::AppHandle, source_id: String) -> Result<Value, String> {
    let operation = super::operation(&app).await?;
    let dir = app.state::<State>().dir.clone();
    let key = format!("{}:{source_id}", dir.display());
    if !RUNNING
        .get_or_init(Default::default)
        .lock()
        .map_err(|_| "扫描状态不可用")?
        .insert(key.clone())
    {
        return Err(SCAN_RUNNING.into());
    }
    let job_id = match store::open(&dir).and_then(|conn| {
        require_enabled(&conn, &source_id)?;
        store::job_start(&conn, "scan", 0)
    }) {
        Ok(id) => id,
        Err(error) => {
            if let Ok(mut running) = RUNNING.get_or_init(Default::default).lock() {
                running.remove(&key);
            }
            return Err(error);
        }
    };
    let id = job_id.clone();
    let returned_source_id = source_id.clone();
    tauri::async_runtime::spawn(async move {
        let operation = operation;
        let scan_dir = dir.clone();
        let scan_id = source_id.clone();
        let scan_job = job_id.clone();
        let result = tauri::async_runtime::spawn_blocking(move || {
            scan_source(&scan_dir, &scan_id, &scan_job)
        })
        .await
        .map_err(|_| "媒体扫描线程失败".to_string())
        .and_then(|v| v);
        let new_items = result.as_ref().ok().map(|value| value["newItemIds"].clone())
            .unwrap_or_else(|| json!([]));
        match result {
            Ok(result) => {
                // 索引已提交，列表不必等待逐文件 ffprobe 完成。
                let _ = app.emit("media-changed", ());
                let resources = result["changedResourceIds"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                for (index, resource) in resources.iter().enumerate() {
                    if store::open(&dir)
                        .and_then(|conn| require_enabled(&conn, &source_id))
                        .is_err()
                    {
                        break;
                    }
                    let probe_dir = dir.clone();
                    let resource = resource.as_str().unwrap_or("").to_string();
                    let allowed = store::open(&dir)
                        .and_then(|conn| {
                            let value = store::resource(&conn, &resource)?;
                            Ok(value["missing"] != true
                                && !ignored::contains(
                                    &conn,
                                    &source_id,
                                    store::text(&value, "path"),
                                )?)
                        })
                        .unwrap_or(false);
                    if !allowed {
                        continue;
                    }
                    let _ = tauri::async_runtime::spawn_blocking(move || {
                        probe::probe_resource(&probe_dir, &resource)
                    })
                    .await;
                    if let Ok(conn) = store::open(&dir) {
                        let _ = store::job_update(
                            &conn,
                            &job_id,
                            "running",
                            index as i64,
                            resources.len() as i64,
                            "正在读取媒体技术信息",
                            "",
                        );
                    }
                    let _ = app.emit("media-job-changed", ());
                }
                if let Ok(conn) = store::open(&dir) {
                    let enabled = require_enabled(&conn, &source_id).is_ok();
                    let n = result["files"].as_i64().unwrap_or(0);
                    let _ = store::job_update(
                        &conn,
                        &job_id,
                        if enabled { "completed" } else { "cancelled" },
                        n,
                        n,
                        if !enabled {
                            SOURCE_DISABLED
                        } else if result["complete"] == true {
                            "扫描完成"
                        } else {
                            "扫描完成，部分目录未读取"
                        },
                        "",
                    );
                }
            }
            Err(error) => {
                if let Ok(conn) = store::open(&dir) {
                    let state = if error == SOURCE_DISABLED {
                        "cancelled"
                    } else {
                        "failed"
                    };
                    let _ = store::job_update(&conn, &job_id, state, 0, 0, "扫描未完成", &error);
                }
            }
        }
        if let Ok(mut running) = RUNNING.get_or_init(Default::default).lock() {
            running.remove(&key);
        }
        drop(operation);
        // 先取得技术信息供自动识别门禁使用，再仅尝试本次新条目。
        if new_items.as_array().is_some_and(|ids| !ids.is_empty()) {
            let _ = super::metadata::refresh(
                app.clone(), json!({"automatic":true,"itemIds":new_items}),
            ).await;
        }
        // 资料任务忙碌时由定时任务补充发现；扫描不绕过已有失败的退避。
        let _ = app.emit("media-changed", ());
    });
    Ok(json!({"jobId":id,"sourceId":returned_source_id}))
}
pub async fn add_source(app: tauri::AppHandle, input: Value) -> Result<Value, String> {
    let operation = super::operation(&app).await?;
    let dir = app.state::<State>().dir.clone();
    let source = tauri::async_runtime::spawn_blocking(move || save_source(&dir, &input))
        .await
        .map_err(|_| "添加媒体目录失败")??;
    drop(operation);
    let mut result = start_scan(app.clone(), source.clone()).await?;
    result["source"] = catalog::source_list(&store::open(&app.state::<State>().dir)?)?
        .into_iter()
        .find(|value| value["id"] == source)
        .unwrap_or(Value::Null);
    let _ = app.emit("media-changed", ());
    Ok(result)
}

fn save_source(dir: &Path, input: &Value) -> Result<String, String> {
    let root = normalized(Path::new(store::text(input, "path")))?;
    if !root.is_dir() {
        return Err("请选择媒体文件夹".into());
    }
    let mut conn = store::open(dir)?;
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    let path = root.to_string_lossy().to_string();
    let existing: Option<String> = tx
        .query_row(
            "SELECT id FROM sources WHERE path=?1 COLLATE NOCASE",
            [&path],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    let id = existing.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    for source in catalog::source_list(&tx)? {
        if source["id"] != id && overlapping_paths(&path, store::text(&source, "path")) {
            return Err("该目录与已有媒体目录重叠，请选择互不包含的媒体目录".into());
        }
    }
    let label = input["label"]
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| {
            root.file_name()
                .map(|value| value.to_string_lossy().to_string())
                .unwrap_or_else(|| path.clone())
        });
    let kind = match store::text(input, "kind") {
        kind @ ("auto" | "movie" | "movies" | "tv" | "series" | "other") => kind,
        _ => "auto",
    };
    tx.execute(
        "INSERT INTO sources(id,path,label,kind,smart_folders,created_at)VALUES(?1,?2,?3,?4,?5,?6)
         ON CONFLICT(id)DO UPDATE SET label=excluded.label,kind=excluded.kind,smart_folders=excluded.smart_folders",
        params![id,path,label,kind,input["smartFolders"].as_bool().unwrap_or(true),store::now()],
    ).map_err(|e| e.to_string())?;
    store::set_setting(&tx, &format!("source.disabled.{id}"), &json!(false))?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(id)
}
fn overlapping_paths(left: &str, right: &str) -> bool {
    let normalize = |path: &str| path.replace('\\', "/").trim_end_matches('/').to_lowercase();
    let left = normalize(left);
    let right = normalize(right);
    left == right || left.starts_with(&(right.clone() + "/")) || right.starts_with(&(left + "/"))
}
pub async fn scan(app: tauri::AppHandle, input: Value) -> Result<Value, String> {
    if let Some(id) = input["sourceId"].as_str() {
        match start_scan(app.clone(), id.to_string()).await {
            Err(error) if error == SCAN_RUNNING && input["afterCurrent"] == true => {
                let pending = format!("pending:{}:{id}", app.state::<State>().dir.display());
                let queued = RUNNING
                    .get_or_init(Default::default)
                    .lock()
                    .map_err(|_| "扫描状态不可用")?
                    .insert(pending.clone());
                if queued {
                    let id = id.to_string();
                    tauri::async_runtime::spawn(async move {
                        loop {
                            tokio::time::sleep(Duration::from_secs(1)).await;
                            match start_scan(app.clone(), id.clone()).await {
                                Err(error) if error == SCAN_RUNNING => continue,
                                _ => break,
                            }
                        }
                        if let Ok(mut running) = RUNNING.get_or_init(Default::default).lock() {
                            running.remove(&pending);
                        }
                    });
                }
                Ok(json!({"queued":true,"sourceId":id}))
            }
            result => result,
        }
    } else {
        let conn = store::open(&app.state::<State>().dir)?;
        let sources = catalog::source_list(&conn)?;
        drop(conn);
        let mut jobs = Vec::new();
        for source in sources {
            if let Ok(job) = start_scan(app.clone(), store::text(&source, "id").into()).await {
                jobs.push(job);
            }
        }
        Ok(json!({"jobs":jobs}))
    }
}
pub async fn remove_source(app: tauri::AppHandle, input: Value) -> Result<Value, String> {
    let operation = super::operation(&app).await?;
    let dir = app.state::<State>().dir.clone();
    let id = store::text(&input, "sourceId").to_string();
    let status = playback::status(app.clone(), json!({})).await?;
    if status["active"] == true {
        let conn = store::open(&dir)?;
        if store::resource(&conn, store::text(&status, "resourceId"))
            .ok()
            .is_some_and(|resource| resource["sourceId"] == id)
        {
            return Err("该媒体目录中有文件正在播放，请先停止播放再移除".into());
        }
    }
    tauri::async_runtime::spawn_blocking(move || {
        let _operation = operation;
        disable_source(&mut store::open(&dir)?, &id)
    })
    .await
    .map_err(|_| "停用媒体目录失败")??;
    let _ = app.emit("media-changed", ());
    Ok(json!({"ok":true}))
}
pub async fn browse(app: tauri::AppHandle, input: Value) -> Result<Value, String> {
    let operation = super::operation(&app).await?;
    let dir = app.state::<State>().dir.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _operation = operation;
        browse_directory(&store::open(&dir)?, &input)
    })
    .await
    .map_err(|_| "浏览媒体目录失败")?
}

pub fn start(app: tauri::AppHandle) {
    #[cfg(windows)]
    watch::start(app.clone());
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
            let sources = {
                let state = app.state::<State>();
                let Ok(conn) = store::open(&state.dir) else {
                    continue;
                };
                let interval = store::setting(&conn, "autoScanIntervalMinutes")
                    .ok()
                    .and_then(|v| v.as_u64())
                    .unwrap_or(30);
                if interval == 0 {
                    continue;
                }
                let Ok(sources) = catalog::source_list(&conn) else {
                    continue;
                };
                sources
                    .into_iter()
                    .filter(|source| {
                        if store::setting(
                            &conn,
                            &format!("source.disabled.{}", store::text(source, "id")),
                        )
                        .ok()
                            == Some(json!(true))
                        {
                            return false;
                        }
                        chrono::DateTime::parse_from_rfc3339(store::text(source, "lastScan"))
                            .map(|last| {
                                chrono::Utc::now().signed_duration_since(last).num_minutes()
                                    >= interval as i64
                            })
                            .unwrap_or(true)
                    })
                    .collect::<Vec<_>>()
            };
            for source in sources {
                let _ = start_scan(app.clone(), store::text(&source, "id").to_string()).await;
            }
        }
    });
}

#[cfg(test)]
#[path = "ignored/marker_scan_tests.rs"]
mod marker_scan_tests;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ignoring_path_cancels_collected_files_and_browse_hides_it() {
        let data = tempfile::tempdir().unwrap();
        let media = tempfile::tempdir().unwrap();
        let folder = media.path().join("Extras");
        fs::create_dir(&folder).unwrap();
        fs::write(folder.join("old.mkv"), b"sample").unwrap();
        let id = save_source(data.path(), &json!({"path":media.path().to_string_lossy()})).unwrap();
        let mut conn = store::open(data.path()).unwrap();
        let job = store::job_start(&conn, "scan", 0).unwrap();
        let root = fs::canonicalize(media.path()).unwrap();
        let (files, complete) = collect(&root, &conn, &id).unwrap();
        ignored::save(
            &mut conn,
            &json!({"sourceId":id,"path":folder.to_string_lossy()}),
        )
        .unwrap();
        let result = apply_scan(&mut conn, &id, &job, &files, complete).unwrap();
        assert_eq!(result["files"], 0);
        assert!(result["changedResourceIds"].as_array().unwrap().is_empty());
        assert!(store::items(&conn).unwrap().is_empty());
        assert!(
            browse_directory(&conn, &json!({"sourceId":id})).unwrap()["entries"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert!(browse_directory(
            &conn,
            &json!({"sourceId":id,"path":folder.to_string_lossy()})
        )
        .unwrap_err()
        .contains("已忽略"));
        ignored::save(
            &mut conn,
            &json!({"sourceId":id,"path":folder.to_string_lossy(),"remove":true}),
        )
        .unwrap();
        assert_eq!(
            browse_directory(&conn, &json!({"sourceId":id})).unwrap()["entries"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }
    #[test]
    fn adding_source_keeps_overlap_guard_and_can_reenable_removed_source() {
        let data = tempfile::tempdir().unwrap();
        let media = tempfile::tempdir().unwrap();
        fs::write(media.path().join("film.mkv"), b"film").unwrap();
        let input = json!({"path":media.path().to_string_lossy(),"label":"test"});
        let id = save_source(data.path(), &input).unwrap();
        let mut conn = store::open(data.path()).unwrap();
        let job = store::job_start(&conn, "scan", 0).unwrap();
        scan_source(data.path(), &id, &job).unwrap();
        disable_source(&mut conn, &id).unwrap();
        assert_eq!(save_source(data.path(), &input).unwrap(), id);
        scan_source(data.path(), &id, &job).unwrap();
        assert_eq!(catalog::source_list(&conn).unwrap()[0]["available"], true);
        assert_eq!(store::items(&conn).unwrap()[0]["playable"], true);
        let child = media.path().join("child");
        fs::create_dir(&child).unwrap();
        assert!(
            save_source(data.path(), &json!({"path":child.to_string_lossy()}))
                .unwrap_err()
                .contains("重叠")
        );
    }
    #[test]
    fn disabling_source_rejects_scan_batches_collected_before_removal() {
        let data = tempfile::tempdir().unwrap();
        let media = tempfile::tempdir().unwrap();
        fs::write(media.path().join("film.mkv"), b"film").unwrap();
        let mut conn = store::open(data.path()).unwrap();
        conn.execute(
            "INSERT INTO sources(id,path,label,created_at)VALUES('s',?1,'test','')",
            [media.path().to_string_lossy().as_ref()],
        )
        .unwrap();
        let job = store::job_start(&conn, "scan", 0).unwrap();
        scan_source(data.path(), "s", &job).unwrap();
        let item_id = store::items(&conn).unwrap()[0]["id"]
            .as_str()
            .unwrap()
            .to_string();
        store::update_record(&conn, &item_id, &json!({"rating":4}), true).unwrap();
        // 目录枚举结束后被停用，旧批次不得重新启用已索引资源或导入新文件。
        fs::write(media.path().join("new.mkv"), b"new").unwrap();
        let (files, complete) = collect(media.path(), &conn, "s").unwrap();
        disable_source(&mut conn, "s").unwrap();
        assert_eq!(
            apply_scan(&mut conn, "s", &job, &files, complete).unwrap_err(),
            SOURCE_DISABLED
        );
        assert_eq!(
            scan_source(data.path(), "s", &job).unwrap_err(),
            SOURCE_DISABLED
        );
        // 空目录的扫描收尾同样不能把源状态改回可用。
        assert_eq!(
            apply_scan(&mut conn, "s", &job, &[], true).unwrap_err(),
            SOURCE_DISABLED
        );
        assert!(catalog::source_list(&conn).unwrap().is_empty());
        let resource = store::resources_for(&conn, &item_id).unwrap().remove(0);
        assert_eq!(resource["available"], false);
        assert_eq!(resource["missing"], true);
        let item = store::item(&conn, &item_id).unwrap();
        assert_eq!(item["independent"], true);
        assert_eq!(item["rating"], 4.0);
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM resources", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }
    #[test]
    fn changed_files_reset_only_resource_state_and_preserve_records() {
        for change_size in [true, false] {
            let data = tempfile::tempdir().unwrap();
            let media = tempfile::tempdir().unwrap();
            let file = media.path().join("Film.mkv");
            let other_file = media.path().join("Other.mkv");
            fs::write(&file, b"original").unwrap();
            fs::write(&other_file, b"other version").unwrap();
            let conn = store::open(data.path()).unwrap();
            conn.execute(
                "INSERT INTO sources(id,path,label,created_at)VALUES('s',?1,'sample',?2)",
                params![media.path().to_string_lossy(), store::now()],
            )
            .unwrap();
            let job = store::job_start(&conn, "scan", 0).unwrap();
            scan_source(data.path(), "s", &job).unwrap();
            let id: String = conn
                .query_row(
                    "SELECT id FROM resources WHERE path=?1",
                    [file.to_string_lossy().as_ref()],
                    |row| row.get(0),
                )
                .unwrap();
            let item_id = store::resource(&conn, &id).unwrap()["itemIds"][0]
                .as_str()
                .unwrap()
                .to_string();
            store::update_record(
                &conn, &item_id,
                &json!({"watched":true,"favorite":true,"rating":4,"comment":"保留短评","markedAt":"2020-01-02"}),
                true,
            ).unwrap();
            conn.execute(
                "UPDATE resources SET position_seconds=80,duration_seconds=100,technical_json='{\"state\":\"ready\"}'",
                [],
            ).unwrap();
            conn.execute(
                "INSERT INTO media_playback_coverage(resource_id,segments_json)SELECT id,'[[0,95]]' FROM resources",
                [],
            ).unwrap();
            conn.execute(
                "INSERT INTO view_sessions(id,item_id,resource_id,started_at,ended_at,seconds,position_seconds)VALUES('history',?1,?2,'2020-01-02T00:00:00Z','2020-01-02T00:01:00Z',60,80)",

                params![item_id, id],
            ).unwrap();
            let unchanged = scan_source(data.path(), "s", &job).unwrap();
            assert_eq!(unchanged["changedResourceIds"], json!([]));
            assert_eq!(
                store::resource(&conn, &id).unwrap()["positionSeconds"],
                80.0
            );
            if change_size {
                fs::write(&file, b"a completely different cut").unwrap();
            } else {
                let modified = fs::metadata(&file).unwrap().modified().unwrap();
                fs::OpenOptions::new()
                    .write(true)
                    .open(&file)
                    .unwrap()
                    .set_times(
                        fs::FileTimes::new().set_modified(modified + Duration::from_secs(10)),
                    )
                    .unwrap();
            }
            let changed = scan_source(data.path(), "s", &job).unwrap();
            assert_eq!(changed["changedResourceIds"], json!([id]));
            let resource = store::resource(&conn, &id).unwrap();
            assert_eq!(resource["positionSeconds"], 0.0);
            assert_eq!(resource["durationSeconds"], 0.0);
            assert_eq!(resource["technical"], json!({}));
            let coverage_count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM media_playback_coverage WHERE resource_id=?1",
                    [&id],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(coverage_count, 0);
            let other: (f64, String) = conn.query_row(
                "SELECT r.position_seconds,c.segments_json FROM resources r JOIN media_playback_coverage c ON c.resource_id=r.id WHERE r.id<>?1",
                [&id], |r| Ok((r.get(0)?, r.get(1)?)),
            ).unwrap();
            assert_eq!(other, (80.0, "[[0,95]]".into()));
            let item = store::item(&conn, &item_id).unwrap();
            assert_eq!(item["watched"], true);
            assert_eq!(item["favorite"], true);
            assert_eq!(item["rating"], 4.0);
            assert_eq!(item["comment"], "保留短评");
            assert_eq!(item["markedAt"], "2020-01-02");
            let history: (f64, f64) = conn
                .query_row(
                    "SELECT seconds,position_seconds FROM view_sessions WHERE id='history'",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .unwrap();
            assert_eq!(history, (60.0, 80.0));
        }
    }
    #[test]
    fn scan_is_idempotent_and_missing_preserves_personal_data() {
        let data = tempfile::tempdir().unwrap();
        let media = tempfile::tempdir().unwrap();
        let file = media.path().join("Film.2024.mkv");
        fs::write(&file, b"sample").unwrap();
        let conn = store::open(data.path()).unwrap();
        conn.execute(
            "INSERT INTO sources(id,path,label,created_at)VALUES('s',?1,'sample',?2)",
            params![media.path().to_string_lossy(), store::now()],
        )
        .unwrap();
        let job = store::job_start(&conn, "scan", 0).unwrap();
        scan_source(data.path(), "s", &job).unwrap();
        scan_source(data.path(), "s", &job).unwrap();
        let items = store::items(&conn).unwrap();
        assert_eq!(items.len(), 1);
        let id = store::text(&items[0], "id");
        store::update_record(&conn, id, &json!({"watched":true,"rating":4}), true).unwrap();
        fs::remove_file(file).unwrap();
        scan_source(data.path(), "s", &job).unwrap();
        let item = store::item(&conn, id).unwrap();
        assert_eq!(item["independent"], true);
        assert_eq!(item["watched"], true);
        assert_eq!(item["rating"], 4.0);
        assert_eq!(store::resources_for(&conn, id).unwrap()[0]["missing"], true);
    }
    #[test]
    fn offline_source_does_not_claim_files_deleted() {
        let data = tempfile::tempdir().unwrap();
        let media = tempfile::tempdir().unwrap();
        fs::write(media.path().join("x.mp4"), b"x").unwrap();
        let conn = store::open(data.path()).unwrap();
        conn.execute(
            "INSERT INTO sources(id,path,label,created_at)VALUES('s',?1,'sample',?2)",
            params![media.path().to_string_lossy(), store::now()],
        )
        .unwrap();
        let job = store::job_start(&conn, "scan", 0).unwrap();
        scan_source(data.path(), "s", &job).unwrap();
        let id = store::items(&conn).unwrap()[0]["id"]
            .as_str()
            .unwrap()
            .to_string();
        conn.execute(
            "UPDATE sources SET path=?1 WHERE id='s'",
            [media.path().join("offline").to_string_lossy().as_ref()],
        )
        .unwrap();
        assert!(scan_source(data.path(), "s", &job).is_err());
        let item = store::item(&conn, &id).unwrap();
        assert_eq!(item["id"], id);
        let resources = store::resources_for(&conn, &id).unwrap();
        assert_eq!(resources[0]["missing"], false);
        assert_eq!(resources[0]["available"], false);
    }
    #[test]
    fn smart_folder_keeps_unidentified_files_visible() {
        let data = tempfile::tempdir().unwrap();
        let media = tempfile::tempdir().unwrap();
        let movie = media.path().join("Movie");
        fs::create_dir(&movie).unwrap();
        fs::write(movie.join("Movie.2024.mkv"), b"a").unwrap();
        fs::write(movie.join("Unknown.mkv"), b"b").unwrap();
        let conn = store::open(data.path()).unwrap();
        conn.execute(
            "INSERT INTO sources(id,path,label,created_at)VALUES('s',?1,'sample',?2)",
            params![media.path().to_string_lossy(), store::now()],
        )
        .unwrap();
        let job = store::job_start(&conn, "scan", 0).unwrap();
        scan_source(data.path(), "s", &job).unwrap();
        let values = store::items(&conn).unwrap();
        let first = values
            .iter()
            .find(|v| store::text(v, "title").starts_with("Movie"))
            .unwrap();
        conn.execute(
            "UPDATE media_items SET title='Movie',kind='movie',recognized=1 WHERE id=?1",
            [store::text(first, "id")],
        )
        .unwrap();
        let values = store::items(&conn).unwrap();
        let by_id: HashMap<&str, &Value> =
            values.iter().map(|v| (store::text(v, "id"), v)).collect();
        assert!(smart_directory(&conn, "s", &movie, &by_id)
            .unwrap()
            .is_none());
        let second = values
            .iter()
            .find(|v| store::text(v, "title") == "Unknown.mkv")
            .unwrap();
        store::merge_items(&conn, store::text(second, "id"), store::text(first, "id")).unwrap();
        let values = store::items(&conn).unwrap();
        let by_id = values.iter().map(|v| (store::text(v, "id"), v)).collect();
        assert!(smart_directory(&conn, "s", &movie, &by_id)
            .unwrap()
            .is_some());
    }
}
