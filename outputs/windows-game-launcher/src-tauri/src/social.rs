//! Friend traffic never serializes local Game or platform credentials.
use crate::{db, lock_db, AppState};
use rusqlite::OptionalExtension;
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex,
};
use tauri::{Emitter, Manager};
pub struct Social {
    wake: tokio::sync::Notify,
    status: Mutex<Value>,
    stopping: AtomicBool,
}
impl Default for Social {
    fn default() -> Self {
        Self {
            wake: Default::default(),
            status: Mutex::new(json!({"connected":false})),
            stopping: AtomicBool::new(false),
        }
    }
}
pub fn changed(app: &tauri::AppHandle) {
    if let Some(s) = app.try_state::<Social>() {
        s.wake.notify_one();
    }
}
fn identity(app: &tauri::AppHandle) -> Result<String, String> {
    let state = app.state::<AppState>();
    let c = lock_db(&state)?;
    let user = db::setting(&c, "cloud_login_user")?;
    if user.is_empty() {
        return Err("请先登录游迹云账号".into());
    }
    Ok(format!("{}|{}", db::setting(&c, "cloud_base_url")?, user))
}
fn game_key(c: &rusqlite::Connection, g: &crate::model::Game) -> String {
    let existing:Option<String>=c.query_row("SELECT s.resource FROM sync_game_links l JOIN sync_shadow s ON s.kind='game' AND s.id=l.cloud_id WHERE l.local_id=?1",[&g.id],|r|r.get(0)).optional().ok().flatten();
    if let Some(raw) = existing {
        if let Ok(v) = serde_json::from_str::<Value>(&raw) {
            if let (Some(source), Some(id)) = (
                v["payload"]["source"].as_str(),
                v["payload"]["sourceIdentity"].as_str(),
            ) {
                return format!("{source}:{id}");
            }
        }
    }
    let id = match g.source.as_str() {
        "steam" => g.appid.clone(),
        "epic" => {
            let m: Value = serde_json::from_str(&g.metadata_json).unwrap_or_default();
            format!(
                "{}:{}",
                m["epicNamespace"].as_str().unwrap_or(""),
                m["epicAppName"].as_str().unwrap_or("")
            )
        }
        _ => g.id.clone(),
    };
    format!("{}:{id}", g.source)
}
fn games(app: &tauri::AppHandle) -> Result<Value, String> {
    let state = app.state::<AppState>();
    let sessions = state
        .runtime
        .lock()
        .map_err(|_| "运行状态不可用")?
        .social_sessions();
    let c = lock_db(&state)?;
    Ok(json!(sessions
        .into_iter()
        .map(|(g, s)| json!({"key":game_key(&c,&g),"title":g.title,"sessionId":s}))
        .collect::<Vec<_>>()))
}
pub(crate) fn notifications(app: &tauri::AppHandle) -> bool {
    let state = app.state::<AppState>();
    lock_db(&state).is_ok_and(|c| {
        db::setting(&c, "social_notifications").unwrap_or_default() != "false"
            && db::setting(&c, "social_dnd").unwrap_or_default() != "true"
    })
}
mod artwork;
pub fn start(app: tauri::AppHandle) {
    artwork::start(app.clone());
    tauri::async_runtime::spawn(async move {
        let mut cursor = None;
        let mut account = String::new();
        let mut last_games = Value::Null;
        let mut heartbeat = std::time::Instant::now() - std::time::Duration::from_secs(60);
        let mut failures: usize = 0;
        let mut last_poll = std::time::Instant::now();
        loop {
            let social = app.state::<Social>();
            tokio::select! {_=tokio::time::sleep(std::time::Duration::from_secs(if failures==0{5}else{[5,15,60,300][(failures-1).min(3)]}))=>{},_=social.wake.notified()=>{}}
            if social.stopping.load(Ordering::Acquire) {
                break;
            }
            if last_poll.elapsed().as_secs() > 20 {
                cursor = None;
            }
            let context = identity(&app);
            if context.as_ref().ok() != Some(&account) {
                cursor = None;
                last_games = Value::Null;
                failures = 0;
                account = context.clone().unwrap_or_default();
            }
            if let Err(e) = context {
                if let Ok(mut s) = social.status.lock() {
                    *s = json!({"connected":false,"error":e});
                }
                continue;
            }
            let result: Result<(), String> = async {
                let current = games(&app)?;
                if current != last_games || heartbeat.elapsed().as_secs() >= 30 || cursor.is_none()
                {
                    crate::cloud::request(
                        &app,
                        "POST",
                        "/v1/social/presence",
                        Some(json!({"games":current,"announce":cursor.is_some()})),
                    )
                    .await?;
                    last_games = current;
                    heartbeat = std::time::Instant::now();
                }
                let path = cursor
                    .map(|c: i64| format!("/v1/social/events?cursor={c}"))
                    .unwrap_or("/v1/social/events".into());
                let events = crate::cloud::request(&app, "GET", &path, None).await?;
                last_poll = std::time::Instant::now();
                let was_connected = cursor.is_some();
                cursor = events["cursor"].as_i64();
                // Account may have changed while the network request was in flight.
                if identity(&app).ok().as_ref() != Some(&account) {
                    cursor = None;
                    return Ok(());
                }
                if was_connected && events["reset"] != true && notifications(&app) {
                    let items = events["events"].as_array().cloned().unwrap_or_default();
                    if !items.is_empty() {
                        let first = &items[0];
                        let message = if items.len() > 1 {
                            format!(
                                "{} 等 {} 位好友开始游戏",
                                first["nickname"].as_str().unwrap_or("好友"),
                                items.len()
                            )
                        } else {
                            format!(
                                "{} 开始玩游戏",
                                first["nickname"].as_str().unwrap_or("好友")
                            )
                        };
                        let _ = crate::achievement_overlay::show_social(
                            &app,
                            first["actor"].as_str().unwrap_or("").into(),
                            first["key"].as_str().unwrap_or("").into(),
                            message,
                            first["title"].as_str().unwrap_or("游戏").into(),
                        );
                    }
                }
                if let Ok(mut s) = social.status.lock() {
                    *s = json!({"connected":true,"checkedAt":chrono::Utc::now().to_rfc3339()});
                }
                let _ = app.emit("social-changed", ());
                Ok(())
            }
            .await;
            if let Err(e) = result {
                cursor = None;
                failures = (failures + 1).min(4);
                if let Ok(mut s) = social.status.lock() {
                    *s = json!({"connected":false,"error":e});
                }
            } else {
                failures = 0;
            }
        }
    });
}
// Bounded best effort; TTL handles an unreachable network or forced process termination.
pub fn leaving(app: &tauri::AppHandle) {
    if let Some(s) = app.try_state::<Social>() {
        s.stopping.store(true, Ordering::Release);
        s.wake.notify_one();
    }
    if identity(app).is_ok() {
        let app = app.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        tauri::async_runtime::spawn(async move {
            let _ = tokio::time::timeout(
                std::time::Duration::from_secs(2),
                crate::cloud::request(
                    &app,
                    "POST",
                    "/v1/social/presence",
                    Some(json!({"games":[],"offline":true})),
                ),
            )
            .await;
            let _ = tx.send(());
        });
        let _ = rx.recv_timeout(std::time::Duration::from_secs(2));
    }
}
#[tauri::command]
pub async fn social_call(
    app: tauri::AppHandle,
    action: String,
    id: Option<String>,
    body: Option<Value>,
    offset: Option<u32>,
    game: Option<String>,
) -> Result<Value, String> {
    let uid = || {
        id.as_deref()
            .ok_or("缺少用户标识")
            .and_then(|s| uuid::Uuid::parse_str(s).map_err(|_| "用户标识无效"))
    };
    let (method, path) = match action.as_str() {
        "me" => ("GET", "/v1/social/me".into()),
        "edit" => ("POST", "/v1/social/me".into()),
        "friends" => ("GET", "/v1/social/friends".into()),
        "request" => ("POST", "/v1/social/requests".into()),
        "relation" => ("POST", format!("/v1/social/friends/{}", uid()?)),
        "cover" => {
            let mut u=reqwest::Url::parse(&format!("https://social.invalid/v1/social/covers/{}",uid()?)).map_err(|e|e.to_string())?;
            u.query_pairs_mut().append_pair("key",game.as_deref().ok_or("缺少游戏标识")?);
            ("GET",format!("{}?{}",u.path(),u.query().unwrap_or("")))
        }
        "space" => {
            let mut path = format!(
                "/v1/social/spaces/{}?offset={}",
                uid()?,
                offset.unwrap_or(0)
            );
            if let Some(g) = game {
                let mut u = reqwest::Url::parse(&format!("https://social.invalid{path}"))
                    .map_err(|e| e.to_string())?;
                u.query_pairs_mut().append_pair("game", &g);
                path = format!("{}?{}", u.path(), u.query().unwrap_or(""));
            }
            ("GET", path)
        }
        "avatar" => ("GET", format!("/v1/social/avatar/{}", uid()?)),
        "uploadAvatar" => ("POST", "/v1/social/avatar".into()),
        _ => return Err("不支持的社交操作".into()),
    };
    let result = crate::cloud::request(&app, method, &path, body).await?;
    if method != "GET" { changed(&app); }
    Ok(result)
}
#[tauri::command]
pub fn social_local(app: tauri::AppHandle, settings: Option<Value>, include_games: Option<bool>) -> Result<Value, String> {
    let state = app.state::<AppState>();
    let c = lock_db(&state)?;
    if let Some(v) = settings {
        for (key, field) in [
            ("social_notifications", "notifications"),
            ("social_dnd", "dnd"),
        ] {
            if let Some(b) = v[field].as_bool() {
                db::set_setting(&c, key, if b { "true" } else { "false" })?;
            }
        }
    }
    let candidates = if include_games.unwrap_or(true) { db::games(&c)?
        .iter()
        .map(|g| json!({"key":game_key(&c,g),"title":g.title,"id":g.id}))
        .collect::<Vec<_>>() } else { Vec::new() };
    Ok(
        json!({"status":app.state::<Social>().status.lock().map_err(|_|"状态不可用")?.clone(),"notifications":db::setting(&c,"social_notifications")?!="false","dnd":db::setting(&c,"social_dnd")?=="true","games":candidates}),
    )
}

pub(crate) async fn can_notify(app: &tauri::AppHandle, actor: &str, key: &str) -> bool {
    if !notifications(app) || uuid::Uuid::parse_str(actor).is_err() {
        return false;
    }
    let Ok(v) = crate::cloud::request(app, "GET", "/v1/social/friends", None).await else {
        return false;
    };
    v["friends"].as_array().is_some_and(|items| {
        items.iter().any(|f| {
            f["id"] == actor
                && f["relation"] == "accepted"
                && f["muted"] != true
                && f["presence"]["games"]
                    .as_array()
                    .is_some_and(|games| games.iter().any(|g| g["key"] == key))
        })
    })
}

#[tauri::command]
pub fn social_test_notice(app:tauri::AppHandle)->Result<(),String>{
 if !notifications(&app){return Err("请先开启好友通知并关闭免打扰".into());}
 crate::achievement_overlay::show_social(&app,String::new(),String::new(),"示例好友开始玩游戏".into(),"游迹 · 通知预览".into())
}
