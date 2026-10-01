use crate::{db, epic_auth, epic_library, lock_db, model::Game, AppState};
use tauri::{Emitter, Manager};

fn save_imported(conn: &mut rusqlite::Connection, games: Vec<Game>) -> Result<usize, String> {
    let transaction = conn.transaction().map_err(|error| error.to_string())?;
    let mut added = 0;
    for mut game in games {
        if let Some(existing) = db::game(&transaction, &game.id)? {
            let old: serde_json::Value =
                serde_json::from_str(&existing.metadata_json).unwrap_or_default();
            let mut metadata: serde_json::Value =
                serde_json::from_str(&game.metadata_json).map_err(|_| "Epic 游戏资料无效")?;
            if let Some(details) = old.get("epicAchievementDetails") {
                metadata["epicAchievementDetails"] = details.clone();
            }
            game.metadata_json = metadata.to_string();
            game.exe_path = existing.exe_path;
            game.custom_unlock_path = existing.custom_unlock_path;
        } else {
            added += 1;
        }
        db::upsert_game(&transaction, &game)?;
    }
    transaction.commit().map_err(|error| error.to_string())?;
    Ok(added)
}

#[tauri::command]
pub(crate) async fn import_epic(app: tauri::AppHandle) -> Result<usize, String> {
    let _guard = epic_auth::AUTH_LOCK.lock().await;
    let session = epic_auth::session(&app, false).await?;
    let games = match epic_library::owned_games(&session.access_token).await {
        Ok(games) => games,
        Err(epic_library::Error::Unauthorized) => {
            let session = epic_auth::session(&app, true).await?;
            epic_library::owned_games(&session.access_token)
                .await
                .map_err(|error| match error {
                    epic_library::Error::Unauthorized => "Epic 授权已失效，请重新登录".into(),
                    epic_library::Error::Message(message) => message,
                })?
        }
        Err(epic_library::Error::Message(message)) => return Err(message),
    };
    let state = app.state::<AppState>();
    let added = save_imported(&mut *lock_db(&state)?, games)?;
    let _ = app.emit("library-changed", "epic");
    Ok(added)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repeated_import_refreshes_metadata_without_losing_paths_or_favorites() {
        let mut conn = db::open(std::path::Path::new(":memory:")).unwrap();
        let game = Game {
            id: "epic-test".into(),
            source: "epic".into(),
            appid: String::new(),
            title: "测试游戏".into(),
            exe_path: String::new(),
            launch_uri: "com.epicgames.launcher://apps/test?action=launch".into(),
            custom_unlock_path: String::new(),
            metadata_json: "{}".into(),
            scan_status: String::new(),
            source_file: String::new(),
            last_scan: String::new(),
            schema_source: String::new(),
        };
        assert_eq!(save_imported(&mut conn, vec![game.clone()]).unwrap(), 1);
        crate::activity::set_favorite(&conn, &game.id, true).unwrap();
        let mut saved = game.clone();
        saved.exe_path = "D:/Games/game.exe".into();
        saved.custom_unlock_path = "records.json".into();
        db::upsert_game(&conn, &saved).unwrap();
        let mut updated = game.clone();
        updated.title = "更新的游戏名称".into();
        updated.metadata_json = "{\"description\":\"更新资料\"}".into();
        assert_eq!(save_imported(&mut conn, vec![updated.clone()]).unwrap(), 0);
        let actual = db::game(&conn, &game.id).unwrap().unwrap();
        assert_eq!(actual.exe_path, saved.exe_path);
        assert_eq!(actual.title, updated.title);
        assert_eq!(actual.metadata_json, updated.metadata_json);
        assert_eq!(actual.custom_unlock_path, saved.custom_unlock_path);
        assert_eq!(db::games(&conn).unwrap().len(), 1);
        assert!(crate::activity::all(&conn).unwrap()[&game.id].favorite);
        assert_eq!(save_imported(&mut conn, vec![]).unwrap(), 0);
    }
}
