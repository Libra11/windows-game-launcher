use crate::{db, lock_db, model::Game, steam, AppState};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
use tauri::{Emitter, Manager};

#[derive(Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Snapshot {
    pub checked_at: String,
    pub error: String,
    pub games: HashMap<String, Entry>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Entry {
    pub seconds: Option<u64>,
    pub last_played: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Info {
    pub source: String,
    pub seconds: Option<u64>,
    pub last_played: String,
    pub checked_at: String,
    pub state: String,
    pub reason: String,
}

fn cache_key(steamid: &str) -> String {
    format!("steam_playtime:{steamid}")
}

pub(crate) fn read(conn: &Connection, steamid: &str) -> Result<Snapshot, String> {
    let text = db::setting(conn, &cache_key(steamid))?;
    if text.is_empty() {
        return Ok(Snapshot::default());
    }
    serde_json::from_str(&text).map_err(|_| "Steam 时长缓存无法读取".into())
}

pub(crate) fn save_owned(conn: &Connection, steamid: &str, games: &[Game]) -> Result<(), String> {
    let mut entries = HashMap::new();
    for game in games {
        let meta: serde_json::Value =
            serde_json::from_str(&game.metadata_json).map_err(|_| "Steam 时长数据格式无效")?;
        entries.insert(
            game.appid.clone(),
            Entry {
                seconds: meta
                    .get("steamPlayedSeconds")
                    .and_then(serde_json::Value::as_u64),
                last_played: meta
                    .get("steamLastPlayed")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("")
                    .into(),
            },
        );
    }
    let snapshot = Snapshot {
        checked_at: chrono::Utc::now().to_rfc3339(),
        error: String::new(),
        games: entries,
    };
    db::set_setting(
        conn,
        &cache_key(steamid),
        &serde_json::to_string(&snapshot).map_err(|error| error.to_string())?,
    )
}

pub(crate) fn info(game: &Game, local_seconds: u64, snapshot: &Snapshot, connected: bool) -> Info {
    if game.source != "steam" {
        return Info {
            source: game.source.clone(),
            seconds: Some(local_seconds),
            last_played: String::new(),
            checked_at: String::new(),
            state: "ready".into(),
            reason: "从启动器启动后，按实际游戏进程自动记录".into(),
        };
    }
    let entry = snapshot.games.get(&game.appid);
    let seconds = entry.and_then(|entry| entry.seconds);
    let reason = if !connected {
        "请在设置中填写 Steam API Key 和 SteamID64".into()
    } else if !snapshot.error.is_empty() {
        format!("同步失败：{}", snapshot.error)
    } else if snapshot.checked_at.is_empty() {
        "等待从 Steam 同步累计时长".into()
    } else if seconds.is_none() {
        "Steam 未返回此游戏的时长，请检查账号游戏详情和游玩时长可见性".into()
    } else {
        "Steam 官方累计时长，可能延迟更新".into()
    };
    let state = if seconds.is_some() {
        if connected && snapshot.error.is_empty() {
            "ready"
        } else {
            "cached"
        }
    } else if connected && snapshot.checked_at.is_empty() && snapshot.error.is_empty() {
        "pending"
    } else {
        "unavailable"
    };
    Info {
        source: "steam".into(),
        seconds,
        last_played: entry
            .map(|entry| entry.last_played.clone())
            .unwrap_or_default(),
        checked_at: snapshot.checked_at.clone(),
        state: state.into(),
        reason,
    }
}

static REFRESHING: AtomicBool = AtomicBool::new(false);
struct RefreshGuard;
impl Drop for RefreshGuard {
    fn drop(&mut self) {
        REFRESHING.store(false, Ordering::Release);
    }
}

pub(crate) async fn refresh(app: &tauri::AppHandle) -> Result<(), String> {
    if REFRESHING.swap(true, Ordering::AcqRel) {
        return Ok(());
    }
    let _guard = RefreshGuard;
    let (key, steamid) = {
        let state = app.state::<AppState>();
        let conn = lock_db(&state)?;
        (
            db::setting(&conn, "steam_api_key")?,
            db::setting(&conn, "steam_id")?,
        )
    };
    if key.is_empty() || steamid.is_empty() {
        return Ok(());
    }
    let result = steam::owned_games(&key, &steamid).await;
    {
        let state = app.state::<AppState>();
        let conn = lock_db(&state)?;
        // 账号切换后丢弃旧请求，避免将其他账号的时长显示为当前账号的数据。
        if db::setting(&conn, "steam_id")? != steamid {
            return Ok(());
        }
        match &result {
            Ok(games) => save_owned(&conn, &steamid, games)?,
            Err(error) => {
                let mut snapshot = read(&conn, &steamid)?;
                snapshot.error = error.clone();
                db::set_setting(
                    &conn,
                    &cache_key(&steamid),
                    &serde_json::to_string(&snapshot).map_err(|error| error.to_string())?,
                )?;
            }
        }
    }
    let _ = app.emit("library-changed", ());
    result.map(|_| ())
}

pub(crate) fn start_refresh(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        let _ = refresh(&app).await;
    });
}

pub(crate) fn watch(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            let _ = refresh(&app).await;
            tokio::time::sleep(Duration::from_secs(300)).await;
        }
    });
}
