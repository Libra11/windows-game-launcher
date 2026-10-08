#[cfg(windows)]
pub(super) fn place(
    window: &tauri::WebviewWindow,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) -> Result<(), String> {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn ShowWindow(hwnd: *mut std::ffi::c_void, command: i32) -> i32;
        fn IsZoomed(hwnd: *mut std::ffi::c_void) -> i32;
        fn SetWindowPos(
            hwnd: *mut std::ffi::c_void,
            after: *mut std::ffi::c_void,
            x: i32,
            y: i32,
            width: i32,
            height: i32,
            flags: u32,
        ) -> i32;
    }
    let hwnd = window.hwnd().map_err(|e| e.to_string())?;
    unsafe {
        // 掌机窗口管理器可能把提示窗口最大化；先恢复普通状态且不抢焦点。
        if IsZoomed(hwnd.0 as _) != 0 {
            ShowWindow(hwnd.0 as _, 4);
        }
        // 位置、物理尺寸和 Z 序一并应用，避免异步窗口消息重新覆盖坐标。
        if SetWindowPos(
            hwnd.0 as _,
            -1isize as _,
            x,
            y,
            width,
            height,
            0x0010 | 0x0200,
        ) == 0
        {
            return Err(std::io::Error::last_os_error().to_string());
        }
    }
    Ok(())
}
#[cfg(not(windows))]
pub(super) fn place(
    window: &tauri::WebviewWindow,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) -> Result<(), String> {
    window
        .set_size(tauri::PhysicalSize::new(width as u32, height as u32))
        .map_err(|e| e.to_string())?;
    window
        .set_position(tauri::PhysicalPosition::new(x, y))
        .map_err(|e| e.to_string())
}
#[cfg(windows)]
pub(super) fn raise(window: &tauri::WebviewWindow) -> Result<(), String> {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn SetWindowPos(
            hwnd: *mut std::ffi::c_void,
            after: *mut std::ffi::c_void,
            x: i32,
            y: i32,
            width: i32,
            height: i32,
            flags: u32,
        ) -> i32;
    }
    let hwnd = window.hwnd().map_err(|e| e.to_string())?;
    // 即便置顶属性未变也刷新 Z 序；NOACTIVATE 防止打断游戏输入。
    let result = unsafe {
        SetWindowPos(
            hwnd.0 as _,
            -1isize as _,
            0,
            0,
            0,
            0,
            0x0001 | 0x0002 | 0x0010 | 0x4000,
        )
    };
    if result == 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(())
}
#[cfg(not(windows))]
pub(super) fn raise(window: &tauri::WebviewWindow) -> Result<(), String> {
    window.set_always_on_top(true).map_err(|e| e.to_string())
}
#[cfg(windows)]
pub(super) fn foreground_point() -> Option<(f64, f64)> {
    #[repr(C)]
    struct Rect {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }
    #[link(name = "user32")]
    unsafe extern "system" {
        fn GetForegroundWindow() -> *mut std::ffi::c_void;
        fn GetWindowRect(hwnd: *mut std::ffi::c_void, rect: *mut Rect) -> i32;
    }
    let mut rect = Rect {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_null() || GetWindowRect(hwnd, &mut rect) == 0 {
            return None;
        }
    }
    Some((
        (rect.left as f64 + rect.right as f64) / 2.0,
        (rect.top as f64 + rect.bottom as f64) / 2.0,
    ))
}
#[cfg(not(windows))]
pub(super) fn foreground_point() -> Option<(f64, f64)> {
    None
}
#[cfg(windows)]
pub(super) fn sound(name: &str) {
    #[link(name = "winmm")]
    unsafe extern "system" {
        fn PlaySoundW(sound: *const u8, module: *mut std::ffi::c_void, flags: u32) -> i32;
    }
    let bytes: &'static [u8] = match name {
        "chime" => include_bytes!("../resources/achievement-chime.wav"),
        "soft" => include_bytes!("../resources/achievement-soft.wav"),
        _ => return,
    };
    // 异步播放时缓冲区必须存活；内嵌的静态字节不会释放。
    unsafe {
        PlaySoundW(
            bytes.as_ptr(),
            std::ptr::null_mut(),
            0x0004 | 0x0001 | 0x0002,
        );
    }
}
#[cfg(not(windows))]
pub(super) fn sound(_: &str) {}

#[cfg(windows)]
pub(super) fn visibility(window: &tauri::WebviewWindow, visible: bool) -> Result<(), String> {
    // 原生 HWND 和 WebView 控件的可见性分别管理，后台显示时也必须恢复网页绘制。
    let webview: &tauri::Webview = window.as_ref();
    if visible {
        webview.show()
    } else {
        webview.hide()
    }
    .map_err(|error| error.to_string())?;
    #[link(name = "user32")]
    unsafe extern "system" {
        fn SetWindowPos(
            hwnd: *mut std::ffi::c_void,
            after: *mut std::ffi::c_void,
            x: i32,
            y: i32,
            width: i32,
            height: i32,
            flags: u32,
        ) -> i32;
    }
    let hwnd = window.hwnd().map_err(|e| e.to_string())?;
    // Tauri 的 show 可能激活窗口；直接显示/隐藏，并明确保留游戏焦点。
    let flags = 0x0001 | 0x0002 | 0x0010 | 0x0200 | if visible { 0x0040 } else { 0x0080 };
    if unsafe { SetWindowPos(hwnd.0 as _, -1isize as _, 0, 0, 0, 0, flags) } == 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(())
}
#[cfg(not(windows))]
pub(super) fn visibility(window: &tauri::WebviewWindow, visible: bool) -> Result<(), String> {
    if visible {
        window.show()
    } else {
        window.hide()
    }
    .map_err(|e| e.to_string())
}
#[cfg(windows)]
pub(super) fn is_visible(window: &tauri::WebviewWindow) -> Result<bool, String> {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn IsWindowVisible(hwnd: *mut std::ffi::c_void) -> i32;
    }
    let hwnd = window.hwnd().map_err(|e| e.to_string())?;
    Ok(unsafe { IsWindowVisible(hwnd.0 as _) != 0 })
}
#[cfg(not(windows))]
pub(super) fn is_visible(window: &tauri::WebviewWindow) -> Result<bool, String> {
    window.is_visible().map_err(|e| e.to_string())
}
