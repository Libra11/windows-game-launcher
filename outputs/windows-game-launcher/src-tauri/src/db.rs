use crate::model::{Achievement, AchievementDefinition, Game};
use rusqlite::{params, Connection, OptionalExtension};
use std::{collections::HashSet, path::Path};

pub fn open(path: &Path) -> Result<Connection, String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let conn = Connection::open(path).map_err(|e| e.to_string())?;
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         CREATE TABLE IF NOT EXISTS games (
           id TEXT PRIMARY KEY, source TEXT NOT NULL, appid TEXT NOT NULL DEFAULT '',
           title TEXT NOT NULL, exe_path TEXT NOT NULL DEFAULT '', launch_uri TEXT NOT NULL DEFAULT '',
           custom_unlock_path TEXT NOT NULL DEFAULT '', metadata_json TEXT NOT NULL DEFAULT '{}',
           scan_status TEXT NOT NULL DEFAULT '', source_file TEXT NOT NULL DEFAULT '',
           last_scan TEXT NOT NULL DEFAULT '', schema_source TEXT NOT NULL DEFAULT ''
         );
         CREATE TABLE IF NOT EXISTS achievements (
           appid TEXT NOT NULL, api_name TEXT NOT NULL, name TEXT NOT NULL,
           description TEXT NOT NULL DEFAULT '', icon TEXT NOT NULL DEFAULT '',
           hidden INTEGER NOT NULL DEFAULT 0, PRIMARY KEY(appid, api_name)
         );
         CREATE TABLE IF NOT EXISTS unlocks (
           game_id TEXT NOT NULL, api_name TEXT NOT NULL, source TEXT NOT NULL,
           unlocked_at TEXT NOT NULL, evidence TEXT NOT NULL DEFAULT '',
           PRIMARY KEY(game_id, api_name)
         );
         CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);",
    ).map_err(|e| e.to_string())?;
    crate::activity::initialize(&conn)?;
    if setting(&conn, "statistics_started_at")?.is_empty() {
        set_setting(&conn, "statistics_started_at", &chrono::Local::now().to_rfc3339())?;
    }
    Ok(conn)
}

fn game_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Game> {
    Ok(Game {
        id: row.get(0)?,
        source: row.get(1)?,
        appid: row.get(2)?,
        title: row.get(3)?,
        exe_path: row.get(4)?,
        launch_uri: row.get(5)?,
        custom_unlock_path: row.get(6)?,
        metadata_json: row.get(7)?,
        scan_status: row.get(8)?,
        source_file: row.get(9)?,
        last_scan: row.get(10)?,
        schema_source: row.get(11)?,
    })
}

const GAME_COLUMNS: &str = "id,source,appid,title,exe_path,launch_uri,custom_unlock_path,metadata_json,scan_status,source_file,last_scan,schema_source";

pub fn games(conn: &Connection) -> Result<Vec<Game>, String> {
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {GAME_COLUMNS} FROM games ORDER BY title COLLATE NOCASE"
        ))
        .map_err(|e| e.to_string())?;
    let result = stmt
        .query_map([], game_from_row)
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string());
    result
}

pub fn game(conn: &Connection, id: &str) -> Result<Option<Game>, String> {
    conn.query_row(
        &format!("SELECT {GAME_COLUMNS} FROM games WHERE id=?1"),
        [id],
        game_from_row,
    )
    .optional()
    .map_err(|e| e.to_string())
}

pub fn upsert_game(conn: &Connection, game: &Game) -> Result<(), String> {
    conn.execute(
        "INSERT INTO games (id,source,appid,title,exe_path,launch_uri,custom_unlock_path,metadata_json)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8)
         ON CONFLICT(id) DO UPDATE SET appid=excluded.appid,title=excluded.title,
         exe_path=excluded.exe_path,launch_uri=excluded.launch_uri,
         custom_unlock_path=excluded.custom_unlock_path,metadata_json=excluded.metadata_json",
        params![game.id, game.source, game.appid, game.title, game.exe_path, game.launch_uri,
            game.custom_unlock_path, game.metadata_json],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn save_metadata(
    conn: &Connection,
    game_id: &str,
    metadata: &serde_json::Value,
) -> Result<(), String> {
    let existing: String = conn
        .query_row(
            "SELECT metadata_json FROM games WHERE id=?1",
            [game_id],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    let mut metadata = metadata.clone();
    if let Ok(previous) = serde_json::from_str::<serde_json::Value>(&existing) {
        if let Some(icon) = previous.get("icon") {
            metadata["icon"] = icon.clone();
        }
        for field in ["cover", "libraryCovers", "libraryHeroes"] {
            let empty = metadata.get(field).is_none_or(|value| {
                value.is_null() || value.as_str().is_some_and(|url| url.is_empty())
                    || value.as_array().is_some_and(|urls| urls.is_empty())
            });
            if empty {
                if let Some(value) = previous.get(field) {
                    metadata[field] = value.clone();
                }
            }
        }
    }
    let name = metadata
        .get("name")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    conn.execute(
        "UPDATE games SET metadata_json=?1,
         title=CASE WHEN source='steam' AND ?2<>'' THEN ?2 ELSE title END WHERE id=?3",
        params![metadata.to_string(), name, game_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn update_scan(conn: &Connection, id: &str, status: &str, file: &str) -> Result<(), String> {
    conn.execute(
        "UPDATE games SET scan_status=?1, source_file=?2, last_scan=?3 WHERE id=?4",
        params![status, file, chrono::Utc::now().to_rfc3339(), id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn save_definitions(
    conn: &mut Connection,
    game_id: &str,
    appid: &str,
    items: &[AchievementDefinition],
    source: &str,
) -> Result<(), String> {
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    let current = game(&tx, game_id)?.ok_or("游戏不存在")?;
    if (source == crate::xbox::SCHEMA
        && crate::achievement_platform::namespace(&tx, &current)? != appid)
        || (source != crate::xbox::SCHEMA && current.appid != appid)
    {
        return Err("成就平台关联已更换，已丢弃旧成就资料".into());
    }
    for item in items {
        let sql = if source == "Steam Web API" || source == crate::xbox::SCHEMA {
            "INSERT INTO achievements(appid,api_name,name,description,icon,hidden)
             VALUES (?1,?2,?3,?4,?5,?6) ON CONFLICT(appid,api_name) DO UPDATE SET
             name=excluded.name,description=excluded.description,icon=excluded.icon,hidden=excluded.hidden"
        } else {
            "INSERT INTO achievements(appid,api_name,name,description,icon,hidden)
             VALUES (?1,?2,?3,?4,?5,?6) ON CONFLICT(appid,api_name) DO UPDATE SET
             name=CASE WHEN achievements.name=achievements.api_name THEN excluded.name ELSE achievements.name END,
             description=CASE WHEN achievements.description='' THEN excluded.description ELSE achievements.description END,
             icon=CASE WHEN achievements.icon='' THEN excluded.icon ELSE achievements.icon END,
             hidden=excluded.hidden"
        };
        tx.execute(
            sql,
            params![
                appid,
                item.api_name,
                item.name,
                item.description,
                item.icon,
                item.hidden as i32
            ],
        )
        .map_err(|e| e.to_string())?;
    }
    tx.execute(
        "UPDATE games SET schema_source=?1 WHERE id=?2",
        params![source, game_id],
    )
    .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())
}

pub fn ensure_definition(conn: &Connection, appid: &str, api_name: &str) -> Result<(), String> {
    conn.execute(
        "INSERT OR IGNORE INTO achievements(appid,api_name,name) VALUES (?1,?2,?2)",
        params![appid, api_name],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn achievement_api_names(conn: &Connection, appid: &str) -> Result<HashSet<String>, String> {
    let mut stmt = conn
        .prepare("SELECT api_name FROM achievements WHERE appid=?1")
        .map_err(|e| e.to_string())?;
    let result = stmt
        .query_map([appid], |row| row.get(0))
        .map_err(|e| e.to_string())?
        .collect::<Result<HashSet<_>, _>>()
        .map_err(|e| e.to_string());
    result
}

pub fn mark_runtime_only_schema(conn: &Connection, game_id: &str) -> Result<(), String> {
    conn.execute(
        "UPDATE games SET schema_source='本地记录（仅已解锁）' WHERE id=?1 AND schema_source=''",
        [game_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn achievements(conn: &Connection, game_id: &str) -> Result<Vec<Achievement>, String> {
    let game = game(conn, game_id)?.ok_or("游戏不存在")?;
    let appid = crate::achievement_platform::namespace(conn, &game)?;
    let mut stmt = conn
        .prepare(
            "SELECT a.api_name,a.name,a.description,a.icon,a.hidden,u.unlocked_at,u.source
         FROM achievements a LEFT JOIN unlocks u ON u.game_id=?1 AND u.api_name=a.api_name
         WHERE a.appid=?2 ORDER BY a.name COLLATE NOCASE",
        )
        .map_err(|e| e.to_string())?;
    let result = stmt
        .query_map(params![game_id, appid], |row| {
            Ok(Achievement {
                api_name: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                icon: row.get(3)?,
                hidden: row.get::<_, i32>(4)? != 0,
                unlocked_at: row.get(5)?,
                unlock_source: row.get(6)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string());
    result
}

pub fn add_unlock(
    conn: &Connection,
    game_id: &str,
    api_name: &str,
    source: &str,
    unlocked_at: &str,
    evidence: &str,
) -> Result<bool, String> {
    if source == "local" || source == "xbox-local" {
        let promoted = conn
            .execute(
                "UPDATE unlocks SET source=?5,unlocked_at=?1,evidence=?2
            WHERE game_id=?3 AND api_name=?4 AND source='manual'",
                params![unlocked_at, evidence, game_id, api_name, source],
            )
            .map_err(|e| e.to_string())?;
        if promoted > 0 {
            return Ok(false);
        }
    }
    let count = conn
        .execute(
            "INSERT OR IGNORE INTO unlocks(game_id,api_name,source,unlocked_at,evidence)
        VALUES (?1,?2,?3,?4,?5)",
            params![game_id, api_name, source, unlocked_at, evidence],
        )
        .map_err(|e| e.to_string())?;
    Ok(count > 0)
}

pub fn toggle_manual(conn: &Connection, game_id: &str, api_name: &str) -> Result<(), String> {
    let existing: Option<String> = conn
        .query_row(
            "SELECT source FROM unlocks WHERE game_id=?1 AND api_name=?2",
            params![game_id, api_name],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    match existing.as_deref() {
        Some("manual") => {
            conn.execute(
                "DELETE FROM unlocks WHERE game_id=?1 AND api_name=?2",
                params![game_id, api_name],
            )
            .map_err(|e| e.to_string())?;
        }
        Some(_) => return Err("自动或 Steam 解锁记录不能手动撤销".into()),
        None => {
            add_unlock(
                conn,
                game_id,
                api_name,
                "manual",
                &chrono::Utc::now().to_rfc3339(),
                "",
            )?;
        }
    }
    Ok(())
}

pub fn replace_steam_unlocks(
    conn: &Connection,
    game_id: &str,
    items: &[(String, String)],
) -> Result<(), String> {
    // 调用方使用事务，将官方快照与新增解锁统计一起提交。
    conn.execute(
        "DELETE FROM unlocks WHERE game_id=?1 AND source='steam'",
        [game_id],
    )
    .map_err(|e| e.to_string())?;
    for (api_name, unlocked_at) in items {
        conn.execute("INSERT INTO unlocks(game_id,api_name,source,unlocked_at,evidence) VALUES (?1,?2,'steam',?3,'Steam Web API')",
            params![game_id, api_name, unlocked_at]).map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn setting(conn: &Connection, key: &str) -> Result<String, String> {
    Ok(conn
        .query_row("SELECT value FROM settings WHERE key=?1", [key], |row| {
            row.get(0)
        })
        .optional()
        .map_err(|e| e.to_string())?
        .unwrap_or_default())
}

pub fn set_setting(conn: &Connection, key: &str, value: &str) -> Result<(), String> {
    conn.execute("INSERT INTO settings(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        params![key, value]).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_metadata_uses_chinese_title_only_for_steam_games() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE games(id TEXT PRIMARY KEY, source TEXT, title TEXT, metadata_json TEXT);
             INSERT INTO games VALUES('steam-1086940','steam','Baldur''s Gate 3','{\"icon\":\"icon.jpg\"}');
             INSERT INTO games VALUES('local-1086940','local','我的自定义名称','{}');",
        )
        .unwrap();
        let metadata = serde_json::json!({"name": "博德之门3", "cover": "cover.jpg"});
        save_metadata(&conn, "steam-1086940", &metadata).unwrap();
        save_metadata(&conn, "local-1086940", &metadata).unwrap();
        let steam = conn
            .query_row(
                "SELECT title, metadata_json FROM games WHERE id='steam-1086940'",
                [],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .unwrap();
        let local: String = conn
            .query_row(
                "SELECT title FROM games WHERE id='local-1086940'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(steam.0, "博德之门3");
        assert_eq!(local, "我的自定义名称");
        let saved: serde_json::Value = serde_json::from_str(&steam.1).unwrap();
        assert_eq!(saved["icon"], "icon.jpg");
        assert_eq!(saved["cover"], "cover.jpg");
    }
    #[test]
    fn metadata_refresh_preserves_missing_artwork_and_replaces_updated_assets() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE games(id TEXT PRIMARY KEY, source TEXT, title TEXT, metadata_json TEXT);").unwrap();
        let previous = serde_json::json!({"cover": "old-header.jpg", "libraryCovers": ["old-portrait.jpg"], "libraryHeroes": ["old-hero.jpg"]});
        conn.execute("INSERT INTO games VALUES('local','local','自定义名称',?1)", [previous.to_string()]).unwrap();
        save_metadata(&conn, "local", &serde_json::json!({"name": "官方名称", "cover": "", "libraryCovers": [], "libraryHeroes": []})).unwrap();
        let read = || {
            let data: String = conn.query_row("SELECT metadata_json FROM games WHERE id='local'", [], |row| row.get(0)).unwrap();
            serde_json::from_str::<serde_json::Value>(&data).unwrap()
        };
        let retained = read();
        assert_eq!(retained["cover"], previous["cover"]);
        assert_eq!(retained["libraryCovers"], previous["libraryCovers"]);
        assert_eq!(retained["libraryHeroes"], previous["libraryHeroes"]);
        save_metadata(&conn, "local", &serde_json::json!({"libraryCovers": ["new-portrait.jpg"]})).unwrap();
        assert_eq!(read()["libraryCovers"], serde_json::json!(["new-portrait.jpg"]));
    }

    #[test]
    fn keeps_steam_and_local_unlocks_separate() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE games (id TEXT PRIMARY KEY,source TEXT,appid TEXT,title TEXT,exe_path TEXT,launch_uri TEXT,custom_unlock_path TEXT,metadata_json TEXT,scan_status TEXT,source_file TEXT,last_scan TEXT,schema_source TEXT); CREATE TABLE achievements (appid TEXT,api_name TEXT,name TEXT,description TEXT,icon TEXT,hidden INTEGER,PRIMARY KEY(appid,api_name)); CREATE TABLE unlocks (game_id TEXT,api_name TEXT,source TEXT,unlocked_at TEXT,evidence TEXT,PRIMARY KEY(game_id,api_name));").unwrap();
        conn.execute(
            "INSERT INTO unlocks VALUES('local','ACH_ONE','local','2026-01-01','file')",
            [],
        )
        .unwrap();
        replace_steam_unlocks(
            &mut conn,
            "steam",
            &[("ACH_ONE".into(), "2026-02-01".into())],
        )
        .unwrap();
        let local: String = conn
            .query_row(
                "SELECT source FROM unlocks WHERE game_id='local'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(local, "local");
    }

    #[test]
    fn cached_steam_unlocks_survive_offline_restart() {
        let root = std::env::temp_dir().join(format!("launcher-db-test-{}", uuid::Uuid::new_v4()));
        let file = root.join("games.sqlite");
        let mut conn = open(&file).unwrap();
        let game = Game {
            id: "steam-480".into(),
            source: "steam".into(),
            appid: "480".into(),
            title: "测试游戏".into(),
            exe_path: String::new(),
            launch_uri: "steam://rungameid/480".into(),
            custom_unlock_path: String::new(),
            metadata_json: "{}".into(),
            scan_status: String::new(),
            source_file: String::new(),
            last_scan: String::new(),
            schema_source: String::new(),
        };
        upsert_game(&conn, &game).unwrap();
        save_definitions(
            &mut conn,
            &game.id,
            "480",
            &[AchievementDefinition {
                api_name: "ACH_ONE".into(),
                name: "First".into(),
                description: String::new(),
                icon: String::new(),
                hidden: false,
            }],
            "Steam Web API",
        )
        .unwrap();
        replace_steam_unlocks(
            &mut conn,
            &game.id,
            &[("ACH_ONE".into(), "2026-01-01T00:00:00Z".into())],
        )
        .unwrap();
        drop(conn);
        let offline = open(&file).unwrap();
        let items = achievements(&offline, &game.id).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].unlock_source.as_deref(), Some("steam"));
        drop(offline);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn local_schema_upgrades_placeholders_without_replacing_steam_names() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE games (id TEXT PRIMARY KEY,source TEXT,appid TEXT,title TEXT,exe_path TEXT,launch_uri TEXT,custom_unlock_path TEXT,metadata_json TEXT,scan_status TEXT,source_file TEXT,last_scan TEXT,schema_source TEXT); CREATE TABLE achievements (appid TEXT,api_name TEXT,name TEXT,description TEXT,icon TEXT,hidden INTEGER,PRIMARY KEY(appid,api_name));").unwrap();
        conn.execute(
            "INSERT INTO games(id,source,appid,title,exe_path,launch_uri,custom_unlock_path,metadata_json,scan_status,source_file,last_scan,schema_source) VALUES('local','local','480','Game','','','','{}','','','','')",
            [],
        )
        .unwrap();
        ensure_definition(&conn, "480", "ACH_ONE").unwrap();
        let mut conn = conn;
        save_definitions(
            &mut conn,
            "local",
            "480",
            &[AchievementDefinition {
                api_name: "ACH_ONE".into(),
                name: "本地名称".into(),
                description: String::new(),
                icon: String::new(),
                hidden: false,
            }],
            "本地定义文件",
        )
        .unwrap();
        let name: String = conn
            .query_row(
                "SELECT name FROM achievements WHERE appid='480'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(name, "本地名称");
        save_definitions(
            &mut conn,
            "local",
            "480",
            &[AchievementDefinition {
                api_name: "ACH_ONE".into(),
                name: "Steam 官方名称".into(),
                description: String::new(),
                icon: "https://example.test/icon.png".into(),
                hidden: false,
            }],
            "Steam Web API",
        )
        .unwrap();
        save_definitions(
            &mut conn,
            "local",
            "480",
            &[AchievementDefinition {
                api_name: "ACH_ONE".into(),
                name: "旧本地名称".into(),
                description: String::new(),
                icon: String::new(),
                hidden: false,
            }],
            "本地定义文件",
        )
        .unwrap();
        let name: String = conn
            .query_row(
                "SELECT name FROM achievements WHERE appid='480'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(name, "Steam 官方名称");
    }
}
