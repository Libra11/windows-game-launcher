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
            if let Err(error) = crate::statistics::store::record_unlock(&conn, event) {
                eprintln!("新解锁统计未保存：{error}");
            }
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
            let icon = {
                let state = app.state::<AppState>();
                lock_db(&state)
                    .and_then(|conn| db::achievements(&conn, &event.game_id))
                    .unwrap_or_default()
                    .into_iter()
                    .find(|item| item.api_name == event.api_name)
                    .map(|item| item.icon)
                    .unwrap_or_default()
            };
            if let Err(error) = crate::achievement_overlay::show(
                app,
                crate::achievement_overlay::Notice {
                    name: event.achievement_name.clone(),
                    game: event.game_title.clone(),
                    icon,
                },
            ) {
                eprintln!("成就弹层未发送：{error}");
                let _ = show(
                    app,
                    &format!("成就解锁 · {}", event.game_title),
                    &event.achievement_name,
                );
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
pub(crate) fn test_achievement_notification(
    app: tauri::AppHandle,
    delay_seconds: Option<u64>,
) -> Result<String, String> {
    let delay = delay_seconds.unwrap_or(0);
    if delay > 10 {
        return Err("测试延迟不得超过 10 秒".into());
    }
    if delay > 0 {
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
            if let Err(error) = crate::achievement_overlay::show(
                &app,
                crate::achievement_overlay::Notice {
                    name: "每一步都值得记录".into(),
                    game: "游迹 · 游戏内弹层测试".into(),
                    icon: String::new(),
                },
            ) {
                eprintln!("延迟测试失败：{error}");
            }
        });
        return Ok(format!("{delay} 秒后显示测试弹层，请切回游戏。"));
    }
    crate::achievement_overlay::show(
        &app,
        crate::achievement_overlay::Notice {
            name: "每一步都值得记录".into(),
            game: "游迹 · 成就弹层测试".into(),
            icon: String::new(),
        },
    )?;
    Ok("测试弹层已排队；可在无边框游戏中测试，独占全屏可能遮挡弹层。".into())
}
