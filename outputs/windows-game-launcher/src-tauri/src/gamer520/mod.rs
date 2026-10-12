pub(crate) mod detail;
#[cfg(test)]
mod detail_tests;
mod download_fetch;
pub(crate) mod downloads;
mod fetch;
mod parser;
#[cfg(test)]
mod qr_tests;
#[cfg(test)]
mod tests;

use reqwest::Url;
use serde::Serialize;

const ORIGIN: &str = "https://www.gamer520.com/";
const MAX_HTML: usize = 2 * 1024 * 1024;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SearchItem {
    id: String,
    title: String,
    url: String,
    image: String,
    categories: Vec<String>,
    is_pc: bool,
    published_at: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SearchResults {
    query: String,
    page: u32,
    items: Vec<SearchItem>,
    has_next: bool,
    search_url: String,
}

fn search_url(query: &str, page: u32) -> Result<Url, String> {
    if query.is_empty() || query.chars().count() > 100 || query.chars().any(char::is_control) {
        return Err("请输入 1 至 100 个字符的游戏名称".into());
    }
    if !(1..=100).contains(&page) {
        return Err("搜索页码无效".into());
    }
    let mut url = Url::parse(ORIGIN).expect("固定网站地址有效");
    if page > 1 {
        url.set_path(&format!("/page/{page}"));
    }
    url.query_pairs_mut().append_pair("s", query);
    Ok(url)
}

fn site_url(value: &str) -> Option<Url> {
    let url = Url::parse(ORIGIN).ok()?.join(value).ok()?;
    (url.scheme() == "https"
        && url.host_str() == Some("www.gamer520.com")
        && url.port().is_none()
        && url.username().is_empty()
        && url.password().is_none())
    .then_some(url)
}

#[tauri::command]
pub(crate) async fn search_gamer520(query: String, page: u32) -> Result<SearchResults, String> {
    let query = query.trim();
    let url = search_url(query, page)?;
    let html = fetch::html(fetch::get(&url, None).await?).await?;
    parser::parse(&html, query, page, url.as_str())
}

#[tauri::command]
pub(crate) fn open_gamer520_page(url: String) -> Result<(), String> {
    let url = site_url(&url).ok_or("只能打开 Gamer520 的 HTTPS 页面")?;
    crate::epic_auth::open_browser(url.as_str())
}
