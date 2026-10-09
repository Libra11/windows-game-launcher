use crate::network::{self, Service};
use reqwest::{Client, Url};
use tauri::{
    http::{Request, Response},
    UriSchemeResponder,
};
use tokio::sync::Semaphore;

const MAX_IMAGE_BYTES: usize = 12 * 1024 * 1024;
static DOWNLOADS: Semaphore = Semaphore::const_new(8);

fn image_url(value: &str) -> Result<Url, String> {
    let url = Url::parse(value).map_err(|_| "图片地址无效")?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("图片地址无效".into());
    }
    Ok(url)
}

pub(crate) async fn fetch(client: &Client, source: &str) -> Result<(String, Vec<u8>), String> {
    let url = image_url(source)?;
    let mut response = client.get(url).send().await.map_err(|error| {
        if error.is_timeout() {
            "图片连接超时"
        } else {
            "图片连接失败"
        }
        .to_string()
    })?;
    if !response.status().is_success() {
        return Err(format!(
            "图片请求失败（HTTP {}）",
            response.status().as_u16()
        ));
    }
    if response
        .content_length()
        .is_some_and(|size| size > MAX_IMAGE_BYTES as u64)
    {
        return Err("图片超过读取限制".into());
    }
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .filter(|value| value.starts_with("image/"))
        .ok_or("响应不是图片")?
        .to_string();
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| "图片读取失败")? {
        if bytes.len() + chunk.len() > MAX_IMAGE_BYTES {
            return Err("图片超过读取限制".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    if bytes.is_empty() {
        return Err("图片内容为空".into());
    }
    Ok((content_type, bytes))
}

fn source(request: &Request<Vec<u8>>) -> Result<String, String> {
    if request.method() != tauri::http::Method::GET {
        return Err("不支持的图片请求".into());
    }
    let url = Url::parse(&request.uri().to_string()).map_err(|_| "图片请求无效")?;
    let value = url
        .query_pairs()
        .find(|(key, _)| key == "url")
        .map(|(_, value)| value.into_owned())
        .ok_or("缺少图片地址")?;
    image_url(&value)?;
    Ok(value)
}

pub(crate) fn handle(request: Request<Vec<u8>>, responder: UriSchemeResponder) {
    let source = source(&request);
    tauri::async_runtime::spawn(async move {
        let result = async {
            let source = source?;
            let _permit = DOWNLOADS.acquire().await.map_err(|_| "图片请求暂不可用")?;
            fetch(&network::client(Service::Image)?, &source).await
        }
        .await;
        let response = match result {
            Ok((content_type, bytes)) => Response::builder()
                .status(200)
                .header("Content-Type", content_type)
                .body(bytes),
            Err(_) => Response::builder()
                .status(502)
                .header("Content-Type", "text/plain; charset=utf-8")
                .body("图片加载失败".as_bytes().to_vec()),
        };
        if let Ok(mut response) = response {
            response
                .headers_mut()
                .insert("Cache-Control", "no-store".parse().unwrap());
            responder.respond(response);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
    };

    fn server(reply: Vec<u8>) -> (String, std::thread::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/image", listener.local_addr().unwrap());
        let thread = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            let mut request = [0; 4096];
            let size = stream.read(&mut request).unwrap();
            stream.write_all(&reply).unwrap();
            String::from_utf8_lossy(&request[..size]).into_owned()
        });
        (url, thread)
    }

    #[test]
    fn transfers_bytes_through_configured_http_proxy_without_direct_fallback() {
        let reply = b"HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: 4\r\nConnection: close\r\n\r\nPNG!".to_vec();
        let (proxy, thread) = server(reply);
        let proxy = Url::parse(&proxy).unwrap();
        let settings = network::ProxySettings {
            mode: network::ProxyMode::Custom,
            address: format!(
                "http://{}:{}",
                proxy.host_str().unwrap(),
                proxy.port().unwrap()
            ),
        }
        .validated()
        .unwrap();
        let client = network::build_client(&settings, Service::Image).unwrap();
        let (kind, bytes) =
            tauri::async_runtime::block_on(fetch(&client, "http://unreachable.invalid/icon.png"))
                .unwrap();
        assert_eq!(kind, "image/png");
        assert_eq!(bytes, b"PNG!");
        assert!(thread
            .join()
            .unwrap()
            .starts_with("GET http://unreachable.invalid/icon.png HTTP/1.1"));
    }

    #[test]
    fn direct_mode_ignores_saved_proxy_address_and_proxy_failure_is_reported() {
        let reply = b"HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: 4\r\nConnection: close\r\n\r\nPNG!".to_vec();
        let (url, thread) = server(reply);
        let settings = network::ProxySettings {
            mode: network::ProxyMode::Direct,
            address: "http://127.0.0.1:1".into(),
        };
        let client = network::build_client(&settings, Service::Image).unwrap();
        assert!(tauri::async_runtime::block_on(fetch(&client, &url)).is_ok());
        assert!(thread.join().unwrap().starts_with("GET /image HTTP/1.1"));
        let (proxy, thread) = server(
            b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
        );
        let mut proxy = Url::parse(&proxy).unwrap();
        proxy.set_path("");
        let settings = network::ProxySettings {
            mode: network::ProxyMode::Custom,
            address: proxy.to_string(),
        }
        .validated()
        .unwrap();
        let client = network::build_client(&settings, Service::Image).unwrap();
        assert!(tauri::async_runtime::block_on(fetch(
            &client,
            "http://unreachable.invalid/icon.png"
        ))
        .unwrap_err()
        .contains("502"));
        thread.join().unwrap();
    }

    #[test]
    fn account_and_public_clients_share_proxy_and_keep_redirects_disabled() {
        for service in [Service::Epic, Service::Public] {
            let (proxy, thread) = server(b"HTTP/1.1 302 Found\r\nLocation: http://unreachable.invalid/redirect\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec());
            let mut proxy = Url::parse(&proxy).unwrap();
            proxy.set_path("");
            let settings = network::ProxySettings {
                mode: network::ProxyMode::Custom,
                address: proxy.to_string(),
            }
            .validated()
            .unwrap();
            let client = network::build_client(&settings, service).unwrap();
            let response = tauri::async_runtime::block_on(async {
                client
                    .get("http://unreachable.invalid/account")
                    .send()
                    .await
            })
            .unwrap();
            assert_eq!(response.status().as_u16(), 302);
            assert!(thread
                .join()
                .unwrap()
                .starts_with("GET http://unreachable.invalid/account HTTP/1.1"));
        }
    }

    #[test]
    fn socks5_mode_transfers_images() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let settings = network::ProxySettings {
            mode: network::ProxyMode::Custom,
            address: format!("socks5://{}", listener.local_addr().unwrap()),
        }
        .validated()
        .unwrap();
        let thread = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            let mut greeting = [0; 2];
            stream.read_exact(&mut greeting).unwrap();
            assert_eq!(greeting[0], 5);
            let mut methods = vec![0; greeting[1] as usize];
            stream.read_exact(&mut methods).unwrap();
            stream.write_all(&[5, 0]).unwrap();
            let mut connect = [0; 4];
            stream.read_exact(&mut connect).unwrap();
            assert_eq!(&connect[..2], &[5, 1]);
            match connect[3] {
                1 => {
                    let mut address = [0; 6];
                    stream.read_exact(&mut address).unwrap();
                }
                3 => {
                    let mut length = [0];
                    stream.read_exact(&mut length).unwrap();
                    let mut address = vec![0; length[0] as usize + 2];
                    stream.read_exact(&mut address).unwrap();
                }
                kind => panic!("unexpected SOCKS address type {kind}"),
            }
            stream.write_all(&[5, 0, 0, 1, 127, 0, 0, 1, 0, 0]).unwrap();
            let mut request = [0; 4096];
            let size = stream.read(&mut request).unwrap();
            assert!(String::from_utf8_lossy(&request[..size]).starts_with("GET /icon.png HTTP/1.1"));
            stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: 4\r\nConnection: close\r\n\r\nPNG!").unwrap();
        });
        let client = network::build_client(&settings, Service::Image).unwrap();
        assert_eq!(
            tauri::async_runtime::block_on(fetch(&client, "http://127.0.0.1:4444/icon.png"))
                .unwrap()
                .1,
            b"PNG!"
        );
        thread.join().unwrap();
    }

    #[test]
    fn rejects_non_images_and_oversized_responses() {
        let client = Client::builder().no_proxy().build().unwrap();
        for reply in [
            b"HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: 1\r\nConnection: close\r\n\r\nx".to_vec(),
            format!("HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", MAX_IMAGE_BYTES + 1).into_bytes(),
        ] {
            let (url, thread) = server(reply);
            assert!(tauri::async_runtime::block_on(fetch(&client, &url)).is_err());
            thread.join().unwrap();
        }
    }

    #[test]
    fn accepts_encoded_image_url_but_rejects_local_file_and_credentials() {
        let request = Request::builder().uri("http://youji-image.localhost/?url=https%3A%2F%2Fexample.com%2Fa.png%3Fx%3D1%26y%3D2").body(Vec::new()).unwrap();
        assert_eq!(
            source(&request).unwrap(),
            "https://example.com/a.png?x=1&y=2"
        );
        assert!(image_url("file:///C:/secret").is_err());
        assert!(image_url("https://user:pass@example.com/a.png").is_err());
    }
}
