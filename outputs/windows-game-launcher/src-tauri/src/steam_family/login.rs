use super::auth;
use std::sync::{atomic::{AtomicBool, Ordering}, Arc};
use tauri::{Emitter, Manager};

pub(crate) const WINDOW_LABEL: &str = "steam-family-login";
const LOGIN_URL: &str = "https://store.steampowered.com/login/?redir=account%2Ffamilymanagement&l=schinese";

fn event(app: &tauri::AppHandle, state: &str, message: &str) {
    let _ = app.emit_to("main", "steam-family-auth", serde_json::json!({"state":state,"message":message}));
}

fn allowed(url: &reqwest::Url) -> bool {
    url.scheme() == "https" && url.port_or_known_default() == Some(443) && matches!(url.host_str(),
        Some("store.steampowered.com" | "login.steampowered.com" | "steamcommunity.com" | "help.steampowered.com"))
}

pub(crate) async fn begin(app: &tauri::AppHandle) -> Result<(), String> {
    let _guard = auth::AUTH_LOCK.lock().await;
    let url = reqwest::Url::parse(LOGIN_URL).map_err(|_| "Steam 登录地址无效")?;
    if let Some(window) = app.get_webview_window(WINDOW_LABEL) {
        window.navigate(url).map_err(|_| "无法重新打开 Steam 登录页面")?;
        let _ = window.show();
        return window.set_focus().map_err(|_| "无法打开 Steam 登录窗口".into());
    }
    let generation = auth::LOGIN_GENERATION.fetch_add(1, Ordering::AcqRel) + 1;
    let nonce = uuid::Uuid::new_v4().to_string();
    let script = include_str!("login.js").replace("__LOGIN_NONCE_JSON__", &serde_json::to_string(&nonce).map_err(|_| "无法生成登录验证")?);
    let processing = Arc::new(AtomicBool::new(false));
    let completed = Arc::new(AtomicBool::new(false));
    let navigation_app = app.clone();
    let done = completed.clone();
    let builder = tauri::WebviewWindowBuilder::new(app, WINDOW_LABEL, tauri::WebviewUrl::External(url))
        .title("登录 Steam · 家庭游戏库").inner_size(1050.0, 760.0).min_inner_size(720.0, 580.0)
        .initialization_script(script.clone())
        .on_navigation(move |url| {
            if url.scheme() != "youji-steam-family" { return allowed(url); }
            if url.host_str() != Some("complete") { return false; }
            let fields = url.query_pairs().collect::<std::collections::HashMap<_, _>>();
            if fields.get("state").map(|value| value.as_ref()) != Some(nonce.as_str()) { return false; }
            if fields.contains_key("error") {
                // 页面只返回固定错误类别，不接受网页生成的任意正文。
                event(&navigation_app, "error", "Steam 登录授权读取失败或等待超时，请重试");
                return false;
            }
            let Some(token) = fields.get("token").map(|value| value.to_string()) else { return false };
            if processing.swap(true, Ordering::AcqRel) { return false; }
            let app = navigation_app.clone();
            let done = done.clone();
            let pending = processing.clone();
            tauri::async_runtime::spawn(async move {
                match auth::complete(&app, token, generation).await {
                    Ok(()) => {
                        done.store(true, Ordering::Release);
                        event(&app, "connected", "Steam 家庭库已连接");
                        if let Some(window) = app.get_webview_window(WINDOW_LABEL) { let _ = window.close(); }
                    }
                    Err(error) => event(&app, "error", &error),
                }
                pending.store(false, Ordering::Release);
            });
            false
        })
        .on_page_load(move |window, payload| {
            if matches!(payload.event(), tauri::webview::PageLoadEvent::Finished)
                && allowed(payload.url())
                && payload.url().host_str() == Some("store.steampowered.com")
            {
                if window.eval(&script).is_err() { event(window.app_handle(), "error", "无法读取 Steam 登录结果，请重试"); }
            }
        });
    // 此远程窗口不加入任何应用 IPC capability；仅通过受 nonce 保护的导航回调交接。
    let window = crate::webview_proxy::configure(app, builder).build().map_err(|_| "无法打开 Steam 官方登录窗口")?;
    let close_app = app.clone();
    window.on_window_event(move |event_type| {
        if matches!(event_type, tauri::WindowEvent::Destroyed) && !completed.load(Ordering::Acquire) {
            auth::LOGIN_GENERATION.fetch_add(1, Ordering::AcqRel);
            event(&close_app, "cancelled", "Steam 登录已取消");
        }
    });
    Ok(())
}
