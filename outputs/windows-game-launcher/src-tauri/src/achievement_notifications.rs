use crate::{db, lock_db, model::UnlockEvent, AppState};
use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager};
use tauri_plugin_notification::NotificationExt;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RecentUnlock {
    game_id: String,
    game_title: String,
    achievement_name: String,
    unlocked_at: String,
}

fn read(conn: &rusqlite::Connection) -> Result<Vec<RecentUnlock>, String> {
    let text = db::setting(conn, "recent_achievement_notifications")?;
    if text.is_empty() {
        return Ok(Vec::new());
    }
    serde_json::from_str(&text).map_err(|_| "最近成就记录无法读取".into())
}

pub(crate) fn notify(app: &tauri::AppHandle, events: Vec<UnlockEvent>) {
    if events.is_empty() {
        return;
    }
    let enabled = (|| -> Result<bool, String> {
        let state = app.state::<AppState>();
        let conn = lock_db(&state)?;
        let mut recent = read(&conn)?;
        for event in &events {
            recent.insert(
                0,
                RecentUnlock {
                    game_id: event.game_id.clone(),
                    game_title: event.game_title.clone(),
                    achievement_name: event.achievement_name.clone(),
                    unlocked_at: chrono::Utc::now().to_rfc3339(),
                },
            );
        }
        recent.truncate(50);
        db::set_setting(
            &conn,
            "recent_achievement_notifications",
            &serde_json::to_string(&recent).map_err(|error| error.to_string())?,
        )?;
        Ok(db::setting(&conn, "achievement_notifications")? != "false")
    })()
    .unwrap_or(true);
    for event in events {
        let _ = app.emit("achievement-unlocked", &event);
        if enabled {
            if let Err(error) = show(
                app,
                &format!("成就解锁 · {}", event.game_title),
                &event.achievement_name,
            ) {
                eprintln!("成就通知未发送：{error}");
            }
        }
    }
    let _ = app.emit("library-changed", ());
}

pub(crate) fn monitor_failed(app: &tauri::AppHandle, title: &str, reason: &str) {
    let message =
        format!("{title} 的成就监听已停止：{reason}。当前无法捕获新解锁，请查看检测详情。");
    let _ = app.emit("launcher-error", &message);
    let state = app.state::<AppState>();
    let enabled = lock_db(&state)
        .and_then(|conn| db::setting(&conn, "achievement_notifications"))
        .is_ok_and(|value| value != "false");
    if enabled {
        let _ = show(app, "成就监听异常", &message);
    }
}

fn show(app: &tauri::AppHandle, title: &str, body: &str) -> Result<(), String> {
    app.notification()
        .builder()
        .title(title)
        .body(body)
        .show()
        .map_err(|error| format!("Windows 通知发送失败：{error}"))
}

#[tauri::command]
pub(crate) fn list_recent_unlocks(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<RecentUnlock>, String> {
    read(&*lock_db(&state)?)
}

#[tauri::command]
pub(crate) fn test_achievement_notification(app: tauri::AppHandle) -> Result<String, String> {
    show(
        &app,
        "游戏收藏室 · 通知测试",
        "成就通知已提交。可以在全屏游戏中再次测试显示效果。",
    )?;
    Ok("测试通知已提交；若未弹出，请查看 Windows 通知设置、勿扰模式和通知中心。".into())
}
