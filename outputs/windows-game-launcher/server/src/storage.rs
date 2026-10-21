// OSS 原生 V4。权限检查失败即拒绝签名，不回退到公开访问。
// STS token 是启动时读取的固定凭据，本模块不执行角色获取或自动刷新。
mod config;
mod security;
mod signing;
#[cfg(test)]
mod tests;
use crate::{ApiError, Result};
use chrono::Utc;
use reqwest::{Client, Method};
use std::{collections::BTreeMap, path::Path};
use url::Url;
pub struct Storage {
    object_prefix: String,
    public_endpoint: Url,
    service_endpoint: Url,
    bucket: String,
    region: String,
    access: String,
    secret: String,
    security_token: Option<String>,
    client: Client,
    pub quota: u64,
}
fn unavailable() -> ApiError {
    ApiError(
        axum::http::StatusCode::SERVICE_UNAVAILABLE,
        "私有 OSS 存档存储不可用".into(),
    )
}
fn client() -> Result<Client> {
    Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(900))
        .build()
        .map_err(|_| unavailable())
}
impl Storage {
    fn object_key(&self, key: &str) -> Result<String> {
        if key.is_empty()
            || key.starts_with('/')
            || key
                .split('/')
                .any(|p| p.is_empty() || p == "." || p == "..")
            || key.contains('\\')
            || key.chars().any(char::is_control)
        {
            return Err(unavailable());
        }
        Ok(if self.object_prefix.is_empty() {
            key.to_owned()
        } else {
            format!("{}/{}", self.object_prefix, key)
        })
    }
    pub fn from_env() -> Result<Option<Self>> {
        config::read(|key| std::env::var(key).ok())
    }
    #[cfg(test)]
    pub(crate) fn test(endpoint: String, quota: u64) -> Self {
        // 只有测试构造器允许直连本机 HTTP；生产配置严格使用 virtual hosted HTTPS。
        let endpoint = Url::parse(&endpoint).unwrap();
        Self {
            object_prefix: String::new(),
            public_endpoint: endpoint.clone(),
            service_endpoint: endpoint,
            bucket: "test".into(),
            region: "cn-hangzhou".into(),
            access: "test".into(),
            secret: "test".into(),
            security_token: None,
            client: client().unwrap_or_else(|_| panic!("test client")),
            quota,
        }
    }
    pub async fn upload_url(
        &self,
        key: &str,
        bytes: u64,
    ) -> Result<(String, BTreeMap<String, String>)> {
        let key = self.object_key(key)?;
        let mut headers = self.security_headers().await?;
        headers.insert("content-length".into(), bytes.to_string());
        headers.insert("content-type".into(), "application/octet-stream".into());
        self.presign(
            &self.public_endpoint,
            signing::Request {
                method: "PUT",
                key: &key,
                query: &[],
                expires: 600,
                headers,
            },
            Utc::now(),
        )
    }
    pub async fn download_url(&self, key: &str) -> Result<String> {
        let key = self.object_key(key)?;
        self.check_private().await?;
        Ok(self
            .presign(
                &self.public_endpoint,
                signing::Request {
                    method: "GET",
                    key: &key,
                    query: &[],
                    expires: 600,
                    headers: BTreeMap::new(),
                },
                Utc::now(),
            )?
            .0)
    }
    pub async fn fetch(&self, key: &str, path: &Path, expected: u64) -> Result<()> {
        let key = self.object_key(key)?;
        use tokio::io::AsyncWriteExt;
        let (url, _) = self.presign(
            &self.service_endpoint,
            signing::Request {
                method: "GET",
                key: &key,
                query: &[],
                expires: 600,
                headers: BTreeMap::new(),
            },
            Utc::now(),
        )?;
        let mut response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|_| unavailable())?;
        if !response.status().is_success() {
            return Err(unavailable());
        }
        let mut file = tokio::fs::File::create(path)
            .await
            .map_err(|_| unavailable())?;
        let mut size = 0u64;
        while let Some(chunk) = response.chunk().await.map_err(|_| unavailable())? {
            size = size
                .checked_add(chunk.len() as u64)
                .filter(|n| *n <= expected)
                .ok_or_else(|| ApiError::bad("对象超出预留大小"))?;
            file.write_all(&chunk).await.map_err(|_| unavailable())?;
        }
        if size != expected {
            return Err(ApiError::bad("对象大小不匹配"));
        }
        file.sync_all().await.map_err(|_| unavailable())?;
        Ok(())
    }
    pub async fn publish(&self, key: &str, path: &Path, size: u64) -> Result<()> {
        let key = self.object_key(key)?;
        let mut headers = self.security_headers().await?;
        headers.insert("content-length".into(), size.to_string());
        headers.insert("content-type".into(), "application/octet-stream".into());
        let (url, headers) = self.presign(
            &self.service_endpoint,
            signing::Request {
                method: "PUT",
                key: &key,
                query: &[],
                expires: 600,
                headers,
            },
            Utc::now(),
        )?;
        let file = tokio::fs::File::open(path)
            .await
            .map_err(|_| unavailable())?;
        let mut request = self.client.put(url);
        for (k, v) in headers {
            request = request.header(k, v)
        }
        let response = request
            .body(reqwest::Body::wrap_stream(
                tokio_util::io::ReaderStream::new(file),
            ))
            .send()
            .await
            .map_err(|_| unavailable())?;
        if !response.status().is_success() {
            return Err(unavailable());
        }
        Ok(())
    }
    pub async fn delete(&self, key: &str) -> Result<()> {
        let key = self.object_key(key)?;
        let (url, _) = self.presign(
            &self.service_endpoint,
            signing::Request {
                method: "DELETE",
                key: &key,
                query: &[],
                expires: 60,
                headers: BTreeMap::new(),
            },
            Utc::now(),
        )?;
        let response = self
            .client
            .request(Method::DELETE, url)
            .send()
            .await
            .map_err(|_| unavailable())?;
        if !response.status().is_success() {
            return Err(unavailable());
        }
        Ok(())
    }
}
