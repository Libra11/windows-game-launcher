use super::api;
use crate::{db, lock_db, AppState};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::atomic::{AtomicU64, Ordering};
use tauri::Manager;

const SESSION_KEY: &str = "steam_family_session";
pub(crate) static AUTH_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
pub(crate) static LOGIN_GENERATION: AtomicU64 = AtomicU64::new(0);

#[derive(Serialize, Deserialize)]
pub(crate) struct Session {
    pub token: String,
    pub steam_id: String,
    pub expires_at: i64,
    pub family_name: String,
    pub family_id: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Status {
    pub connected: bool,
    pub expired: bool,
    pub steam_id: String,
    pub family_name: String,
    pub has_family: bool,
    pub account_mismatch: bool,
}

fn parse_token(token: String) -> Result<Session, String> {
    if token.len() > 16_384 || token.split('.').count() != 3 { return Err("Steam 登录凭证格式无效".into()); }
    let payload = token.split('.').nth(1).ok_or("Steam 登录凭证格式无效")?;
    let bytes = URL_SAFE_NO_PAD.decode(payload.trim_end_matches('=')).map_err(|_| "Steam 登录凭证格式无效")?;
    let data: Value = serde_json::from_slice(&bytes).map_err(|_| "Steam 登录凭证格式无效")?;
    let steam_id = data.get("sub").and_then(api::number).filter(|id| *id > 0)
        .ok_or("Steam 登录凭证没有账号身份")?.to_string();
    let expires_at = data.get("exp").and_then(api::number).and_then(|value| i64::try_from(value).ok())
        .filter(|value| *value > chrono::Utc::now().timestamp() + 60)
        .ok_or("Steam 登录凭证已过期，请重新登录")?;
    Ok(Session { token, steam_id, expires_at, family_name: String::new(), family_id: None })
}

fn read(app: &tauri::AppHandle) -> Result<Option<Session>, String> {
    let raw = db::setting(&*lock_db(&app.state::<AppState>())?, SESSION_KEY)?;
    if raw.is_empty() { return Ok(None); }
    serde_json::from_str(&raw).map(Some).map_err(|_| "Steam 家庭库授权无法读取，请重新登录".into())
}

pub(crate) fn session(app: &tauri::AppHandle) -> Result<Session, String> {
    let current = read(app)?.ok_or("请先在设置中登录 Steam 家庭库")?;
    if current.expires_at <= chrono::Utc::now().timestamp() + 30 { return Err("Steam 家庭库授权已过期，请重新登录".into()); }
    let configured = db::setting(&*lock_db(&app.state::<AppState>())?, "steam_id")?;
    if !configured.is_empty() && configured != current.steam_id {
        return Err("Steam 家庭库登录账号与 SteamID64 不一致，请重新登录对应账号".into());
    }
    Ok(current)
}

pub(crate) async fn complete(app: &tauri::AppHandle, token: String, generation: u64) -> Result<(), String> {
    let _guard = AUTH_LOCK.lock().await;
    if LOGIN_GENERATION.load(Ordering::Acquire) != generation { return Err("Steam 登录已取消".into()); }
    let mut current = parse_token(token)?;
    let family = api::family(&current.token, &current.steam_id).await?;
    current.family_name = family.name;
    current.family_id = family.id;
    if LOGIN_GENERATION.load(Ordering::Acquire) != generation { return Err("Steam 登录已取消".into()); }
    let state = app.state::<AppState>();
    let conn = lock_db(&state)?;
    let configured = db::setting(&conn, "steam_id")?;
    if !configured.is_empty() && configured != current.steam_id {
        return Err("登录账号与已填写的 SteamID64 不一致，请登录同一账号".into());
    }
    // JWT 内容只用于提取身份；上面的 Steam 授权接口验证成功后才保存。
    save(&conn, &current)
}

pub(crate) fn save(conn: &rusqlite::Connection, session: &Session) -> Result<(), String> {
    db::set_setting(conn, SESSION_KEY, &serde_json::to_string(session).map_err(|_| "无法保存 Steam 授权")?)
}

pub(crate) fn status(app: &tauri::AppHandle) -> Result<Status, String> {
    let current = read(app)?;
    let configured = db::setting(&*lock_db(&app.state::<AppState>())?, "steam_id")?;
    let expired = current.as_ref().is_some_and(|session| session.expires_at <= chrono::Utc::now().timestamp() + 30);
    let connected = current.as_ref().is_some_and(|session| !expired && (configured.is_empty() || configured == session.steam_id));
    let account_mismatch = current.as_ref().is_some_and(|session| !configured.is_empty() && configured != session.steam_id);
    Ok(Status {
        connected, expired,
        steam_id: current.as_ref().map(|session| session.steam_id.clone()).unwrap_or_default(),
        family_name: current.as_ref().map(|session| session.family_name.clone()).unwrap_or_default(),
        has_family: current.is_some_and(|session| session.family_id.is_some()),
        account_mismatch,
    })
}

pub(crate) async fn disconnect(app: &tauri::AppHandle) -> Result<(), String> {
    LOGIN_GENERATION.fetch_add(1, Ordering::AcqRel);
    let _guard = AUTH_LOCK.lock().await;
    db::set_setting(&*lock_db(&app.state::<AppState>())?, SESSION_KEY, "")?;
    if let Some(window) = app.get_webview_window(super::login::WINDOW_LABEL) {
        let _ = window.close();
    }
    Ok(())
}
