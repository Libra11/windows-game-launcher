use crate::{db, lock_db, AppState};
use tauri::{Emitter, Manager};

#[tauri::command]
pub(crate) fn remove_local_game(app: tauri::AppHandle, game_id: String) -> Result<(), String> {
    let state = app.state::<AppState>();
    // 与扫描和启动串行处理，避免移除后又写入这款游戏的记录。
    let mut initialized = state.initialized.lock().map_err(|_| "扫描状态不可用")?;
    let mut tracker = state.runtime.lock().map_err(|_| "运行状态暂时不可用")?;
    let mut conn = lock_db(&state)?;
    let tx = conn.transaction().map_err(|error| error.to_string())?;
    let game = db::game(&tx, &game_id)?.ok_or("游戏不存在或已被移除")?;
    if game.source != "local" {
        return Err("只能移除本地游戏".into());
    }
    if matches!(
        tracker.info(&game_id).state.as_str(),
        "starting" | "running"
    ) {
        return Err("游戏正在启动或运行，请退出游戏后再移除".into());
    }
    for table in ["unlocks", "game_activity", "play_sessions"] {
        tx.execute(&format!("DELETE FROM {table} WHERE game_id=?1"), [&game_id])
            .map_err(|error| error.to_string())?;
    }
    tx.execute(
        "DELETE FROM games WHERE id=?1 AND source='local'",
        [&game_id],
    )
    .map_err(|error| error.to_string())?;
    // 成就定义按 AppID 共享，保留它们供其他游戏条目使用。
    tx.commit().map_err(|error| error.to_string())?;
    initialized.remove(&game_id);
    tracker.forget(&game_id);
    drop(conn);
    drop(tracker);
    drop(initialized);
    let _ = app.emit("library-changed", &game_id);
    Ok(())
}
