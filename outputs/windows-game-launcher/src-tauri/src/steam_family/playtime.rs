use super::{api, auth, catalog};
use crate::{db, lock_db, AppState};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::atomic::Ordering};
use tauri::{Emitter, Manager};

#[derive(Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Snapshot {
    pub checked_at: String,
    pub error: String,
    pub games: HashMap<String, Entry>,
    #[serde(skip)]
    pub connected: bool,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Entry {
    pub seconds: u64,
    pub last_played: String,
    pub checked_at: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ResultInfo {
    pub games: usize,
    pub shared_games: usize,
    pub shared_seconds: u64,
}

fn key(steam_id: &str) -> String { format!("steam_family_playtime:{steam_id}") }

pub(crate) fn read(conn: &Connection, steam_id: &str) -> Result<Snapshot, String> {
    let raw = db::setting(conn, &key(steam_id))?;
    if raw.is_empty() { return Ok(Snapshot::default()); }
    let mut snapshot: Snapshot = serde_json::from_str(&raw).map_err(|_| "Steam 本人时长缓存无法读取")?;
    let session: serde_json::Value = serde_json::from_str(&db::setting(conn, "steam_family_session")?).unwrap_or_default();
    snapshot.connected = session["steam_id"].as_str() == Some(steam_id)
        && session["expires_at"].as_i64().is_some_and(|time| time > chrono::Utc::now().timestamp() + 30);
    Ok(snapshot)
}

pub(crate) fn save(conn: &Connection, steam_id: &str, data: &catalog::Catalog) -> Result<ResultInfo, String> {
    let mut snapshot = read(conn, steam_id)?;
    let checked_at = chrono::Utc::now().to_rfc3339();
    let mut result = ResultInfo { games: 0, shared_games: 0, shared_seconds: 0 };
    for item in &data.items {
        let Some(seconds) = item.played_seconds else { continue };
        result.games += 1;
        if item.shared {
            result.shared_games += 1;
            result.shared_seconds = result.shared_seconds.checked_add(seconds).ok_or("Steam 时长数据超出有效范围")?;
        }
        snapshot.games.insert(item.appid.clone(), Entry {
            seconds, last_played: item.last_played.clone(), checked_at: checked_at.clone(),
        });
    }
    // 没返回的项目保留个人历史缓存，不把未知时长写成零。
    snapshot.checked_at = checked_at;
    snapshot.error.clear();
    write(conn, steam_id, &snapshot)?;
    Ok(result)
}

fn write(conn: &Connection, steam_id: &str, snapshot: &Snapshot) -> Result<(), String> {
    db::set_setting(conn, &key(steam_id), &serde_json::to_string(snapshot).map_err(|_| "无法保存 Steam 本人时长")?)
}

pub(crate) async fn refresh_if_connected(app: &tauri::AppHandle) -> Result<Option<ResultInfo>, String> {
    let status = auth::status(app)?;
    if !status.connected || !status.has_family {
        return Ok(None);
    }
    refresh(app).await.map(Some)
}

async fn refresh(app: &tauri::AppHandle) -> Result<ResultInfo, String> {
    let _guard = auth::AUTH_LOCK.lock().await;
    let session = auth::session(app)?;
    let generation = auth::LOGIN_GENERATION.load(Ordering::Acquire);
    let response = async {
        let family = api::family(&session.token, &session.steam_id).await?;
        let family_id = family.id.ok_or("当前账号没有加入 Steam 家庭")?;
        let data = api::library(&session.token, &session.steam_id, &family_id).await?;
        catalog::parse(&data, &session.steam_id)
    }.await;
    if auth::LOGIN_GENERATION.load(Ordering::Acquire) != generation {
        return Err("家庭库连接已变更，已丢弃旧时长结果".into());
    }
    let result = {
        let state = app.state::<AppState>();
        let conn = lock_db(&state)?;
        let current_id = db::setting(&conn, "steam_id")?;
        if !current_id.is_empty() && current_id != session.steam_id {
            return Err("Steam 账号已变更，已丢弃旧时长结果".into());
        }
        match response {
            Ok(data) => save(&conn, &session.steam_id, &data),
            Err(error) => {
                let mut snapshot = read(&conn, &session.steam_id)?;
                snapshot.error = error.clone();
                write(&conn, &session.steam_id, &snapshot)?;
                Err(error)
            }
        }
    };
    let _ = app.emit("library-changed", ());
    result
}
