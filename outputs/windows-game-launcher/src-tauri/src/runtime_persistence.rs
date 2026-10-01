use crate::{activity, db, lock_db, runtime, AppState};
use rusqlite::OptionalExtension;
use std::collections::HashSet;
use tauri::Manager;

pub(crate) fn restore(app: &tauri::AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let mut tracker = state.runtime.lock().map_err(|_| "运行状态不可用")?;
    let conn = lock_db(&state)?;
    let text = db::setting(&conn, "runtime_recovery")?;
    let records: Vec<runtime::RecoveryRecord> = if text.is_empty() {
        Vec::new()
    } else {
        serde_json::from_str(&text).map_err(|_| "运行恢复记录无法读取")?
    };
    let mut restored = HashSet::new();
    for record in records {
        let Some(game) = db::game(&conn, &record.game_id)? else {
            continue;
        };
        let seconds: Option<i64> = conn
            .query_row(
                "SELECT seconds FROM play_sessions WHERE id=?1 AND game_id=?2 AND ended_at=''",
                rusqlite::params![record.session_id, record.game_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        if let Some(seconds) = seconds {
            let seconds = u64::try_from(seconds).map_err(|_| "运行恢复记录中的时长无效")?;
            if tracker.recover(&record, game, seconds) {
                restored.insert(record.session_id);
            }
        }
    }
    let open: Vec<String> = {
        let mut stmt = conn
            .prepare("SELECT id FROM play_sessions WHERE ended_at=''")
            .map_err(|error| error.to_string())?;
        let rows = stmt
            .query_map([], |row| row.get(0))
            .map_err(|error| error.to_string())?;
        rows.collect::<Result<_, _>>()
            .map_err(|error| error.to_string())?
    };
    for id in open {
        if !restored.contains(&id) {
            // 不补算启动器停止或异常退出期间无法确认的时间。
            conn.execute(
                "UPDATE play_sessions SET ended_at=?1 WHERE id=?2",
                rusqlite::params![chrono::Utc::now().to_rfc3339(), id],
            )
            .map_err(|error| error.to_string())?;
        }
    }
    db::set_setting(
        &conn,
        "runtime_recovery",
        &serde_json::to_string(&tracker.recovery_records()).map_err(|error| error.to_string())?,
    )
}

pub(crate) fn checkpoint(
    app: &tauri::AppHandle,
    force: bool,
) -> Result<Vec<runtime::Update>, String> {
    let state = app.state::<AppState>();
    let mut pending = state
        .runtime_pending
        .lock()
        .map_err(|_| "计时保存状态不可用")?;
    let mut tracker = state.runtime.lock().map_err(|_| "运行状态不可用")?;
    let mut updates = tracker.tick();
    if force {
        updates.extend(tracker.checkpoints());
    }
    for update in updates {
        if let Some(previous) = pending
            .iter_mut()
            .find(|previous| previous.session_id == update.session_id)
        {
            let started_at = update
                .started_at
                .clone()
                .or_else(|| previous.started_at.clone());
            *previous = runtime::Update {
                started_at,
                ..update
            };
        } else {
            pending.push(update);
        }
    }
    let mut conn = lock_db(&state)?;
    let mut saved = Vec::new();
    let mut failures = Vec::new();
    pending.retain(|update| {
        let result = (|| -> Result<(), String> {
            if db::game(&conn, &update.game_id)?.is_none() {
                return Ok(());
            }
            if let Some(time) = &update.started_at {
                activity::start(&mut conn, &update.session_id, &update.game_id, time)?;
            }
            if !update.failed {
                activity::checkpoint(
                    &mut conn,
                    &update.session_id,
                    update.seconds,
                    update.finished,
                )?;
            }
            Ok(())
        })();
        match result {
            Ok(()) => {
                saved.push(update.clone());
                false
            }
            Err(error) => {
                failures.push(error);
                true
            }
        }
    });
    let recovery =
        serde_json::to_string(&tracker.recovery_records()).map_err(|error| error.to_string())?;
    if recovery != db::setting(&conn, "runtime_recovery")? {
        db::set_setting(&conn, "runtime_recovery", &recovery)?;
    }
    if failures.is_empty() {
        Ok(saved)
    } else {
        Err(format!("计时保存失败，将自动重试：{}", failures.join("；")))
    }
}
