use crate::{db, model::Game};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::collections::HashMap;

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Activity {
    pub favorite: bool,
    pub last_played: String,
    pub played_seconds: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryGame {
    #[serde(flatten)]
    pub game: Game,
    #[serde(flatten)]
    pub activity: Activity,
    pub runtime: crate::runtime::RuntimeInfo,
    pub installation: Option<crate::installation::Info>,
    pub playtime: crate::steam_playtime::Info,
    pub achievement_platform: crate::achievement_platform::Profile,
}

pub fn initialize(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS game_activity (
         game_id TEXT PRIMARY KEY, favorite INTEGER NOT NULL DEFAULT 0,
         last_played TEXT NOT NULL DEFAULT '', played_seconds INTEGER NOT NULL DEFAULT 0);
         CREATE TABLE IF NOT EXISTS play_sessions (
         id TEXT PRIMARY KEY, game_id TEXT NOT NULL, started_at TEXT NOT NULL,
         ended_at TEXT NOT NULL DEFAULT '', seconds INTEGER NOT NULL DEFAULT 0);",
    )
    .map_err(|e| e.to_string())
}

pub fn all(conn: &Connection) -> Result<HashMap<String, Activity>, String> {
    let mut stmt = conn
        .prepare("SELECT game_id,favorite,last_played,played_seconds FROM game_activity")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get(0)?,
                Activity {
                    favorite: row.get::<_, i32>(1)? != 0,
                    last_played: row.get(2)?,
                    played_seconds: row.get::<_, i64>(3)?.max(0) as u64,
                },
            ))
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<_, _>>().map_err(|e| e.to_string())
}

pub fn set_favorite(conn: &Connection, game_id: &str, value: bool) -> Result<(), String> {
    if db::game(conn, game_id)?.is_none() {
        return Err("游戏不存在".into());
    }
    conn.execute(
        "INSERT INTO game_activity(game_id,favorite) VALUES(?1,?2)
        ON CONFLICT(game_id) DO UPDATE SET favorite=excluded.favorite",
        params![game_id, value],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn start(
    conn: &mut Connection,
    session_id: &str,
    game_id: &str,
    time: &str,
) -> Result<(), String> {
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    if db::game(&tx, game_id)?.is_none() {
        return Err("游戏不存在".into());
    }
    let inserted = tx
        .execute(
            "INSERT OR IGNORE INTO play_sessions(id,game_id,started_at) VALUES(?1,?2,?3)",
            params![session_id, game_id, time],
        )
        .map_err(|e| e.to_string())?;
    if inserted > 0 {
        tx.execute(
            "INSERT INTO game_activity(game_id,last_played) VALUES(?1,?2)
            ON CONFLICT(game_id) DO UPDATE SET last_played=excluded.last_played",
            params![game_id, time],
        )
        .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())
}

// 保存累计秒数而不是增量，同一检查点重复提交不会重复计时。
pub fn checkpoint(
    conn: &mut Connection,
    session_id: &str,
    seconds: u64,
    finished: bool,
) -> Result<(), String> {
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    let seconds = i64::try_from(seconds).map_err(|_| "游玩时长超出范围")?;
    let session: Option<(String, i64)> = tx
        .query_row(
            "SELECT game_id,seconds FROM play_sessions WHERE id=?1",
            [session_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    let Some((game_id, previous)) = session else {
        return Err("游玩记录不存在".into());
    };
    let total = seconds.max(previous);
    let ended_at = if finished {
        chrono::Utc::now().to_rfc3339()
    } else {
        String::new()
    };
    tx.execute("UPDATE play_sessions SET seconds=?1,ended_at=CASE WHEN ?2<>'' THEN ?2 ELSE ended_at END WHERE id=?3", params![total, ended_at, session_id]).map_err(|e| e.to_string())?;
    tx.execute(
        "UPDATE game_activity SET played_seconds=played_seconds+?1 WHERE game_id=?2",
        params![total - previous, game_id],
    )
    .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn playtime_is_idempotent_and_favorites_survive_sessions() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE games(id TEXT PRIMARY KEY,source TEXT,appid TEXT,title TEXT,exe_path TEXT,launch_uri TEXT,custom_unlock_path TEXT,metadata_json TEXT,scan_status TEXT,source_file TEXT,last_scan TEXT,schema_source TEXT);
            INSERT INTO games VALUES('g','local','','Game','','','','{}','','','','');").unwrap();
        initialize(&conn).unwrap();
        set_favorite(&conn, "g", true).unwrap();
        start(&mut conn, "s1", "g", "2026-09-30T00:00:00Z").unwrap();
        start(&mut conn, "s1", "g", "2026-09-30T01:00:00Z").unwrap();
        checkpoint(&mut conn, "s1", 30, false).unwrap();
        checkpoint(&mut conn, "s1", 30, false).unwrap();
        checkpoint(&mut conn, "s1", 42, true).unwrap();
        checkpoint(&mut conn, "s1", 40, true).unwrap();
        let activity = all(&conn).unwrap().remove("g").unwrap();
        assert!(activity.favorite);
        assert_eq!(activity.played_seconds, 42);
        assert_eq!(activity.last_played, "2026-09-30T00:00:00Z");
        assert!(set_favorite(&conn, "missing", true).is_err());
    }
}
