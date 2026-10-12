use super::{downloads, fetch, parser::selector, site_url};
use crate::network::{self, Service};
use reqwest::Url;
use scraper::Html;

pub(super) fn resource_url(value: &str) -> Option<Url> {
    let url = Url::parse(value).ok()?;
    let id = url.path().strip_prefix('/')?.strip_suffix(".html")?;
    (url.scheme() == "https"
        && matches!(
            url.host_str(),
            Some("520.gamers520.com") | Some("vip.gamers520.com")
        )
        && url.port().is_none()
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
        && !id.is_empty()
        && id.bytes().all(|value| value.is_ascii_digit()))
    .then_some(url)
}

pub(super) fn entry_url(value: &str) -> Option<Url> {
    site_url(value).filter(|url| {
        url.path() == "/go"
            && url.fragment().is_none()
            && url.query_pairs().count() == 1
            && url.query_pairs().any(|(name, value)| {
                name == "post_id"
                    && !value.is_empty()
                    && value.bytes().all(|value| value.is_ascii_digit())
            })
    })
}

fn script_destination(html: &str) -> Option<Url> {
    let page = Html::parse_document(html);
    page.select(&selector("script")).find_map(|node| {
        let value = node.text().collect::<String>();
        let (_, tail) = value.split_once("window.location=")?;
        let tail = tail.trim_start();
        let quote = tail.chars().next()?;
        if !matches!(quote, '\'' | '"') {
            return None;
        }
        let target = tail[1..].split(quote).next()?;
        resource_url(target)
    })
}

fn password_form(html: &str, url: &Url) -> bool {
    let page = Html::parse_document(html);
    page.select(&selector("form.post-password-form[method='post']"))
        .any(|form| {
            let expected = format!(
                "https://{}/wp-login.php?action=postpass",
                url.host_str().unwrap_or("")
            );
            form.value().attr("action") == Some(expected.as_str())
                && form
                    .select(&selector("input[name='redirect_to']"))
                    .any(|node| node.value().attr("value") == Some(url.as_str()))
        })
}

async fn unlock(html: String, url: &Url, password: &str) -> Result<String, String> {
    if !password_form(&html, url) {
        return Ok(html);
    }
    if password.is_empty() || password.len() > 100 {
        return Err("原站未提供可用访问密码，请打开原站下载页查看".into());
    }
    let endpoint = format!(
        "https://{}/wp-login.php?action=postpass",
        url.host_str().unwrap()
    );
    // 仅提交游戏主站公开的文件密码；站点响应凭据仅用于本次同域读取，不持久化、不返回前端。
    let response = network::client(Service::Public)?
        .post(endpoint)
        .header(reqwest::header::REFERER, url.as_str())
        .form(&[("post_password", password), ("redirect_to", url.as_str())])
        .send()
        .await
        .map_err(|_| "原站下载密码验证暂时无法连接")?;
    if !response.status().is_redirection()
        || response
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| url.join(value).ok())
            .as_ref()
            != Some(url)
    {
        return Err("原站下载密码验证未完成，请打开原站下载页查看".into());
    }
    let cookie = response
        .headers()
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .filter_map(|value| value.split(';').next())
        .find(|value| value.starts_with("wp-postpass_"))
        .ok_or("原站未确认访问密码，请打开原站下载页查看")?
        .to_owned();
    fetch::html(fetch::get(url, Some(&cookie)).await?).await
}

pub(super) async fn load(value: &str, password: &str) -> Result<downloads::DownloadInfo, String> {
    let source = entry_url(value).ok_or("原站下载入口无效")?;
    let response = fetch::get(&source, None).await?;
    let target = if response.status().is_redirection() {
        response
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|value| value.to_str().ok())
            .and_then(resource_url)
    } else {
        script_destination(&fetch::html(response).await?)
    }
    .ok_or("原站下载跳转无法识别，请打开原站查看")?;
    let html = fetch::html(fetch::get(&target, None).await?).await?;
    let html = unlock(html, &target, password).await?;
    downloads::parse_page(&html, &target)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepts_only_observed_resource_domains_and_literal_redirects() {
        assert!(script_destination(
            "<script>window.location='https://520.gamers520.com/13352.html';</script>"
        )
        .is_some());
        for value in [
            "https://evil.example/1.html",
            "http://520.gamers520.com/1.html",
            "https://520.gamers520.com/wp-login.php",
            "https://user@520.gamers520.com/1.html",
        ] {
            assert!(resource_url(value).is_none());
        }
        assert!(script_destination(
            "<script>window.location='https://evil.example/1.html';</script>"
        )
        .is_none());
    }
}
