use crate::{
    achievement_platform, db,
    model::{Achievement, Game, UnlockEvent},
};
use rusqlite::{params, Connection};
use std::collections::BTreeMap;

pub(crate) fn initialize(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS daily_playtime (
         session_id TEXT NOT NULL, game_id TEXT NOT NULL, date TEXT NOT NULL,
         seconds INTEGER NOT NULL DEFAULT 0, PRIMARY KEY(session_id,date));
         CREATE INDEX IF NOT EXISTS daily_playtime_date ON daily_playtime(date);
         CREATE TABLE IF NOT EXISTS statistics_unlock_events (
         game_id TEXT NOT NULL, namespace TEXT NOT NULL, api_name TEXT NOT NULL,
         recorded_at TEXT NOT NULL, date TEXT NOT NULL,
         PRIMARY KEY(game_id,namespace,api_name));
         CREATE INDEX IF NOT EXISTS statistics_unlock_date ON statistics_unlock_events(date);",
    )
    .map_err(|e| e.to_string())
}

pub(crate) fn session_days(conn: &Connection, id: &str) -> Result<BTreeMap<String, u64>, String> {
    let mut stmt = conn
        .prepare("SELECT date,seconds FROM daily_playtime WHERE session_id=?1")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([id], |row| {
            Ok((row.get(0)?, row.get::<_, i64>(1)?.max(0) as u64))
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<_, _>>().map_err(|e| e.to_string())
}

pub(crate) fn save_days(
    conn: &Connection,
    session_id: &str,
    game_id: &str,
    days: &BTreeMap<String, u64>,
) -> Result<(), String> {
    for (date, seconds) in days {
        let seconds = i64::try_from(*seconds).map_err(|_| "每日时长超出范围")?;
        conn.execute(
            "INSERT INTO daily_playtime(session_id,game_id,date,seconds) VALUES(?1,?2,?3,?4)
             ON CONFLICT(session_id,date) DO UPDATE SET seconds=MAX(seconds,excluded.seconds)",
            params![session_id, game_id, date, seconds],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

// 只由新解锁通知路径调用；历史导入和测试弹层不会进入这里。
pub(crate) fn record_unlock(conn: &Connection, event: &UnlockEvent) -> Result<(), String> {
    let Some(game) = db::game(conn, &event.game_id)? else {
        return Ok(());
    };
    let namespace = achievement_platform::namespace(conn, &game)?;
    let now = chrono::Local::now();
    conn.execute(
        "INSERT OR IGNORE INTO statistics_unlock_events(game_id,namespace,api_name,recorded_at,date)
         VALUES(?1,?2,?3,?4,?5)",
        params![event.game_id, namespace, event.api_name, now.to_rfc3339(), now.date_naive().to_string()],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

// 官方接口不走本地通知路径。首次成功快照建立基线，后续只记录新增且发生于启用之后的解锁。
pub(crate) fn official_changes(
    conn: &Connection,
    game: &Game,
    previous: &[Achievement],
) -> Result<(), String> {
    let namespace = achievement_platform::namespace(conn, game)?;
    let key = format!("statistics_official_baseline:{}:{namespace}", game.id);
    let cached = ["Steam 成就已同步", "Epic 成就已同步"].contains(&game.scan_status.as_str());
    let ready = cached || db::setting(conn, &key)? == "true";
    let started =
        chrono::DateTime::parse_from_rfc3339(&db::setting(conn, "statistics_started_at")?).ok();
    if ready {
        for item in db::achievements(conn, &game.id)? {
            let fresh = !previous
                .iter()
                .any(|old| old.api_name == item.api_name && old.unlocked_at.is_some());
            let after_start = item
                .unlocked_at
                .as_deref()
                .and_then(|time| chrono::DateTime::parse_from_rfc3339(time).ok())
                .zip(started)
                .is_some_and(|(time, start)| time >= start);
            if fresh && after_start && item.unlock_source.as_deref() == Some(game.source.as_str()) {
                record_unlock(
                    conn,
                    &UnlockEvent {
                        game_id: game.id.clone(),
                        game_title: game.title.clone(),
                        api_name: item.api_name,
                        achievement_name: item.name,
                    },
                )?;
            }
        }
    }
    db::set_setting(conn, &key, "true")
}

pub(crate) fn save_steam_snapshot(
    conn: &mut Connection,
    game: &Game,
    items: &[(String, String)],
) -> Result<(), String> {
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    let current = db::game(&tx, &game.id)?.ok_or("游戏不存在")?;
    if current.source != "steam" || current.appid != game.appid {
        return Err("Steam 关联已变更，已丢弃旧解锁快照".into());
    }
    let previous = db::achievements(&tx, &game.id)?;
    db::replace_steam_unlocks(&tx, &game.id, items)?;
    official_changes(&tx, &current, &previous)?;
    tx.commit().map_err(|e| e.to_string())
}
