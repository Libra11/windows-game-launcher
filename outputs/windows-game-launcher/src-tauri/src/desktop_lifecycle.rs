use crate::{db, lock_db, runtime_persistence, AppState};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager,
};

pub(crate) fn show(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

pub(crate) fn initialize(app: &tauri::AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "show", "打开游迹", true, None::<&str>)?;
    let quit = MenuItem::with_id(
        app,
        "quit",
        "退出启动器（游戏继续运行）",
        true,
        None::<&str>,
    )?;
    let menu = Menu::with_items(app, &[&open, &quit])?;
    let mut tray = TrayIconBuilder::with_id("launcher")
        .tooltip("游迹 · 后台计时与成就检测")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show(app),
            "quit" => {
                if let Err(error) = quit_launcher(app.clone()) {
                    show(app);
                    let _ = app.emit("launcher-error", error);
                }
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
            ) {
                show(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

pub(crate) fn closing(window: &tauri::Window, event: &tauri::WindowEvent) {
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
