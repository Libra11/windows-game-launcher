use super::recent;
use std::sync::Mutex;
use tauri::{Emitter, LogicalSize, Manager, PhysicalPosition, WebviewUrl, WebviewWindowBuilder};

pub(super) const LABEL: &str = "tray-menu";
const WIDTH: f64 = 320.0;
const SCREEN_MARGIN: f64 = 8.0;
// 与 tray-menu.css 的 body padding 一致，让可见卡片边缘对齐鼠标落点。
const CONTENT_INSET: f64 = 8.0;

#[derive(Default)]
struct PopupState(Mutex<Option<(f64, f64)>>);

pub(super) fn initialize(
    app: &tauri::AppHandle,
    restored_appearance: Option<&str>,
) -> Result<(), String> {
    app.manage(PopupState::default());
    let builder = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("tray-menu.html".into()))
        .title("游迹快速启动")
        .inner_size(WIDTH, 266.0)
        .decorations(false)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .focused(false)
        .visible(false)
        .transparent(true);
    let builder = if let Some(script) = restored_appearance {
        builder.initialization_script(script)
    } else {
        builder
    };
    crate::webview_proxy::configure(app, builder)
        .build()
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub(super) fn hide(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.hide();
    }
}

pub(super) fn update(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window(LABEL) {
        if window.is_visible().unwrap_or(false) {
            let point = app
                .state::<PopupState>()
                .0
                .lock()
                .ok()
                .and_then(|point| *point);
            if let Some((x, y)) = point {
                if place(app, &window, x, y).is_err() {
                    return;
                }
            }
            let _ = window.emit("tray-menu-changed", recent::snapshot(app));
        }
    }
}

pub(super) fn show(app: &tauri::AppHandle, x: f64, y: f64) -> Result<(), String> {
    let window = app.get_webview_window(LABEL).ok_or("托盘菜单不可用")?;
    place(app, &window, x, y)?;
    *app.state::<PopupState>()
        .0
        .lock()
        .map_err(|_| "托盘菜单位置不可用")? = Some((x, y));
    window.show().map_err(|error| error.to_string())?;
    window.set_focus().map_err(|error| error.to_string())?;
    window
        .emit("tray-menu-open", recent::snapshot(app))
        .map_err(|error| error.to_string())
}

fn popup_origin(
    click: (i32, i32),
    size: (i32, i32),
    area: (i32, i32, i32, i32),
    inset: i32,
    margin: i32,
) -> (i32, i32) {
    let (left, top, right, bottom) = area;
    let (width, height) = size;
    let mut x = click.0 - inset;
    let mut y = click.1 - inset;
    if x + width > right - margin {
        x = click.0 - width + inset;
    }
    if y + height > bottom - margin {
        y = click.1 - height + inset;
    }
    (
        x.clamp(left + margin, (right - width - margin).max(left + margin)),
        y.clamp(top + margin, (bottom - height - margin).max(top + margin)),
    )
}

fn place(
    app: &tauri::AppHandle,
    window: &tauri::WebviewWindow,
    x: f64,
    y: f64,
) -> Result<(), String> {
    let monitor = app
        .monitor_from_point(x, y)
        .ok()
        .flatten()
        .or_else(|| app.primary_monitor().ok().flatten())
        .ok_or("无法获取托盘所在屏幕")?;
    let scale = monitor.scale_factor();
    let area = monitor.work_area();
    let snapshot = recent::snapshot(app);
    let max_height = (f64::from(area.size.height) / scale - SCREEN_MARGIN * 2.0).max(160.0);
    let height = snapshot.height().min(max_height);
    let width = WIDTH.min((f64::from(area.size.width) / scale - SCREEN_MARGIN * 2.0).max(1.0));
    let width_px = (width * scale).round() as i32;
    let height_px = (height * scale).round() as i32;
    let margin = (SCREEN_MARGIN * scale).round() as i32;
    let left = area.position.x;
    let top = area.position.y;
    let right = left + area.size.width as i32;
    let bottom = top + area.size.height as i32;
    let (menu_x, menu_y) = popup_origin(
        (x.round() as i32, y.round() as i32),
        (width_px, height_px),
        (left, top, right, bottom),
        (CONTENT_INSET * scale).round() as i32,
        margin,
    );

    window
        .set_size(LogicalSize::new(width, height))
        .map_err(|error| error.to_string())?;
    window
        .set_position(PhysicalPosition::new(menu_x, menu_y))
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::popup_origin;
    #[test]
    fn menu_visible_corner_starts_at_mouse_and_flips_only_near_edges() {
        assert_eq!(
            popup_origin((500, 300), (320, 532), (0, 0, 1920, 1080), 8, 8),
            (492, 292)
        );
        assert_eq!(
            popup_origin((1900, 1070), (320, 532), (0, 0, 1920, 1040), 8, 8),
            (1588, 500)
        );
    }
    #[test]
    fn placement_clamps_negative_monitor_coordinates_and_scaled_dimensions() {
        let pos = popup_origin((-5, -5), (400, 665), (-1280, -900, 0, 0), 10, 10);
        assert_eq!(pos, (-410, -675));
        assert_eq!(
            popup_origin((-1300, -920), (400, 665), (-1280, -900, 0, 0), 10, 10),
            (-1270, -890)
        );
    }
}

#[tauri::command]
pub(crate) fn tray_menu_snapshot(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
) -> Result<recent::Snapshot, String> {
    if window.label() != LABEL {
        return Err("窗口无权读取托盘菜单".into());
    }
    Ok(recent::snapshot(&app))
}

#[tauri::command]
pub(crate) fn tray_menu_dismiss(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
) -> Result<(), String> {
    if window.label() != LABEL {
        return Err("窗口无权关闭托盘菜单".into());
    }
    hide(&app);
    Ok(())
}

#[tauri::command]
pub(crate) fn tray_menu_activate(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    key: String,
) -> Result<(), String> {
    if window.label() != LABEL {
        return Err("窗口无权使用托盘菜单".into());
    }
    hide(&app);
    if key == "quit" {
        if let Err(error) = super::quit_launcher(app.clone()) {
            super::show(&app);
            let _ = app.emit("launcher-error", error);
        }
    } else {
        recent::activate(&app, &key);
    }
    Ok(())
}
