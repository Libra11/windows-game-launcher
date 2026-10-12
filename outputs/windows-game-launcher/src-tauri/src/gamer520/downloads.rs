use super::parser::{selector, text};
use reqwest::Url;
use scraper::{ElementRef, Html};
use serde::Serialize;
use std::collections::HashSet;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CloudLink {
    label: &'static str,
    pub(super) url: String,
    code: String,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DownloadInfo {
    pub(super) links: Vec<CloudLink>,
    pub(super) file_password: String,
    archive_password: String,
    pub(super) source_url: String,
    pub(super) error: String,
    notes: Vec<String>,
}

pub(super) fn cloud_url(value: &str) -> Option<(Url, &'static str)> {
    let url = Url::parse(value).ok()?;
    if url.scheme() != "https"
        || url.port().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return None;
    }
    let (label, prefix) = match url.host_str()? {
        "pan.baidu.com" => ("百度网盘", "/s/"),
        "pan.quark.cn" => ("夸克网盘", "/s/"),
        "pan.xunlei.com" => ("迅雷网盘", "/s/"),
        "drive.uc.cn" => ("UC 网盘", "/s/"),
        "www.alipan.com" | "www.aliyundrive.com" => ("阿里云盘", "/s/"),
        "gofile.io" => ("Gofile", "/d/"),
        "cloud.189.cn" => ("天翼云盘", "/t/"),
        _ => return None,
    };
    (url.path().starts_with(prefix) && url.path().len() > prefix.len()).then_some((url, label))
}

fn code(value: &str) -> Option<String> {
    let (_, tail) = value.split_once("提取码")?;
    let code = tail
        .trim_start_matches([' ', ':', '：'])
        .split_whitespace()
        .next()?;
    (!code.is_empty()
        && code.len() <= 12
        && code.bytes().all(|value| value.is_ascii_alphanumeric()))
    .then(|| code.to_owned())
}

fn associated_code(node: ElementRef<'_>) -> Option<String> {
    let parent = std::iter::once(node)
        .chain(node.ancestors().filter_map(ElementRef::wrap))
        .find(|node| {
            node.value().name() == "p"
                || node
                    .value()
                    .classes()
                    .any(|value| matches!(value, "bdp-card" | "wpkqcg_qrcode_wrapper"))
        })?;
    if let Some(value) = parent
        .select(&selector(".bdp-pwd-box"))
        .find_map(|field| code(&text(field)))
    {
        return Some(value);
    }
    if let Some(value) = code(&text(parent)) {
        return Some(value);
    }
    let next = parent
        .next_siblings()
        .filter_map(ElementRef::wrap)
        .find(|node| !text(*node).is_empty())?;
    let value = text(next);
    value.starts_with("提取码").then(|| code(&value)).flatten()
}

fn text_urls(value: &str) -> Vec<&str> {
    let starts = value
        .match_indices("https://")
        .map(|(start, _)| start)
        .collect::<Vec<_>>();
    starts
        .iter()
        .enumerate()
        .map(|(index, start)| {
            let tail = &value[*start..starts.get(index + 1).copied().unwrap_or(value.len())];
            let end = tail
                .find(|value: char| {
                    value.is_whitespace()
                        || matches!(
                            value,
                            '<' | '>'
                                | '"'
                                | '\''
                                | '，'
                                | '。'
                                | '；'
                                | '）'
                                | '】'
                                | '》'
                                | '('
                                | ')'
                                | '['
                                | ']'
                        )
                })
                .unwrap_or(tail.len());
            tail[..end].trim_end_matches(['.', ',', ';'])
        })
        .collect()
}

fn qr_share_url(node: ElementRef<'_>) -> Option<String> {
    ["data-src", "data-original", "src"]
        .into_iter()
        .find_map(|attribute| {
            let url = Url::parse(node.value().attr(attribute)?).ok()?;
            if url.scheme() != "https"
                || url.host_str() != Some("api.qrserver.com")
                || url.path() != "/v1/create-qr-code/"
                || url.port().is_some()
                || !url.username().is_empty()
                || url.password().is_some()
            {
                return None;
            }
            // 读取原站二维码的编码内容，不下载二维码图片，也不向生成服务提交分享链接。
            let mut values = url.query_pairs().filter(|(name, _)| name == "data");
            let value = values.next()?.1.into_owned();
            if values.next().is_some() || cloud_url(&value).is_none() {
                return None;
            }
            Some(value)
        })
}

pub(super) fn parse_content(content: ElementRef<'_>) -> DownloadInfo {
    let mut info = DownloadInfo::default();
    let mut seen = HashSet::new();
    for node in content.select(&selector(
        "a[href], input[id$='_content'][value], .bdp-qrcode-box img, p, li, pre, td",
    )) {
        let value = match node.value().name() {
            "input" => node.value().attr("value").unwrap_or("").to_owned(),
            "a" => node.value().attr("href").unwrap_or("").to_owned(),
            "img" => qr_share_url(node).unwrap_or_default(),
            _ => text(node),
        };
        let candidates = if matches!(node.value().name(), "a" | "input" | "img") {
            vec![value.as_str()]
        } else {
            text_urls(&value)
        };
        for value in candidates {
            let Some((url, label)) = cloud_url(value) else {
                continue;
            };
            if !seen.insert(url.to_string()) {
                continue;
            }
            let code = url
                .query_pairs()
                .find(|(key, _)| matches!(key.as_ref(), "pwd" | "passcode" | "password"))
                .map(|(_, value)| value.to_string())
                .filter(|value| !value.is_empty())
                .or_else(|| associated_code(node))
                .unwrap_or_default();
            info.links.push(CloudLink {
                label,
                url: url.to_string(),
                code,
            });
            if info.links.len() == 20 {
                break;
            }
        }
        if info.links.len() == 20 {
            break;
        }
    }
    for node in content.select(&selector("p, .bdp-unzip-badge")) {
        let value = text(node);
        if let Some((_, password)) = value.split_once("解压密码") {
            info.archive_password = password
                .trim_start_matches([' ', ':', '：'])
                .trim()
                .to_owned();
        } else if value.to_ascii_lowercase().contains("gofile")
            || value.to_ascii_lowercase().contains("onedrive")
            || value.contains("扫码保存")
            || value.contains("网盘提示")
        {
            info.notes.push(value.chars().take(500).collect());
        }
    }
    info
}

pub(super) fn parse_page(html: &str, url: &Url) -> Result<DownloadInfo, String> {
    let page = Html::parse_document(html);
    let id = url.path().trim_start_matches('/').trim_end_matches(".html");
    let article = page
        .select(&selector(&format!("article#post-{id}")))
        .next()
        .ok_or("原站下载页结构无法识别，请打开原站下载页查看")?;
    let content = article
        .select(&selector(".entry-content"))
        .next()
        .ok_or("原站下载页缺少正文")?;
    if content
        .select(&selector("form.post-password-form"))
        .next()
        .is_some()
    {
        return Err("原站下载页仍需要访问密码，请打开原站下载页查看".into());
    }
    let mut info = parse_content(content);
    info.source_url = url.to_string();
    if info.links.is_empty() {
        info.error = "未读取到支持的网盘地址，请打开原站下载页查看其他下载方式".into();
    }
    Ok(info)
}

pub(super) async fn load(value: &str, password: &str) -> Result<DownloadInfo, String> {
    super::download_fetch::load(value, password).await
}

#[tauri::command]
pub(crate) fn open_gamer520_resource(url: String) -> Result<(), String> {
    let url = resource_target(&url).ok_or("仅支持原站资源页或已识别的 HTTPS 网盘分享地址")?;
    crate::epic_auth::open_browser(url.as_str())
}

pub(super) fn resource_target(value: &str) -> Option<Url> {
    cloud_url(value)
        .map(|(url, _)| url)
        .or_else(|| super::download_fetch::resource_url(value))
        .or_else(|| super::download_fetch::entry_url(value))
        .or_else(|| super::detail::article_url(value))
}
