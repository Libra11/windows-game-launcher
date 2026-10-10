use crate::network::{ProxyMode, ProxySettings};
use tauri::{Manager, WebviewWindowBuilder, Wry};

// 与当前 Wry 的默认参数一致，显式设置代理时保留原有浏览器行为。
const DEFAULT_ARGUMENTS: &str = "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection --autoplay-policy=no-user-gesture-required";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum WebviewProxy {
    System,
    Direct,
    Custom(String),
}

impl From<&ProxySettings> for WebviewProxy {
    fn from(settings: &ProxySettings) -> Self {
        match settings.mode {
            ProxyMode::System => Self::System,
            ProxyMode::Direct => Self::Direct,
            ProxyMode::Custom => Self::Custom(settings.address.clone()),
        }
    }
}

impl WebviewProxy {
    fn arguments(&self) -> Option<String> {
        match self {
            Self::System => None,
            Self::Direct => Some(format!("{DEFAULT_ARGUMENTS} --no-proxy-server")),
            Self::Custom(address) => Some(format!("{DEFAULT_ARGUMENTS} --proxy-server={address}")),
        }
    }
}

pub(crate) fn configure<'a, M: Manager<Wry>>(
    app: &tauri::AppHandle,
    builder: WebviewWindowBuilder<'a, Wry, M>,
) -> WebviewWindowBuilder<'a, Wry, M> {
    // 主窗口与成就弹层共享同一启动配置和原有数据目录，避免参数冲突及丢失缓存。
    match app.state::<WebviewProxy>().arguments() {
        Some(arguments) => builder.additional_browser_args(&arguments),
        None => builder,
    }
}

pub(crate) fn restart_required(app: &tauri::AppHandle, settings: &ProxySettings) -> bool {
    *app.state::<WebviewProxy>() != WebviewProxy::from(settings)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proxy(mode: ProxyMode, address: &str) -> WebviewProxy {
        WebviewProxy::from(
            &ProxySettings {
                mode,
                address: address.into(),
            }
            .validated()
            .unwrap(),
        )
    }

    #[test]
    fn system_and_direct_modes_ignore_unused_custom_address() {
        assert_eq!(
            proxy(ProxyMode::System, ""),
            proxy(ProxyMode::System, "http://localhost:7890")
        );
        assert_eq!(
            proxy(ProxyMode::Direct, ""),
            proxy(ProxyMode::Direct, "http://localhost:7890")
        );
        assert!(proxy(ProxyMode::System, "").arguments().is_none());
        let arguments = proxy(ProxyMode::Direct, "").arguments().unwrap();
        assert!(arguments.contains("--no-proxy-server"));
        assert!(!arguments.contains("--proxy-server="));
    }

    #[test]
    fn custom_http_socks_ipv6_and_default_port_are_passed_to_webview() {
        for address in [
            "http://127.0.0.1:7890",
            "socks5://127.0.0.1:1080",
            "http://[::1]:7890",
            "http://localhost",
        ] {
            let arguments = proxy(ProxyMode::Custom, address).arguments().unwrap();
            assert!(
                arguments.contains(&format!("--proxy-server={address}")),
                "{arguments}"
            );
            assert!(!arguments.contains("--no-proxy-server"));
            assert!(arguments.contains("--disable-features="));
            assert!(arguments.contains("--autoplay-policy=no-user-gesture-required"));
        }
    }

    #[test]
    fn restart_depends_on_effective_configuration_and_clears_when_restored() {
        let active = proxy(ProxyMode::Custom, "http://localhost:7890");
        assert_eq!(active, proxy(ProxyMode::Custom, " http://localhost:7890/ "));
        assert_ne!(active, proxy(ProxyMode::Custom, "http://localhost:7891"));
        assert_ne!(active, proxy(ProxyMode::System, "http://localhost:7890"));
        assert_ne!(active, proxy(ProxyMode::Direct, "http://localhost:7890"));
        assert_eq!(active, proxy(ProxyMode::Custom, "http://localhost:7890"));
    }

    #[test]
    #[cfg(windows)]
    #[ignore = "需要 Windows WebView2，单独运行以验证原生窗口的代理连接"]
    fn native_windows_share_proxy_for_original_image_urls() {
        use std::{
            collections::HashSet,
            io::{Read, Write},
            net::TcpListener,
            sync::mpsc,
            time::{Duration, Instant},
        };
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let settings = ProxySettings {
            mode: ProxyMode::Custom,
            address: format!("http://{}", listener.local_addr().unwrap()),
        }
        .validated()
        .unwrap();
        let profile =
            std::env::temp_dir().join(format!("youji-webview-proxy-test-{}", uuid::Uuid::new_v4()));
        let (send, receive) = mpsc::channel();
        let app = tauri::Builder::default().any_thread().setup(move |app| {
            app.manage(WebviewProxy::from(&settings));
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                let deadline = Instant::now() + Duration::from_secs(20);
                let mut images = HashSet::new();
                while Instant::now() < deadline && images.len() < 2 {
                    let Ok((mut socket, _)) = listener.accept() else {
                        std::thread::sleep(Duration::from_millis(20));
                        continue;
                    };
                    let _ = socket.set_read_timeout(Some(Duration::from_secs(1)));
                    let mut request = [0; 4096];
                    let Ok(size) = socket.read(&mut request) else { continue };
                    let request = String::from_utf8_lossy(&request[..size]);
                    let url = request.lines().next().unwrap_or_default().split_whitespace().nth(1).unwrap_or_default();
                    let (content_type, body) = if let Some(path) = url.strip_prefix("http://youji-proxy.invalid/") {
                        if matches!(path, "main.gif" | "overlay.gif") {
                            images.insert(path.to_string());
                            ("image/gif", b"GIF89a\x01\x00\x01\x00\x80\x00\x00\x00\x00\x00\xff\xff\xff\x2c\x00\x00\x00\x00\x01\x00\x01\x00\x00\x02\x02\x44\x01\x00\x3b".to_vec())
                        } else {
                            let image = if path == "main.html" { "main.gif" } else { "overlay.gif" };
                            ("text/html", format!("<!doctype html><img src='http://youji-proxy.invalid/{image}'>").into_bytes())
                        }
                    } else { ("text/plain", Vec::new()) };
                    let header = format!("HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
                    let _ = socket.write_all(header.as_bytes());
                    let _ = socket.write_all(&body);
                }
                let _ = send.send(images);
                handle.exit(0);
            });
            let mut main_config = app.config().app.windows[0].clone();
            assert!(!main_config.create, "必须在读取代理后才创建主窗口");
            main_config.visible = false;
            main_config.fullscreen = false;
            main_config.url = tauri::WebviewUrl::External("http://youji-proxy.invalid/main.html".parse().unwrap());
            let main = WebviewWindowBuilder::from_config(app.handle(), &main_config)?.data_directory(profile.clone());
            configure(app.handle(), main).build()?;
            let overlay = WebviewWindowBuilder::new(app.handle(), "achievement-overlay", tauri::WebviewUrl::External("http://youji-proxy.invalid/overlay.html".parse().unwrap()))
                .visible(false).data_directory(profile.clone());
            configure(app.handle(), overlay).build()?;
            Ok(())
        }).build(tauri::generate_context!()).unwrap();
        assert_eq!(app.run_return(|_, _| {}), 0);
        let images = receive.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(
            images.len(),
            2,
            "主窗口与成就窗口的原始图片请求都应抵达所选代理：{images:?}"
        );
    }
}
