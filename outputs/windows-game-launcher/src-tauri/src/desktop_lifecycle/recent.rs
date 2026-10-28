//! 托盘只查询最近记录的轻量摘要，启动／播放复用原入口和运行状态校验。
use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use serde_json::json;
use std::{
    path::Path,
    sync::{mpsc, Mutex},
    time::Duration,
};
use tauri::{Emitter, Listener, Manager};

#[derive(Clone, Debug, PartialEq, Eq)]
enum Action {
    Game(String),
    Media { item: String, resource: String },
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Entry {
    title: String,
    action: Action,
}
impl Entry {
    fn key(&self) -> String {
        match &self.action {
            Action::Game(id) => format!("recent-game:{id}"),
            Action::Media { item, resource } => format!("recent-media:{}", json!([item, resource])),
        }
    }
}
#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub(super) struct Entries {
    games: Vec<Entry>,
    media: Vec<Entry>,
}
#[derive(Default)]
pub(super) struct State(Mutex<Entries>);

#[derive(Clone, Serialize)]
pub(crate) struct Item {
    key: String,
    title: String,
}
#[derive(Clone, Serialize)]
pub(crate) struct Snapshot {
    games: Vec<Item>,
    media: Vec<Item>,
}
impl Snapshot {
    // 与 tray-menu.css 的固定行高保持一致，窗口可随记录数量精确收缩。
    pub(super) fn height(&self) -> f64 {
        let rows = self.games.len().max(1) + self.media.len().max(1);
        // 2px 余量吸收 Windows 分数缩放的尺寸舍入，避免无谓的滚动条。
        190.0 + rows as f64 * 38.0
    }
}
impl From<&Entries> for Snapshot {
    fn from(entries: &Entries) -> Self {
        let items = |rows: &Vec<Entry>| {
            rows.iter()
                .map(|entry| Item {
                    key: entry.key(),
                    title: entry.title.clone(),
                })
                .collect()
        };
        Self {
            games: items(&entries.games),
            media: items(&entries.media),
        }
    }
}
pub(super) fn snapshot(app: &tauri::AppHandle) -> Snapshot {
    app.state::<State>()
        .0
        .lock()
        .map(|entries| Snapshot::from(&*entries))
        .unwrap_or_else(|_| Snapshot::from(&Entries::default()))
}

fn games(conn: &Connection) -> Result<Vec<Entry>, String> {
    let mut query = conn.prepare("SELECT g.id,g.title FROM games g JOIN game_activity a ON a.game_id=g.id WHERE julianday(a.last_played) IS NOT NULL AND NOT EXISTS(SELECT 1 FROM cloud_archives c WHERE c.game_id=g.id AND c.archived=1) ORDER BY julianday(a.last_played) DESC,g.id LIMIT 5").map_err(|e| e.to_string())?;
    let rows = query
        .query_map([], |row| {
            Ok(Entry {
                title: row.get(1)?,
                action: Action::Game(row.get(0)?),
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<_, _>>().map_err(|e| e.to_string())
}

fn media(conn: &Connection) -> Result<Vec<Entry>, String> {
    let mut query = conn.prepare(
        "WITH candidates AS (
         SELECT m.id item_id,r.id resource_id,
           CASE WHEN m.kind='episode' AND p.kind IN('season','series') THEN p.id ELSE m.id END group_id,
           CASE WHEN m.kind='episode' AND p.kind='season' THEN CASE WHEN show.title IS NOT NULL THEN show.title||' · '||p.title ELSE p.title END
                WHEN m.kind='episode' AND p.kind='series' THEN p.title
                WHEN m.kind='season' AND p.kind='series' THEN p.title||' · '||m.title ELSE m.title END title,
           julianday(v.started_at) played_at,v.id session_id
         FROM view_sessions v JOIN media_items m ON m.id=v.item_id
         JOIN resources r ON r.id=v.resource_id JOIN sources src ON src.id=r.source_id
         LEFT JOIN media_items p ON p.id=m.parent_id LEFT JOIN media_items show ON show.id=p.parent_id
         WHERE r.available=1 AND r.missing=0 AND src.available=1 AND v.seconds>0
           AND julianday(v.started_at) IS NOT NULL
           AND EXISTS(SELECT 1 FROM resource_links l WHERE l.item_id=m.id AND l.resource_id=r.id)
        ), ranked AS (
         SELECT *,ROW_NUMBER() OVER(PARTITION BY group_id ORDER BY played_at DESC,session_id DESC) rank FROM candidates
        ) SELECT item_id,resource_id,title FROM ranked WHERE rank=1 ORDER BY played_at DESC,group_id LIMIT 5"
    ).map_err(|e| e.to_string())?;
    let rows = query
        .query_map([], |row| {
            Ok(Entry {
                title: row.get(2)?,
                action: Action::Media {
                    item: row.get(0)?,
                    resource: row.get(1)?,
                },
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<_, _>>().map_err(|e| e.to_string())
}

pub(super) fn activate(app: &tauri::AppHandle, key: &str) {
    let action = app.state::<State>().0.lock().ok().and_then(|entries| {
        entries
            .games
            .iter()
            .chain(&entries.media)
            .find(|entry| entry.key() == key)
            .map(|entry| entry.action.clone())
    });
    let Some(action) = action else {
        return;
    };
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let result = match action {
            Action::Game(id) => crate::runtime_commands::launch_game(app.clone(), id)
                .await
                .map(|_| ()),
            Action::Media { item, resource } => match crate::media::check_maintenance(&app) {
                Ok(()) => crate::media::playback::play(
                    app.clone(),
                    json!({"itemId":item,"resourceId":resource}),
                )
                .await
                .map(|_| ()),
                Err(error) => Err(error),
            },
        };
        if let Err(error) = result {
            super::show(&app);
            let _ = app.emit("launcher-error", error);
        }
    });
}

fn read_only(path: &Path) -> Result<Connection, String> {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| e.to_string())?;
    conn.busy_timeout(Duration::from_millis(250))
        .map_err(|e| e.to_string())?;
    Ok(conn)
}
pub(super) fn watch(app: tauri::AppHandle) {
    let (tx, rx) = mpsc::sync_channel(1);
    for event in ["library-changed", "media-changed"] {
        let tx = tx.clone();
        app.listen(event, move |_| {
            let _ = tx.try_send(());
        });
    }
    std::thread::spawn(move || {
        loop {
            let result = (|| -> Result<(), String> {
                let state = app.state::<crate::AppState>();
                if state.maintenance.load(std::sync::atomic::Ordering::Acquire) {
                    return Ok(());
                }
                crate::media::check_maintenance(&app)?;
                let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
                let entries = Entries {
                    games: games(&read_only(&dir.join("games.sqlite"))?)?,
                    media: media(&read_only(&dir.join("media.sqlite"))?)?,
                };
                let state = app.state::<State>();
                if state
                    .0
                    .lock()
                    .map_err(|_| "托盘菜单状态不可用")?
                    .eq(&entries)
                {
                    return Ok(());
                }
                *state.0.lock().map_err(|_| "托盘菜单状态不可用")? = entries;
                drop(state);
                super::popup::update(&app);
                Ok(())
            })();
            // 读库失败保留上次菜单；点击仍由原入口重新校验。
            let _ = result;
            let _ = rx.recv_timeout(Duration::from_secs(3));
            std::thread::sleep(Duration::from_millis(200));
            while rx.try_recv().is_ok() {}
        }
    });
}

#[cfg(test)]
mod tests;
