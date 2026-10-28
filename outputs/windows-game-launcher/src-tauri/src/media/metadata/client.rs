//! TMDB请求、缓存与重试；业务匹配不在此层调整。
use super::store;
use serde_json::Value;
use std::{
    collections::HashMap,
    path::Path,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};
type Cache = HashMap<String, (Instant, Value)>;
static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
async fn request(
    dir: &Path,
    path: &str,
    query: &[(&str, String)],
) -> Result<Value, super::retry::Failure> {
    #[cfg(test)]
    if let Some(value) = super::tests::fixture(dir, path, query) {
        return value.map_err(Into::into);
    }
    if !path.starts_with('/') || path.contains("..") || path.contains('?') {
        return Err("资料接口路径无效".into());
    }
    let cache_key = format!("{path}:{query:?}");
    if let Some((time, value)) = CACHE
        .get_or_init(Default::default)
        .lock()
        .map_err(|_| "资料缓存不可用")?
        .get(&cache_key)
    {
        if time.elapsed() < Duration::from_secs(1800) {
            super::diagnostics::cache_hit();
            return Ok(value.clone());
        }
    }
    let bearer = store::credential(dir, "tmdbReadAccessToken")?;
    let key = if bearer.is_empty() {
        store::credential(dir, "tmdbApiKey")?
    } else {
        String::new()
    };
    if bearer.is_empty() && key.is_empty() {
        return Err("请先在影视设置中保存 TMDB API Key 或 Read Access Token".into());
    }
    let client = crate::network::client(crate::network::Service::Public)?;
    let mut request = client
        .get(format!("https://api.themoviedb.org/3{path}"))
        .query(query)
        .timeout(Duration::from_secs(20));
    if !query.iter().any(|(key, _)| *key == "language") {
        request = request.query(&[("language", "zh-CN")]);
    }
    if !bearer.is_empty() {
        request = request.bearer_auth(bearer);
    } else {
        request = request.query(&[("api_key", key)]);
    }
    let mut response = None;
    for attempt in 0..3 {
        super::diagnostics::network_attempt();
        let current = request.try_clone().ok_or("无法创建资料请求")?.send().await;
        super::diagnostics::http_status(
            dir,
            path,
            attempt as usize,
            current.as_ref().ok().map(|r| r.status().as_u16()),
        );
        match current {
            Ok(value)
                if (value.status().is_server_error() || value.status().as_u16() == 429)
                    && attempt < 2 => {}
            Ok(value) => {
                response = Some(value);
                break;
            }
            Err(_) if attempt < 2 => {}
            Err(_) => return Err("TMDB 连接失败，请检查网络和代理设置".into()),
        }
        tokio::time::sleep(Duration::from_millis(500 * (attempt + 1))).await;
    }
    let response = response.ok_or("TMDB 暂时无法提供资料")?;
    let status = response.status();
    if !status.is_success() {
        return Err(match status.as_u16() {
            401 | 403 => "TMDB 凭据无效或访问被拒绝，请检查设置",
            404 => "TMDB 中没有对应作品、季或集",
            429 => "TMDB 请求过于频繁，请稍后重试",
            _ => "TMDB 暂时无法提供资料",
        }
        .into());
    }
    if response
        .content_length()
        .is_some_and(|len| len > 4 * 1024 * 1024)
    {
        return Err("TMDB 资料响应过大".into());
    }
    let bytes = response.bytes().await.map_err(|_| "TMDB 响应读取失败")?;
    if bytes.len() > 4 * 1024 * 1024 {
        return Err("TMDB 资料响应过大".into());
    }
    let data: Value = serde_json::from_slice(&bytes).map_err(|_| "TMDB 资料格式无效")?;
    if let Ok(mut cache) = CACHE.get_or_init(Default::default).lock() {
        if cache.len() > 512 {
            cache.clear();
        }
        cache.insert(cache_key, (Instant::now(), data.clone()));
    }
    Ok(data)
}

pub async fn api(dir: &Path, path: &str, query: &[(&str, String)]) -> Result<Value, String> {
    api_typed(dir, path, query).await.map_err(Into::into)
}
pub(super) async fn api_typed(
    dir: &Path,
    path: &str,
    query: &[(&str, String)],
) -> Result<Value, super::retry::Failure> {
    let start = Instant::now();
    let result = request(dir, path, query).await;
    super::diagnostics::api_finished(dir, path, start.elapsed(), result.is_ok());
    result
}
pub(super) fn clear_cache() {
    if let Ok(mut cache) = CACHE.get_or_init(Default::default).lock() {
        cache.clear();
    }
}
