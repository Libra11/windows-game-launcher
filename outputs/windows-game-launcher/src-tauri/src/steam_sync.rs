use crate::{
    db,
    library_commands::needs_store_metadata,
    lock_db,
    model::{AchievementDefinition, Game},
    source, steam,
    steam_achievements::NO_ACHIEVEMENTS_SOURCE,
    AppState,
};
use rusqlite::Connection;
use tauri::{Emitter, Manager};

fn save_steam_schema(
    conn: &mut Connection,
    game: &Game,
    items: &[AchievementDefinition],
) -> Result<(), String> {
    if items.is_empty() {
        // 空定义不覆盖已有资料，也不删除任何历史解锁记录。
        if game.source == "steam" && db::achievements(conn, &game.id)?.is_empty() {
            db::save_definitions(conn, &game.id, &game.appid, items, NO_ACHIEVEMENTS_SOURCE)?;
        }
    } else {
        db::save_definitions(conn, &game.id, &game.appid, items, "Steam Web API")?;
    }
    Ok(())
}

pub(crate) async fn sync_game_internal(
    app: &tauri::AppHandle,
    game_id: &str,
    full: bool,
) -> Result<String, String> {
    let state = app.state::<AppState>();
    let (game, key, steamid) = {
        let conn = lock_db(&state)?;
        (
            db::game(&conn, game_id)?.ok_or("游戏不存在")?,
            db::setting(&conn, "steam_api_key")?,
            db::setting(&conn, "steam_id")?,
        )
    };
    if game.source == "epic" {
        return crate::epic_achievements::sync(app, &game).await;
    }
    let profile = crate::achievement_platform::for_game(&*lock_db(&state)?, &game)?;
    let mut updates = Vec::new();
    if !game.appid.is_empty() && (full || needs_store_metadata(&game)) {
        if let Ok(meta) = steam::metadata(&game.appid).await {
            let conn = lock_db(&state)?;
            if db::game(&conn, &game.id)?.is_none_or(|current| current.appid != game.appid) {
                return Err("Steam 关联已变更，已丢弃旧游戏资料".into());
            }
            db::save_metadata(&conn, &game.id, &meta)?;
            updates.push("游戏资料");
        }
    }
    let mut steam_schema_loaded = false;
    if profile.platform == "xbox" {
        crate::scanner::scan_now(app.clone(), game.id.clone())?;
        let count = crate::xbox::sync(app, &game.id).await?;
        return Ok(format!(
            "已载入 Xbox 公开成就资料：{count} 项；解锁记录保存在本地"
        ));
    }
    if profile.platform != "steam" {
        return if updates.is_empty() {
            Err("成就平台尚未确认或已选择不关联成就".into())
        } else {
            Ok("已更新游戏资料；未关联成就平台".into())
        };
    }
    if game.appid.is_empty() {
        return Err("Steam 成就需要关联 Steam AppID".into());
    }
    let mut no_steam_achievements = game.schema_source == NO_ACHIEVEMENTS_SOURCE;
    if !key.is_empty() && (full || game.schema_source.is_empty()) {
        match steam::definitions(&game.appid, &key).await {
            Ok(items) => {
                save_steam_schema(&mut *lock_db(&state)?, &game, &items)?;
                no_steam_achievements = items.is_empty();
                steam_schema_loaded = !items.is_empty();
                updates.push(if no_steam_achievements {
                    "Steam 成就信息（暂无成就）"
                } else {
                    "Steam 成就定义"
                });
            }
            Err(error) => eprintln!("成就定义同步失败：{error}"),
        }
    }
    if game.source == "local" {
        if let Some(file) = source::local_schema_path(&game) {
            if let Ok(text) = std::fs::read_to_string(&file) {
                if let Ok(items) = source::parse_local_schema(&text, &file) {
                    if !items.is_empty()
                        && !steam_schema_loaded
                        && game.schema_source != "Steam Web API"
                    {
                        db::save_definitions(
                            &mut *lock_db(&state)?,
                            &game.id,
                            &game.appid,
                            &items,
                            "本地定义文件",
                        )?;
                        updates.push("本地成就定义");
                    }
                }
            }
        }
    }
    if game.source == "steam" && no_steam_achievements {
        let conn = lock_db(&state)?;
        let status = if db::achievements(&conn, &game.id)?.is_empty() {
            "此游戏暂无 Steam 成就"
        } else {
            "Steam 当前未提供成就定义，已保留缓存"
        };
        db::update_scan(&conn, &game.id, status, "Steam Web API")?;
        if updates.is_empty() {
            updates.push("Steam 成就信息（暂无成就）");
        }
    } else if game.source == "steam" && !key.is_empty() && !steamid.is_empty() {
        let unlocks = steam::player_unlocks(&game.appid, &key, &steamid).await?;
        db::replace_steam_unlocks(&mut *lock_db(&state)?, &game.id, &unlocks)?;
        db::update_scan(
            &*lock_db(&state)?,
            &game.id,
            "Steam 成就已同步",
            "Steam Web API",
        )?;
        updates.push("官方解锁状态");
    }
    if updates.is_empty() {
        return Err("未获取到资料；请检查网络、API Key 或本地定义文件".into());
    }
    Ok(format!("已更新：{}", updates.join("、")))
}

#[tauri::command]
pub(crate) async fn sync_game(app: tauri::AppHandle, game_id: String) -> Result<String, String> {
    let is_steam = {
        let state = app.state::<AppState>();
        let conn = lock_db(&state)?;
        db::game(&conn, &game_id)?.is_some_and(|game| game.source == "steam")
    };
    if is_steam {
        let _ = crate::steam_playtime::refresh(&app).await;
    }
    let result = match sync_game_internal(&app, &game_id, true).await {
        Ok(message) => {
            let state = app.state::<AppState>();
            if let Ok(conn) = lock_db(&state) {
                if db::game(&conn, &game_id)?.is_some_and(|g| {
                    crate::achievement_platform::for_game(&conn, &g)
                        .is_ok_and(|p| p.platform != "xbox")
                }) {
                    let _ = db::set_setting(
                        &conn,
                        &format!("achievement_definition_error:{game_id}"),
                        "",
                    );
                }
            }
            Ok(message)
        }
        Err(error) => {
            let state = app.state::<AppState>();
            if let Ok(conn) = lock_db(&state) {
                let _ = db::set_setting(
                    &conn,
                    &format!("achievement_definition_error:{game_id}"),
                    &error,
                );
            }
            Err(error)
        }
    };
    let _ = app.emit("library-changed", ());
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;

    #[test]
    fn empty_schema_preserves_cached_definitions_and_unlocks() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE games (id TEXT PRIMARY KEY,source TEXT,appid TEXT,title TEXT,exe_path TEXT,launch_uri TEXT,custom_unlock_path TEXT,metadata_json TEXT,scan_status TEXT,source_file TEXT,last_scan TEXT,schema_source TEXT); CREATE TABLE achievements (appid TEXT,api_name TEXT,name TEXT,description TEXT,icon TEXT,hidden INTEGER,PRIMARY KEY(appid,api_name)); CREATE TABLE unlocks (game_id TEXT,api_name TEXT,source TEXT,unlocked_at TEXT,evidence TEXT,PRIMARY KEY(game_id,api_name));").unwrap();
        conn.execute(
            "INSERT INTO games VALUES('steam-480','steam','480','Test','','','','{}','','','','')",
            [],
        )
        .unwrap();
        let game = db::game(&conn, "steam-480").unwrap().unwrap();
        save_steam_schema(&mut conn, &game, &[]).unwrap();
        assert_eq!(
            db::game(&conn, &game.id).unwrap().unwrap().schema_source,
            NO_ACHIEVEMENTS_SOURCE
        );
        let definition = AchievementDefinition {
            api_name: "FIRST".into(),
            name: "初次冒险".into(),
            description: String::new(),
            icon: String::new(),
            hidden: false,
        };
        save_steam_schema(&mut conn, &game, &[definition]).unwrap();
        conn.execute(
            "INSERT INTO unlocks VALUES(?1,'FIRST','steam','2026-01-01T00:00:00Z','Steam')",
            params![game.id],
        )
        .unwrap();
        save_steam_schema(&mut conn, &game, &[]).unwrap();
        assert_eq!(
            db::game(&conn, &game.id).unwrap().unwrap().schema_source,
            "Steam Web API"
        );
        let cached = db::achievements(&conn, &game.id).unwrap();
        assert_eq!(cached.len(), 1);
        assert_eq!(cached[0].name, "初次冒险");
        assert_eq!(cached[0].unlock_source.as_deref(), Some("steam"));
    }
}
