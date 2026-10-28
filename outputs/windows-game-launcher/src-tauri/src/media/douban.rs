use super::{metadata, store, State};
mod import;
mod manual;
pub use manual::manual_metadata;
pub(super) use manual::preview_poster;
pub(super) mod details;
use chrono::{DateTime, Utc};
#[cfg(test)]
use import::display_title;
pub(super) use import::import_page;
use import::{before_boundary, cursor_key, finish_import};
use rusqlite::params;
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc, Arc, Mutex, OnceLock,
    },
    time::Duration,
};
use tauri::{Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

const WINDOW: &str = "media-douban";
const MINE: &str = "https://movie.douban.com/mine";
const MAX_PAGES: usize = 10000;

#[derive(Default)]
struct Runtime {
    busy: AtomicBool,
    cancel: AtomicBool,
    generation: AtomicU64,
    writes: Mutex<()>,
    preview: Mutex<Option<manual::Preview>>,
}
fn runtime(dir: &std::path::Path) -> Arc<Runtime> {
    static MAP: OnceLock<Mutex<HashMap<PathBuf, Arc<Runtime>>>> = OnceLock::new();
    let mut map = MAP
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    map.entry(dir.to_path_buf()).or_default().clone()
}
fn dir(app: &tauri::AppHandle) -> PathBuf {
    app.state::<State>().dir.clone()
}
fn changed(app: &tauri::AppHandle) {
    let _ = app.emit("media-changed", ());
    let _ = app.emit("media-job-changed", ());
}
fn connection(app: &tauri::AppHandle) -> Result<Value, String> {
    store::setting(&store::open(&dir(app))?, "doubanConnection")
}
fn save_connection(app: &tauri::AppHandle, value: &Value) -> Result<(), String> {
    store::set_setting(&store::open(&dir(app))?, "doubanConnection", value)
}
fn safe_status(app: &tauri::AppHandle) -> Result<Value, String> {
    let value = connection(app)?;
    Ok(json!({
        "connected":value["connected"].as_bool().unwrap_or(false),
        "displayName":value["displayName"].as_str().unwrap_or(""),
        "phase":value["phase"].as_str().unwrap_or("disconnected"),
        "needsLogin":value["needsLogin"].as_bool().unwrap_or(false),
        "busy":runtime(&dir(app)).busy.load(Ordering::Acquire),
        "message":value["message"].as_str().unwrap_or(""),
        "lastSync":value["lastSync"].as_str().unwrap_or(""),
        "completed":value["completed"].as_u64().unwrap_or(0),
        "total":value["total"].as_u64().unwrap_or(0),
        "jobId":value["jobId"].as_str().unwrap_or(""),
    }))
}
fn allowed(url: &tauri::Url) -> bool {
    url.scheme() == "https"
        && url.port_or_known_default() == Some(443)
        && url
            .host_str()
            .is_some_and(|host| host == "douban.com" || host.ends_with(".douban.com"))
}
async fn browser(app: &tauri::AppHandle, visible: bool) -> Result<WebviewWindow, String> {
    if let Some(window) = app.get_webview_window(WINDOW) {
        if visible {
            let _ = window.show();
            let _ = window.set_focus();
        }
        return Ok(window);
    }
    let profile = dir(app).join("media").join("douban-profile");
    std::fs::create_dir_all(&profile).map_err(|_| "无法创建豆瓣浏览会话目录")?;
    let builder =
        WebviewWindowBuilder::new(app, WINDOW, WebviewUrl::External(MINE.parse().unwrap()))
            .title("登录豆瓣 · 连接观影记录")
            .inner_size(1080.0, 800.0)
            .visible(visible)
            .data_directory(profile)
            .on_navigation(allowed);
    crate::webview_proxy::configure(app, builder)
        .build()
        .map_err(|_| "无法打开豆瓣官方登录窗口".into())
}
async fn extract(window: &WebviewWindow) -> Result<Value, String> {
    let url = window.url().map_err(|_| "豆瓣窗口不可用")?;
    if !allowed(&url) {
        return Err("豆瓣页面地址不在允许范围内".into());
    }
    if url.host_str() == Some("sec.douban.com") {
        return Ok(json!({"restricted":true,"ready":"complete"}));
    }
    if url.host_str() != Some("movie.douban.com") {
        return Ok(json!({"login":true,"ready":"complete"}));
    }
    let (tx, rx) = mpsc::channel();
    window
        .eval_with_callback(include_str!("douban_extract.js"), move |value| {
            let _ = tx.send(value);
        })
        .map_err(|_| "无法读取豆瓣页面")?;
    let raw =
        tauri::async_runtime::spawn_blocking(move || rx.recv_timeout(Duration::from_secs(10)))
            .await
            .map_err(|_| "豆瓣页面读取任务失败")?
            .map_err(|_| "豆瓣页面读取超时")?;
    serde_json::from_str(&raw).map_err(|_| "豆瓣页面未返回有效数据".into())
}
fn list_url(raw: &str, account: &str) -> Result<tauri::Url, String> {
    let url: tauri::Url = raw.parse().map_err(|_| "豆瓣列表地址无效")?;
    let expected = format!("/people/{account}/collect");
    if url.scheme() != "https"
        || url.host_str() != Some("movie.douban.com")
        || url.port_or_known_default() != Some(443)
        || url.path().trim_end_matches('/') != expected
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("豆瓣列表地址与已连接账号不一致".into());
    }
    Ok(url)
}
async fn navigate(
    window: &WebviewWindow,
    url: &tauri::Url,
    rt: &Runtime,
    generation: u64,
) -> Result<Value, String> {
    window
        .navigate(url.clone())
        .map_err(|_| "无法加载豆瓣列表")?;
    for _ in 0..50 {
        if rt.cancel.load(Ordering::Acquire) || rt.generation.load(Ordering::Acquire) != generation
        {
            return Err("豆瓣更新已取消".into());
        }
        tokio::time::sleep(Duration::from_millis(400)).await;
        if let Ok(page) = extract(window).await {
            if page["restricted"] == true {
                return Err("豆瓣需要安全验证，请打开连接窗口完成验证后重试".into());
            }
            // 公开详情不要求登录；只接受所请求的豆瓣 subject 页面。
            if page["ready"] == "complete" && page["href"] == url.as_str()
                && page["isSubject"] == true && page["detail"].is_object()
            {
                return Ok(page);
            }
            if page["login"] == true && page["ready"] == "complete" {
                return Err("豆瓣登录已失效，请重新登录".into());
            }
            if page["ready"] == "complete"
                && page["href"]
                    .as_str()
                    .is_some_and(|href| href == url.as_str())
            {
                if page["isList"] == true || page["isMine"] == true || page["isSubject"] == true {
                    return Ok(page);
                }
            }
        }
    }
    Err("豆瓣列表加载超时，请检查网络后重试".into())
}
pub async fn status(app: tauri::AppHandle, _input: Value) -> Result<Value, String> {
    safe_status(&app)
}

pub async fn begin_login(app: tauri::AppHandle, _input: Value) -> Result<Value, String> {
    let rt = runtime(&dir(&app));
    if rt.busy.load(Ordering::Acquire) {
        return Err("豆瓣正在更新，请等待完成或先取消更新".into());
    }
    let generation = rt.generation.fetch_add(1, Ordering::AcqRel) + 1;
    rt.cancel.store(false, Ordering::Release);
    let window = browser(&app, true).await?;
    // 已有验证页面原样显示，避免重定向丢失用户待完成的人机验证。
    let current = window.url().map_err(|_| "豆瓣窗口不可用")?;
    if current.host_str() == Some("movie.douban.com") && !current.path().starts_with("/misc/") {
        let _ = window.navigate(MINE.parse().unwrap());
    }
    write_active(&app, &rt, generation, || {
        let mut state = connection(&app).unwrap_or_else(|_| json!({}));
        state["phase"] = json!("loggingIn");
        state["message"] = json!("请在豆瓣官方窗口完成登录，成功后会自动关闭窗口并后台获取");
        save_connection(&app, &state)
    })
    .await?;
    changed(&app);
    let response = safe_status(&app);
    tauri::async_runtime::spawn(async move {
        for _ in 0..900 {
            tokio::time::sleep(Duration::from_secs(1)).await;
            if rt.generation.load(Ordering::Acquire) != generation {
                return;
            }
            let Some(window) = app.get_webview_window(WINDOW) else {
                return;
            };
            let Ok(page) = extract(&window).await else {
                continue;
            };
            if page["ready"] != "complete"
                || page["login"] == true
                || page["restricted"] == true
                || page["loggedIn"] != true
            {
                continue;
            }
            if page["isMine"] != true {
                let _ = window.navigate(MINE.parse().unwrap());
                continue;
            }
            let account = page["accountKey"].as_str().unwrap_or("");
            if account.is_empty()
                || !account
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c))
            {
                continue;
            }
            let saved = write_active(&app, &rt, generation, || {
                let mut state = connection(&app).unwrap_or_else(|_| json!({}));
                let same = state["accountKey"] == account;
                if !same {
                    state = json!({});
                }
                state["connected"] = json!(true);
                state["needsLogin"] = json!(false);
                state["phase"] = json!("connected");
                state["accountKey"] = json!(account);
                state["displayName"] = page["displayName"].clone();
                state["collectHref"] = page["collectHref"].clone();
                state["message"] = json!("豆瓣已连接，正在后台读取观影记录");
                let _ = save_connection(&app, &state);
                if let Ok(conn) = store::open(&dir(&app)) {
                    if store::setting(&conn, "doubanAutoUpdate")
                        .ok()
                        .is_some_and(|value| value.is_null())
                    {
                        let _ = store::set_setting(&conn, "doubanAutoUpdate", &json!(true));
                    }
                }
                Ok(())
            })
            .await;
            let Ok(()) = saved else {
                return;
            };
            let _ = window.hide();
            changed(&app);
            let _ = sync(app.clone(), json!({})).await;
            return;
        }
        let _ = write_active(&app, &rt, generation, || {
            let mut state = connection(&app).unwrap_or_else(|_| json!({}));
            state["phase"] = json!("needsLogin");
            state["needsLogin"] = json!(true);
            state["message"] = json!("豆瓣登录等待超时，请重新连接");
            save_connection(&app, &state)
        })
        .await;
        changed(&app);
    });
    response
}

fn active(rt: &Runtime, generation: u64) -> Result<(), String> {
    if rt.cancel.load(Ordering::Acquire) || rt.generation.load(Ordering::Acquire) != generation {
        Err("豆瓣更新已取消".into())
    } else {
        Ok(())
    }
}
// 只串行化短暂的数据库批次；断开连接等待当前批次结束，然后使旧任务永久失效。
async fn write_active<T>(
    app: &tauri::AppHandle,
    rt: &Runtime,
    generation: u64,
    operation: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    let _operation = super::operation(app).await?;
    let _guard = rt.writes.lock().unwrap_or_else(|e| e.into_inner());
    active(rt, generation)?;
    operation()
}
fn elapsed(value: &Value, key: &str) -> Option<i64> {
    value[key]
        .as_str()
        .and_then(|s| s.parse::<DateTime<Utc>>().ok())
        .map(|date| (Utc::now() - date).num_seconds())
}
pub async fn sync(app: tauri::AppHandle, input: Value) -> Result<Value, String> {
    let rt = runtime(&dir(&app));
    if input["cancel"] == true {
        let _guard = rt.writes.lock().unwrap_or_else(|e| e.into_inner());
        rt.cancel.store(true, Ordering::Release);
        return Ok(json!({"cancelled":true}));
    }
    if rt.busy.swap(true, Ordering::AcqRel) {
        return Ok(json!({"queued":false,"busy":true}));
    }
    let value = match connection(&app) {
        Ok(value) => value,
        Err(error) => {
            rt.busy.store(false, Ordering::Release);
            return Err(error);
        }
    };
    if value["connected"] != true {
        rt.busy.store(false, Ordering::Release);
        return Err("请先连接豆瓣账号".into());
    }
    rt.cancel.store(false, Ordering::Release);
    let generation = rt.generation.load(Ordering::Acquire);
    let prepared = write_active(
        &app,
        &rt,
        generation,
        || -> Result<(String, Value), String> {
            let conn = store::open(&dir(&app))?;
            let job = store::job_start(&conn, "douban-sync", 0)?;
            let mut state = value;
            state["phase"] = json!("syncing");
            state["completed"] = json!(0);
            state["total"] = json!(0);
            state["needsLogin"] = json!(false);
            state["jobId"] = json!(&job);
            state["lastAttempt"] = json!(store::now());
            state["message"] = json!("正在后台读取豆瓣观影记录");
            save_connection(&app, &state)?;
            Ok((job, state))
        },
    )
    .await;
    let (job, _state) = match prepared {
        Ok(value) => value,
        Err(error) => {
            rt.busy.store(false, Ordering::Release);
            return Err(error);
        }
    };
    changed(&app);
    let response = json!({"queued":true,"jobId":&job});
    tauri::async_runtime::spawn(async move {
        let result = run_sync(&app, &rt, generation, &job).await;
        let Ok(_operation) = super::operation(&app).await else {
            rt.busy.store(false, Ordering::Release);
            return;
        };
        let _guard = rt.writes.lock().unwrap_or_else(|e| e.into_inner());
        if rt.generation.load(Ordering::Acquire) != generation {
            if let Ok(conn) = store::open(&dir(&app)) {
                let _ = conn.execute("UPDATE jobs SET state='cancelled',message='连接已断开，任务已取消',updated_at=?2 WHERE id=?1",params![job,store::now()]);
            }
            rt.busy.store(false, Ordering::Release);
            changed(&app);
            return;
        }
        let mut state = connection(&app).unwrap_or_else(|_| json!({}));
        match result {
            Ok(completed) => {
                state["phase"] = json!("connected");
                state["lastSync"] = json!(store::now());
                state["message"] = json!(format!("豆瓣增量导入完成，本次检查 {completed} 条"));
                state["needsLogin"] = json!(false);
            }
            Err(error) => {
                let need = error.contains("登录") || error.contains("验证");
                state["phase"] = json!(if need {
                    "needsLogin"
                } else if rt.cancel.load(Ordering::Acquire) {
                    "cancelled"
                } else {
                    "error"
                });
                state["needsLogin"] = json!(need);
                state["message"] = json!(&error);
                if let Ok(conn) = store::open(&dir(&app)) {
                    let count = state["completed"].as_i64().unwrap_or(0);
                    let total = state["total"].as_i64().unwrap_or(0);
                    let _ = store::job_update(
                        &conn,
                        &job,
                        if rt.cancel.load(Ordering::Acquire) {
                            "cancelled"
                        } else {
                            "failed"
                        },
                        count,
                        total,
                        "豆瓣更新暂停",
                        &error,
                    );
                }
            }
        }
        let _ = save_connection(&app, &state);
        rt.busy.store(false, Ordering::Release);
        changed(&app);
        drop(_guard);
        drop(_operation);
        // 导入和资料任务互不依赖，部分导入后失败也可以补全已保存条目。
        let _ = metadata::refresh(app.clone(), json!({"automatic":true})).await;
    });
    Ok(response)
}
async fn run_sync(
    app: &tauri::AppHandle,
    rt: &Runtime,
    generation: u64,
    job: &str,
) -> Result<usize, String> {
    let state = connection(app)?;
    let account = state["accountKey"]
        .as_str()
        .ok_or("豆瓣账号身份缺失，请重新登录")?
        .to_string();
    let mut next = list_url(state["collectHref"].as_str().unwrap_or(""), &account)?;
    next.query_pairs_mut()
        .clear()
        .append_pair("sort", "time")
        .append_pair("mode", "grid");
    let boundary = store::setting(&store::open(&dir(app))?, &cursor_key(&account))?
        .as_str()
        .unwrap_or("")
        .to_string();
    let window = browser(app, false).await?;
    let mut completed = 0;
    let mut imported = 0;
    let mut head: Option<String> = None;
    let mut seen = HashSet::new();
    for _ in 0..MAX_PAGES {
        let mut attempts = 0;
        let page = loop {
            match navigate(&window, &next, rt, generation).await {
                Ok(page) => break page,
                Err(error)
                    if !error.contains("登录")
                        && !error.contains("验证")
                        && !error.contains("取消")
                        && attempts < 2 =>
                {
                    attempts += 1;
                    tokio::time::sleep(Duration::from_secs(attempts * 2)).await;
                }
                Err(error) => return Err(error),
            }
        };
        if rt.cancel.load(Ordering::Acquire) || rt.generation.load(Ordering::Acquire) != generation
        {
            return Err("豆瓣更新已取消".into());
        }
        let rows = page["records"].as_array().ok_or("豆瓣页面结构无法识别")?;
        if rows.is_empty() && page["empty"] != true {
            return Err("豆瓣未返回有效列表，已保留现有记录并停止更新".into());
        }
        if rows
            .iter()
            .any(|row| row["title"].as_str().is_none_or(str::is_empty))
        {
            return Err("豆瓣条目标题读取不完整，已停止更新".into());
        }
        for row in rows {
            let id = row["subjectId"].as_str().ok_or("豆瓣条目编号缺失")?;
            if !seen.insert(id.to_string()) {
                return Err("豆瓣分页出现重复条目，已停止以避免循环读取".into());
            }
        }
        if head.is_none() {
            head = rows
                .first()
                .and_then(|row| row["subjectId"].as_str())
                .map(str::to_owned);
        }
        let (pending, reached) = before_boundary(rows, &boundary);
        let following = page["next"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(|s| list_url(s, &account))
            .transpose()?;
        completed += pending.len();
        let count = write_active(app, rt, generation, || {
            let conn = store::open(&dir(app))?;
            let added = import_page(&conn, &account, pending)?;
            store::job_update(
                &conn,
                job,
                "running",
                completed as i64,
                completed as i64,
                &format!("已检查 {completed} 条，新增 {} 条", imported + added),
                "",
            )?;
            let mut status = connection(app)?;
            status["completed"] = json!(completed);
            status["total"] = json!(completed);
            save_connection(app, &status)?;
            Ok(added)
        })
        .await?;
        imported += count;
        changed(app);
        if reached || following.is_none() {
            write_active(app, rt, generation, || {
                finish_import(
                    &mut store::open(&dir(app))?,
                    &account,
                    head.as_deref(),
                    job,
                    completed,
                    imported,
                )
            })
            .await?;
            return Ok(completed);
        }
        let following = following.unwrap();
        if following == next {
            return Err("豆瓣下一页未前进，已停止更新".into());
        }
        next = following;
        tokio::time::sleep(Duration::from_millis(1200)).await;
    }
    Err("豆瓣列表超过本次读取上限，已导入内容保留，下次从头补齐".into())
}
pub async fn disconnect(app: tauri::AppHandle, _input: Value) -> Result<Value, String> {
    let rt = runtime(&dir(&app));
    {
        let _operation = super::operation(&app).await?;
        let _guard = rt.writes.lock().unwrap_or_else(|e| e.into_inner());
        rt.cancel.store(true, Ordering::Release);
        rt.generation.fetch_add(1, Ordering::AcqRel);
        save_connection(&app, &json!({"connected":false,"phase":"disconnected"}))?;
    }
    // 清除本模块隔离浏览会话；不触碰系统浏览器，也不删除已导入的条目和个人记录。
    let window = browser(&app, false).await?;
    let clearing = window.clone();
    tauri::async_runtime::spawn_blocking(move || clearing.clear_all_browsing_data())
        .await
        .map_err(|_| "豆瓣会话清理任务失败")?
        .map_err(|_| "无法清除豆瓣本地登录会话，请重试")?;
    let _ = window.close();

    changed(&app);
    safe_status(&app)
}
pub fn cancel_for_maintenance(app: &tauri::AppHandle) {
    let rt = runtime(&dir(app));
    let _guard = rt.writes.lock().unwrap_or_else(|e| e.into_inner());
    rt.cancel.store(true, Ordering::Release);
    rt.generation.fetch_add(1, Ordering::AcqRel);
}
fn recover_interrupted(conn: &rusqlite::Connection) -> Result<(), String> {
    conn.execute("UPDATE jobs SET state='interrupted',message='应用上次退出时豆瓣导入尚未完成，下次从列表开头补齐',updated_at=?1 WHERE kind='douban-sync' AND state='running'",
[store::now()]).map_err(|e|e.to_string())?;

    let mut state = store::setting(conn, "doubanConnection")?;
    if state["phase"] == "syncing" {
        state["phase"] = json!("interrupted");
        state["message"] = json!("上次导入中断，已保留条目与上次成功位置");
        store::set_setting(conn, "doubanConnection", &state)?;
    } else if state["phase"] == "loggingIn" {
        state["phase"] = json!("needsLogin");
        state["needsLogin"] = json!(true);
        state["message"] = json!("上次登录未完成，请重新连接豆瓣");
        store::set_setting(conn, "doubanConnection", &state)?;
    }
    Ok(())
}
pub fn start(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        if let Ok(_operation) = super::operation(&app).await {
            if let Ok(conn) = store::open(&dir(&app)) {
                let _ = recover_interrupted(&conn);
            }
        }
        tokio::time::sleep(Duration::from_secs(15)).await;
        loop {
            let due = (|| -> Result<bool, String> {
                let conn = store::open(&dir(&app))?;
                let enabled = store::setting(&conn, "doubanAutoUpdate")?
                    .as_bool()
                    .unwrap_or(false);
                let interval = store::setting(&conn, "doubanUpdateIntervalMinutes")?
                    .as_i64()
                    .unwrap_or(360)
                    .clamp(15, 10080)
                    * 60;
                let state = store::setting(&conn, "doubanConnection")?;
                Ok(enabled
                    && state["connected"] == true
                    && state["needsLogin"] != true
                    && (state["phase"] == "interrupted"
                        || (elapsed(&state, "lastAttempt").is_none_or(|elapsed| elapsed >= 900)
                            && elapsed(&state, "lastSync")
                                .is_none_or(|elapsed| elapsed >= interval))))
            })();
            if due == Ok(true) {
                let _ = sync(app.clone(), json!({})).await;
            }
            tokio::time::sleep(Duration::from_secs(60)).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_foreign_and_cross_account_list_urls() {
        assert!(list_url("https://movie.douban.com/people/a/collect?start=15", "a").is_ok());
        for raw in [
            "https://movie.douban.com/people/b/collect",
            "http://movie.douban.com/people/a/collect",
            "https://evil.test/people/a/collect",
            "https://movie.douban.com@evil.test/people/a/collect",
        ] {
            assert!(list_url(raw, "a").is_err());
        }
    }
    #[test]
    fn display_titles_only_split_explicit_chinese_and_foreign_aliases() {
        assert_eq!(
            display_title("肖申克的救赎 / The Shawshank Redemption"),
            "肖申克的救赎"
        );
        for name in ["Face / Off", "男/女", "南方 / 北方", "幸福"] {
            assert_eq!(display_title(name), name);
        }
    }
    #[test]
    fn interrupted_jobs_keep_the_last_successful_boundary() {
        let root = tempfile::tempdir().unwrap();
        let conn = store::open(root.path()).unwrap();
        let job = store::job_start(&conn, "douban-sync", 100).unwrap();
        store::set_setting(
            &conn,
            "doubanConnection",
            &json!({"connected":true,"phase":"syncing","accountKey":"a"}),
        )
        .unwrap();
        store::set_setting(&conn, &cursor_key("a"), &json!("42")).unwrap();
        recover_interrupted(&conn).unwrap();
        assert_eq!(
            store::setting(&conn, "doubanConnection").unwrap()["phase"],
            "interrupted"
        );
        let state: String = conn
            .query_row("SELECT state FROM jobs WHERE id=?1", [job], |r| r.get(0))
            .unwrap();
        assert_eq!(state, "interrupted");
        assert_eq!(store::setting(&conn, &cursor_key("a")).unwrap(), "42");
    }
}
