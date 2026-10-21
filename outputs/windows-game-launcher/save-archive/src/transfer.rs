//! 可选的有界对象传输；服务端只依赖归档校验时不启用此模块。
use std::{collections::BTreeMap, path::Path, time::Duration};
use tokio::io::AsyncWriteExt;
fn client(url: &str) -> Result<reqwest::Client, String> {
    let parsed = reqwest::Url::parse(url).map_err(|_| "对象地址无效")?;
    if parsed.scheme() != "https" || !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("对象存储必须使用 HTTPS".into());
    }
    reqwest::Client::builder().redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(30)).timeout(Duration::from_secs(1800))
        .build().map_err(|_| "无法创建对象传输连接".into())
}
pub async fn upload(url: &str, headers: &BTreeMap<String,String>, path: &Path) -> Result<(),String> {
    let client = client(url)?;
    let file = tokio::fs::File::open(path).await.map_err(|e| e.to_string())?;
    let len = file.metadata().await.map_err(|e| e.to_string())?.len();
    if len > crate::MAX_BYTES { return Err("上传归档超过 2 GiB".into()); }
    let signed_length = validate_length(headers, len)?;
    let mut request = client.put(url);
    for (key,value) in headers {
        request = request.header(key, value);
    }
    if !signed_length { request = request.header(reqwest::header::CONTENT_LENGTH, len); }
    let response = request.body(reqwest::Body::from(file)).send().await.map_err(|_| "存档上传连接失败，请重试".to_string())?;
    if !response.status().is_success() { return Err(format!("存档上传失败（HTTP {}）", response.status().as_u16())); }
    Ok(())
}
pub async fn download(url: &str, path: &Path) -> Result<(),String> {
    let mut response = client(url)?.get(url).send().await.map_err(|_| "存档下载连接失败".to_string())?;
    if !response.status().is_success() { return Err(format!("存档下载失败（HTTP {}）",response.status().as_u16())); }
    if response.content_length().is_some_and(|n| n > crate::MAX_BYTES) { return Err("下载存档超过 2 GiB".into()); }
    let mut file = tokio::fs::OpenOptions::new().create_new(true).write(true).open(path).await.map_err(|e| e.to_string())?;
    let mut total = 0u64;
    while let Some(bytes) = response.chunk().await.map_err(|_| "存档下载中断".to_string())? {
        total = total.checked_add(bytes.len() as u64).filter(|n| *n <= crate::MAX_BYTES).ok_or("下载存档超过 2 GiB")?;
        file.write_all(&bytes).await.map_err(|e| e.to_string())?;
    }
    file.sync_all().await.map_err(|e| e.to_string())?;
    Ok(())
}

fn validate_length(headers: &BTreeMap<String,String>, bytes: u64) -> Result<bool,String> {
    let lengths:Vec<&String> = headers.iter().filter(|(k,_)| k.eq_ignore_ascii_case("content-length")).map(|(_,v)|v).collect();
    if lengths.len() > 1 || lengths.first().is_some_and(|v| v.parse::<u64>().ok()!=Some(bytes)) {
        return Err("签名上传大小与本地归档不一致".into());
    }
    Ok(!lengths.is_empty())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn signed_length_must_match_without_duplicate_headers() {
        assert!(!validate_length(&BTreeMap::new(),12).unwrap());
        assert!(validate_length(&[("Content-Length".into(),"12".into())].into(),12).unwrap());
        assert!(validate_length(&[("content-length".into(),"13".into())].into(),12).is_err());
        assert!(validate_length(&[("content-length".into(),"12".into()),("Content-Length".into(),"12".into())].into(),12).is_err());
    }
}
