use crate::network::{self, Service};
use reqwest::{
    cookie::{CookieStore, Jar},
    header::{self, HeaderMap, HeaderValue},
    Response, StatusCode, Url,
};

const CONNECTION_ERROR: &str = "Gamer520 暂时无法连接，请检查网络或代理后重试";

pub(super) async fn get(url: &Url, cookie: Option<&str>) -> Result<Response, String> {
    let client = network::client(Service::Public)?;
    let request = || {
        client
            .get(url.clone())
            .header(header::ACCEPT, "text/html")
            .header(header::ACCEPT_LANGUAGE, "zh-CN,zh;q=0.9")
    };
    let first = match cookie {
        Some(cookie) => request().header(header::COOKIE, cookie),
        None => request(),
    };
    let response = first.send().await.map_err(|_| CONNECTION_ERROR)?;
    if cookie.is_none() {
        if let Some(session) = session_cookie(url, response.status(), response.headers()) {
            // 主站可能先初始化访客会话，再要求重新访问同一个页面。
            drop(response);
            return request()
                .header(header::COOKIE, session)
                .send()
                .await
                .map_err(|_| CONNECTION_ERROR.into());
        }
    }
    Ok(response)
}

fn session_cookie(url: &Url, status: StatusCode, headers: &HeaderMap) -> Option<HeaderValue> {
    if status != StatusCode::FORBIDDEN
        || super::site_url(url.as_str()).is_none()
        || headers.contains_key("cf-mitigated")
    {
        return None;
    }
    let jar = Jar::default();
    for cookie in headers.get_all(header::SET_COOKIE) {
        if let Ok(cookie) = cookie.to_str() {
            jar.add_cookie_str(cookie, url);
        }
    }
    let mut cookie = jar.cookies(url)?;
    let has_session = cookie.to_str().ok()?.split(';').any(|pair| {
        pair.trim_start()
            .split_once('=')
            .is_some_and(|(name, value)| name.starts_with("server_session_") && !value.is_empty())
    });
    cookie.set_sensitive(true);
    has_session.then_some(cookie)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retries_only_same_site_visitor_sessions() {
        let url = Url::parse("https://www.gamer520.com/?s=game").unwrap();
        let mut headers = HeaderMap::new();
        headers.append(
            header::SET_COOKIE,
            HeaderValue::from_static("server_session_test=visitor; Path=/; Secure; HttpOnly"),
        );
        headers.append(
            header::SET_COOKIE,
            HeaderValue::from_static("scoped=hidden; Path=/account"),
        );
        let cookie = session_cookie(&url, StatusCode::FORBIDDEN, &headers).unwrap();
        assert_eq!(cookie, "server_session_test=visitor");
        assert!(cookie.is_sensitive());
        assert!(session_cookie(&url, StatusCode::TOO_MANY_REQUESTS, &headers).is_none());
        let resource = Url::parse("https://520.gamers520.com/123.html").unwrap();
        assert!(session_cookie(&resource, StatusCode::FORBIDDEN, &headers).is_none());
        headers.insert("cf-mitigated", HeaderValue::from_static("challenge"));
        assert!(session_cookie(&url, StatusCode::FORBIDDEN, &headers).is_none());
    }
}
