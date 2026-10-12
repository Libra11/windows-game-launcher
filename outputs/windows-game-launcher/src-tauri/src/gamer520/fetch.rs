use crate::network::{self, Service};
use reqwest::{Response, Url};

pub(super) async fn get(url: &Url, cookie: Option<&str>) -> Result<Response, String> {
    let mut request = network::client(Service::Public)?
        .get(url.clone())
        .header(reqwest::header::ACCEPT, "text/html")
        .header(reqwest::header::ACCEPT_LANGUAGE, "zh-CN,zh;q=0.9");
    if let Some(cookie) = cookie {
        request = request.header(reqwest::header::COOKIE, cookie);
    }
    request
        .send()
        .await
        .map_err(|_| "Gamer520 暂时无法连接，请检查网络或代理后重试".into())
}

pub(super) async fn html(mut response: Response) -> Result<String, String> {
    match response.status().as_u16() {
        200 => {}
        403 | 429 => {
            return Err("Gamer520 暂时拒绝请求或需要浏览器验证，请稍后重试或打开原站".into())
        }
        status => return Err(format!("Gamer520 页面读取失败（HTTP {status}）")),
    }
    if !response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.to_ascii_lowercase().contains("text/html"))
    {
        return Err("Gamer520 未返回网页内容，请打开原站查看".into());
    }
    if response
        .content_length()
        .is_some_and(|length| length > super::MAX_HTML as u64)
    {
        return Err("Gamer520 页面过大".into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "Gamer520 页面读取失败")?
    {
        if bytes.len() + chunk.len() > super::MAX_HTML {
            return Err("Gamer520 页面过大".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    String::from_utf8(bytes).map_err(|_| "Gamer520 页面编码无法识别".into())
}
