use super::{
    downloads, fetch,
    parser::{selector, text},
    site_url,
};
use reqwest::Url;
use scraper::{ElementRef, Html};
use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ContentBlock {
    kind: &'static str,
    text: String,
    image: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GameDetail {
    id: String,
    title: String,
    url: String,
    cover: String,
    categories: Vec<String>,
    blocks: Vec<ContentBlock>,
    pub(super) downloads: downloads::DownloadInfo,
}

pub(super) fn article_url(value: &str) -> Option<Url> {
    let url = site_url(value)?;
    let id = url.path().strip_prefix('/')?.strip_suffix(".html")?;
    (!id.is_empty()
        && id.bytes().all(|value| value.is_ascii_digit())
        && url.query().is_none()
        && url.fragment().is_none())
    .then_some(url)
}

pub(super) fn image_url(node: ElementRef<'_>, base: &Url) -> Option<String> {
    ["data-src", "data-original", "src"]
        .into_iter()
        .find_map(|attribute| {
            let url = base.join(node.value().attr(attribute)?).ok()?;
            (matches!(url.scheme(), "http" | "https")
                && url.host_str().is_some()
                && url.username().is_empty()
                && url.password().is_none())
            .then(|| url.to_string())
        })
}

fn blocks(node: ElementRef<'_>, base: &Url, output: &mut Vec<ContentBlock>) {
    if output.len() >= 150 {
        return;
    }
    let name = node.value().name();
    if matches!(
        name,
        "script" | "style" | "iframe" | "form" | "input" | "button" | "noscript"
    ) {
        return;
    }
    if name == "img" {
        if let Some(image) = image_url(node, base) {
            output.push(ContentBlock {
                kind: "image",
                text: String::new(),
                image,
            });
        }
        return;
    }
    if matches!(
        name,
        "h1" | "h2" | "h3" | "h4" | "p" | "li" | "blockquote" | "pre" | "tr"
    ) {
        let value = text(node);
        if !value.is_empty() {
            output.push(ContentBlock {
                kind: if name.starts_with('h') {
                    "heading"
                } else {
                    "text"
                },
                text: value,
                image: String::new(),
            });
        }
        for image in node.select(&selector("img")) {
            blocks(image, base, output);
        }
        return;
    }
    for child in node.children().filter_map(ElementRef::wrap) {
        blocks(child, base, output);
    }
}

pub(super) fn parse(html: &str, url: &Url) -> Result<GameDetail, String> {
    let id = url.path().trim_start_matches('/').trim_end_matches(".html");
    let page = Html::parse_document(html);
    if page
        .select(&selector(&format!("body.postid-{id}")))
        .next()
        .is_none()
    {
        return Err("原站未返回对应游戏详情，可能需要浏览器验证，请打开原站查看".into());
    }
    let article = page
        .select(&selector(&format!("article#post-{id}")))
        .next()
        .ok_or("游戏详情结构无法识别")?;
    let title = article
        .select(&selector("h1.entry-title"))
        .next()
        .map(text)
        .filter(|value| !value.is_empty())
        .ok_or("游戏详情缺少标题")?;
    let content = article
        .select(&selector(".entry-content"))
        .next()
        .ok_or("游戏详情缺少正文")?;
    if content
        .select(&selector("form.post-password-form"))
        .next()
        .is_some()
    {
        return Err("游戏详情需要原站访问密码，请打开原站查看".into());
    }
    let cover = content
        .select(&selector("img"))
        .find_map(|node| image_url(node, url))
        .unwrap_or_default();
    let mut body = Vec::new();
    blocks(content, url, &mut body);
    let file_password = page
        .select(&selector(".widget-pay .pwd [data-clipboard-text]"))
        .next()
        .and_then(|node| node.value().attr("data-clipboard-text"))
        .unwrap_or("")
        .to_owned();
    let source_url = page
        .select(&selector(".widget-pay a.go-downblank[href]"))
        .find_map(|node| {
            let target = node.value().attr("href").and_then(site_url)?;
            (target.path() == "/go"
                && target
                    .query_pairs()
                    .any(|(key, value)| key == "post_id" && value == id))
            .then(|| target.to_string())
        })
        .unwrap_or_else(|| url.to_string());
    let mut download_info = downloads::parse_content(content);
    download_info.file_password = file_password;
    download_info.source_url = source_url;
    Ok(GameDetail {
        id: id.to_owned(),
        title,
        url: url.to_string(),
        cover,
        categories: article
            .select(&selector(".entry-meta a[rel='category']"))
            .map(text)
            .collect(),
        blocks: body,
        downloads: download_info,
    })
}

#[tauri::command]
pub(crate) async fn get_gamer520_detail(url: String) -> Result<GameDetail, String> {
    let url = article_url(&url).ok_or("请选择有效的 Gamer520 游戏条目")?;
    let html = fetch::html(fetch::get(&url, None).await?).await?;
    let mut detail = parse(&html, &url)?;
    if detail.downloads.source_url != url.as_str() {
        match downloads::load(
            &detail.downloads.source_url,
            &detail.downloads.file_password,
        )
        .await
        {
            Ok(mut info) => {
                info.file_password = detail.downloads.file_password;
                detail.downloads = info;
            }
            Err(error) => detail.downloads.error = error,
        }
    }
    Ok(detail)
}
