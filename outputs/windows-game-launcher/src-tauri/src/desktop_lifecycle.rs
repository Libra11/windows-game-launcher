use crate::{db, lock_db, runtime_persistence, AppState};
pub(crate) mod popup;
mod recent;
use tauri::{
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager,
};

pub(crate) fn show(app: &tauri::AppHandle) {
    popup::hide(app);
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

pub(crate) fn initialize(
    app: &tauri::AppHandle,
    restored_appearance: Option<&str>,
) -> tauri::Result<()> {
    app.manage(recent::State::default());
    popup::initialize(app, restored_appearance).map_err(std::io::Error::other)?;
    let mut tray = TrayIconBuilder::with_id("launcher")
        .tooltip("游迹 · 后台计时与成就检测")
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| {
            match event {
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } => show(tray.app_handle()),
                TrayIconEvent::Click {
                    button: MouseButton::Right,
                    button_state: MouseButtonState::Up,
                    position,
                    ..
                } => {
                    if let Err(error) = popup::show(tray.app_handle(), position.x, position.y) {
                        eprintln!("托盘菜单显示失败：{error}");
                    }
                }
                _ => {}
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    recent::watch(app.clone());
    Ok(())
}

pub(crate) fn closing(window: &tauri::Window, event: &tauri::WindowEvent) {
    if window.label() == popup::LABEL {
        match event {
            tauri::WindowEvent::Focused(false) => popup::hide(window.app_handle()),
            tauri::WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                popup::hide(window.app_handle());
            }
            _ => {}
        }
        return;
    }
    if window.label() != "main" {
        return;
    }
    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
        let app = window.app_handle();
        let state = app.state::<AppState>();
        let background = lock_db(&state)
            .and_then(|conn| db::setting(&conn, "close_to_tray"))
            .map(|value| value != "false")
            .unwrap_or(false);
        if background && app.tray_by_id("launcher").is_some() {
            api.prevent_close();
            if let Err(error) = window.hide() {
                let _ = app.emit("launcher-error", format!("隐藏窗口失败：{error}"));
            }
        } else if let Err(error) = runtime_persistence::checkpoint(app, true) {
            api.prevent_close();
            let _ = app.emit("launcher-error", error);
        }
    }
}

#[tauri::command]
pub(crate) fn quit_launcher(app: tauri::AppHandle) -> Result<(), String> {
    runtime_persistence::checkpoint(&app, true)?;
    app.exit(0);
    Ok(())
}
