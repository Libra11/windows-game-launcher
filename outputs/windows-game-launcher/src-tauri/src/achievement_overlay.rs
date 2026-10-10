#[path = "achievement_overlay_native.rs"]
mod native;
use crate::{db, lock_db, AppState};
use native::{foreground_point, is_visible, place, raise, sound, visibility};
use serde::{Deserialize, Serialize};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc,
};
use std::time::Duration;
use tauri::{Emitter, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_notification::NotificationExt;

const LABEL: &str = "achievement-overlay";
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Options {
    pub position: String,
    pub duration: u64,
    pub sound: String,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            position: "bottom-right".into(),
            duration: 5,
            sound: "chime".into(),
        }
    }
}
impl Options {
    fn validate(&self) -> Result<(), String> {
        if !["bottom-right", "bottom-left", "top-right", "top-left"]
            .contains(&self.position.as_str())
            || !(3..=10).contains(&self.duration)
            || !["chime", "soft", "off"].contains(&self.sound.as_str())
        {
            return Err("通知设置无效".into());
        }
        Ok(())
    }
}
#[derive(Serialize)]
pub(crate) struct Notice {
    pub name: String,
    pub game: String,
    pub icon: String,
}
pub(crate) struct Overlay {
    sender: mpsc::SyncSender<Notice>,
    ready: Arc<AtomicBool>,
}
pub(crate) fn options(app: &tauri::AppHandle) -> Result<Options, String> {
    let state = app.state::<AppState>();
    let conn = lock_db(&state)?;
    let text = db::setting(&conn, "achievement_overlay_options")?;
    if text.is_empty() {
        return Ok(Options::default());
    }
    let options: Options = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    options.validate()?;
    Ok(options)
}
pub(crate) fn initialize(app: &tauri::AppHandle) -> Result<(), String> {
    let (sender, receiver) = mpsc::sync_channel::<Notice>(64);
    let ready = Arc::new(AtomicBool::new(false));
    app.manage(Overlay {
        sender,
        ready: ready.clone(),
    });
    let builder = WebviewWindowBuilder::new(
        app,
        LABEL,
        WebviewUrl::App("achievement-overlay.html".into()),
    )
    .title("游迹成就提示")
    .inner_size(420.0, 132.0)
    .decorations(false)
    .resizable(false)
    .maximizable(false)
    .minimizable(false)
    .max_inner_size(420.0, 132.0)
    .shadow(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .focused(false)
    .focusable(false)
    .visible(false)
    .transparent(true);
    let window = crate::webview_proxy::configure(app, builder)
        .build()
        .map_err(|e| e.to_string())?;
    window
        .set_ignore_cursor_events(true)
        .map_err(|e| e.to_string())?;
    let handle = app.clone();
    std::thread::spawn(move || {
        for notice in receiver {
            for _ in 0..100 {
                if ready.load(Ordering::Acquire) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            if !ready.load(Ordering::Acquire) {
                eprintln!("成就弹层尚未就绪");
                fallback(&handle, &notice);
                continue;
            }
            let result = (|| -> Result<(), String> {
                let opts = options(&handle)?;
                position(&handle, &window, &opts)?;
                raise(&window)?;
                window
                    .emit("achievement-overlay", &notice)
                    .map_err(|e| e.to_string())?;
                visibility(&window, true)?;
                sound(&opts.sound);
                // 重申置顶但不获取焦点，覆盖游戏重新置顶的窗口。
                for _ in 0..opts.duration * 4 {
                    std::thread::sleep(Duration::from_millis(250));
                    position(&handle, &window, &options(&handle)?)?;
                    raise(&window)?;
                }
                Ok(())
            })();
            let _ = visibility(&window, false);
            let _ = window.emit("achievement-overlay-hidden", ());
            if let Err(error) = result {
                eprintln!("成就弹层显示失败：{error}");
                fallback(&handle, &notice);
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    });
    Ok(())
}
fn fallback(app: &tauri::AppHandle, notice: &Notice) {
    let _ = app
        .notification()
        .builder()
        .title(format!("成就解锁 · {}", notice.game))
        .body(&notice.name)
        .show();
}
fn position(
    app: &tauri::AppHandle,
    window: &tauri::WebviewWindow,
    opts: &Options,
) -> Result<(), String> {
    let monitor = foreground_point()
        .and_then(|(x, y)| app.monitor_from_point(x, y).ok().flatten())
        .or_else(|| app.primary_monitor().ok().flatten())
        .ok_or("无法获取游戏屏幕")?;
    let scale = monitor.scale_factor();
    let area = monitor.work_area();
    let width = (420.0 * scale) as i32;
    let height = (132.0 * scale) as i32;
    let margin = (16.0 * scale) as i32;
    let x = area.position.x
        + if opts.position.ends_with("right") {
            area.size.width as i32 - width - margin
        } else {
            margin
        };
    let y = area.position.y
        + if opts.position.starts_with("bottom") {
            area.size.height as i32 - height - margin
        } else {
            margin
        };
    place(window, x, y, width, height)
}
pub(crate) fn show(app: &tauri::AppHandle, notice: Notice) -> Result<(), String> {
    if app.get_webview_window(LABEL).is_none() {
        return Err("成就弹层不可用".into());
    }
    app.try_state::<Overlay>()
        .ok_or("成就弹层不可用")?
        .sender
        .try_send(notice)
        .map_err(|_| "成就提示队列已满或已停止".into())
}
#[tauri::command]
pub(crate) fn achievement_overlay_ready(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
) -> Result<(), String> {
    if window.label() != LABEL {
        return Err("窗口无权确认弹层".into());
    }
    app.state::<Overlay>().ready.store(true, Ordering::Release);
    Ok(())
}
#[tauri::command]
pub(crate) fn get_achievement_overlay_options(app: tauri::AppHandle) -> Result<Options, String> {
    options(&app)
}
#[tauri::command]
pub(crate) fn save_achievement_overlay_options(
    app: tauri::AppHandle,
    options: Options,
) -> Result<(), String> {
    options.validate()?;
    let state = app.state::<AppState>();
    let conn = lock_db(&state)?;
    db::set_setting(
        &conn,
        "achievement_overlay_options",
        &serde_json::to_string(&options).map_err(|e| e.to_string())?,
    )?;
    drop(conn);
    if let Some(window) = app.get_webview_window(LABEL) {
        if is_visible(&window)? {
            position(&app, &window, &options)?;
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_invalid_options() {
        let mut opts = Options::default();
        assert!(opts.validate().is_ok());
        opts.duration = 0;
        assert!(opts.validate().is_err());
        opts.duration = 5;
        opts.position = "invalid".into();
        assert!(opts.validate().is_err());
        opts.position = "top-left".into();
        opts.sound = "unknown".into();
        assert!(opts.validate().is_err());
    }
}
