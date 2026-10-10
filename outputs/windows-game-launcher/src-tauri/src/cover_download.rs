use crate::network::{self, Service};

const MAX_BYTES: usize = 20 * 1024 * 1024;

pub(crate) fn extension(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("png")
    } else if bytes.starts_with(b"\xff\xd8\xff") {
        Some("jpg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("gif")
    } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        Some("webp")
    } else if bytes.get(4..8) == Some(b"ftyp")
        && matches!(bytes.get(8..12), Some(b"avif") | Some(b"avis"))
    {
        Some("avif")
    } else {
        None
    }
}

pub(crate) async fn fetch(url: &str, refresh: bool) -> Result<(Vec<u8>, &'static str), String> {
    let parsed = reqwest::Url::parse(url).map_err(|_| "封面地址无效")?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return Err("封面仅支持 HTTP 或 HTTPS 地址".into());
    }
    let mut request = network::client(Service::Image)?.get(parsed);
    if refresh {
        request = request.header(reqwest::header::CACHE_CONTROL, "no-cache");
    }
    let mut response = request.send().await.map_err(|_| "封面下载失败")?
        .error_for_status().map_err(|_| "封面服务器返回错误")?;
    if response.content_length().is_some_and(|size| size > MAX_BYTES as u64) {
        return Err("封面文件超过 20 MB".into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| "封面下载中断")? {
        if bytes.len() + chunk.len() > MAX_BYTES {
            return Err("封面文件超过 20 MB".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    let format = extension(&bytes).ok_or("服务器未返回支持的图片文件")?;
    Ok((bytes, format))
}
