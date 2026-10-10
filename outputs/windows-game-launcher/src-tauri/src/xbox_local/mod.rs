mod events;
pub(crate) fn has_active_capture() -> bool {
    #[cfg(windows)] { service::has_active_capture() }
    #[cfg(not(windows))] { false }
}
#[cfg(windows)]
mod service;
use crate::{
    db, lock_db,
    model::{Game, UnlockEvent},
    AppState,
};
#[cfg(windows)]
use tauri::Emitter;

#[cfg(windows)]
pub(crate) const NAMESPACE: &str = "xbox:64439fe5";
pub(crate) const SCHEMA: &str = "Xbox 本地事件（仅已确认）";

pub(crate) fn supported(game: &Game) -> bool {
    if game.source != "local" {
        return false;
    }
    let path = std::path::Path::new(&game.exe_path);
    if !path.file_name().is_some_and(|name| {
        name.to_string_lossy()
            .eq_ignore_ascii_case("WellDweller.exe")
    }) {
        return false;
    }
    let Some(folder) = path.parent() else {
        return false;
    };
    std::fs::read_to_string(folder.join("MicrosoftGame.config")).is_ok_and(|xml| {
        xml.to_ascii_lowercase()
            .contains("<titleid>64439fe5</titleid>")
            && folder.join("GDKExtension.dll").is_file()
    })
}

pub(crate) fn scan(
    app: &tauri::AppHandle,
    state: &AppState,
    game: &Game,
    notify: bool,
) -> Result<Vec<UnlockEvent>, String> {
    #[cfg(not(windows))]
    {
        let _ = (app, notify);
        db::update_scan(
            &*lock_db(state)?,
            &game.id,
            "Xbox 本地接口捕获仅在 Windows 桌面端提供",
            "",
        )?;
        Ok(Vec::new())
    }
    #[cfg(windows)]
    {
        let (file, status) = service::ensure(app, game)?;
        let text = if file.is_file() {
            if file.metadata().map_err(|e| e.to_string())?.len() > 20 * 1024 * 1024 {
                return Err("Xbox 事件记录超过读取限制，请归档后重新捕获".into());
            }
            std::fs::read_to_string(&file).map_err(|e| format!("无法读取 Xbox 事件：{e}"))?
        } else {
            String::new()
        };
        let observations = events::parse(&text);
        let conn = lock_db(state)?;
        let current = db::game(&conn, &game.id)?.ok_or("游戏不存在")?;
        if current.exe_path != game.exe_path
            || !supported(&current)
            || crate::achievement_platform::for_game(&conn, &current)?.platform != "xbox"
        {
            return Err("游戏路径已变化，等待重新检测".into());
        }
        let mut output = Vec::new();
        let mut changed = false;
        for event in observations {
            let api_name = format!("{NAMESPACE}:{}", event.id);
            let name = format!("Xbox 成就 #{}", event.id);
            conn.execute("INSERT OR IGNORE INTO achievements(appid,api_name,name,description,icon,hidden) VALUES(?1,?2,?3,?4,'',0)",
                rusqlite::params![NAMESPACE,api_name,name,"游戏真实调用进度 100%，完成回调成功；名称尚待 Xbox 定义补全"]).map_err(|e|e.to_string())?;
            let name: String = conn
                .query_row(
                    "SELECT name FROM achievements WHERE appid=?1 AND api_name=?2",
                    rusqlite::params![NAMESPACE, api_name],
                    |row| row.get(0),
                )
                .map_err(|e| e.to_string())?;
            let added = db::add_unlock(
                &conn,
                &game.id,
                &api_name,
                "xbox-local",
                &event.at,
                &file.to_string_lossy(),
            )?;
            changed |= added;
            if added && notify && !event.quiet {
                output.push(UnlockEvent {
                    game_id: game.id.clone(),
                    game_title: game.title.clone(),
                    api_name,
                    achievement_name: name,
                });
            }
        }
        if current.schema_source != crate::xbox::SCHEMA {
            conn.execute(
                "UPDATE games SET schema_source=?1 WHERE id=?2",
                rusqlite::params![SCHEMA, game.id],
            )
            .map_err(|e| e.to_string())?;
        }
        db::update_scan(&conn, &game.id, &status, &file.to_string_lossy())?;
        drop(conn);
        if changed || current.scan_status != status {
            let _ = app.emit("library-changed", ());
        }
        Ok(output)
    }
}

pub(crate) fn retain(games: &[Game]) {
    #[cfg(windows)]
    service::retain(
        &games
            .iter()
            .filter(|g| supported(g))
            .map(|g| g.id.clone())
            .collect::<Vec<_>>(),
    );
    #[cfg(not(windows))]
    let _ = games;
}

pub(crate) fn stop_all() {
    #[cfg(windows)]
    service::stop_all();
}
