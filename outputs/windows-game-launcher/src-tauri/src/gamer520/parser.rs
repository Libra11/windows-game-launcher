use super::{site_url, SearchItem, SearchResults};
use scraper::{ElementRef, Html, Selector};
use std::collections::HashSet;

pub(super) fn selector(value: &str) -> Selector {
    Selector::parse(value).expect("固定搜索页面选择器有效")
}

pub(super) fn text(node: ElementRef<'_>) -> String {
    fn collect(node: ElementRef<'_>, parts: &mut Vec<String>) {
        for child in node.children() {
            if let Some(element) = ElementRef::wrap(child) {
                if !matches!(
                    element.value().name(),
                    "script" | "style" | "iframe" | "form" | "noscript"
                ) {
                    if element.value().name() == "br" {
                        parts.push(" ".into());
                    } else {
                        collect(element, parts);
                    }
                }
            } else if let Some(value) = child.value().as_text() {
                parts.push(value.to_string());
            }
        }
    }
    let mut parts = Vec::new();
    collect(node, &mut parts);
    parts
        .join("")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

pub(super) fn parse(
    html: &str,
    query: &str,
    page: u32,
    search_url: &str,
) -> Result<SearchResults, String> {
    let document = Html::parse_document(html);
    let empty = document
        .select(&selector("body.search-no-results"))
        .next()
        .is_some();
    let heading = document.select(&selector("h1.term-title")).next().map(text);
    let expected_heading = format!("搜索：{query}")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if heading.as_deref() != Some(expected_heading.as_str())
        || document.select(&selector("body.search")).next().is_none()
    {
        return Err(
            "Gamer520 未返回有效搜索结果，可能需要浏览器验证或网站已改版，请打开原站查看".into(),
        );
    }
    let rows = selector("main.site-main .posts-wrapper article[id^='post-']");
    let title = selector(".entry-title a[href]");
    let image = selector(".entry-media img");
    let category = selector(".meta-category a");
    let time = selector("time[datetime]");
    let mut seen = HashSet::new();
    let mut items = Vec::new();
    let mut row_count = 0;
    for row in document.select(&rows) {
        row_count += 1;
        let Some(id) = row
            .value()
            .attr("id")
            .and_then(|id| id.strip_prefix("post-"))
        else {
            continue;
        };
        if id.is_empty() || !id.bytes().all(|byte| byte.is_ascii_digit()) {
            continue;
        }
        let Some(link) = row.select(&title).next() else {
            continue;
        };
        let Some(url) = link.value().attr("href").and_then(site_url) else {
            continue;
        };
        if url.path() != format!("/{id}.html") || url.query().is_some() || url.fragment().is_some()
        {
            continue;
        }
        let title = text(link);
        if title.is_empty() || !seen.insert(id.to_owned()) {
            continue;
        }
        let image = row
            .select(&image)
            .next()
            .and_then(|node| {
                ["data-src", "data-original", "src"]
                    .into_iter()
                    .find_map(|attribute| {
                        let value = node.value().attr(attribute)?;
                        let url = reqwest::Url::parse(search_url).ok()?.join(value).ok()?;
                        (matches!(url.scheme(), "http" | "https")
                            && url.host_str().is_some()
                            && url.username().is_empty()
                            && url.password().is_none())
                        .then(|| url.to_string())
                    })
            })
            .unwrap_or_default();
        items.push(SearchItem {
            id: id.to_owned(),
            title,
            url: url.to_string(),
            image,
            categories: row
                .select(&category)
                .map(text)
                .filter(|value| !value.is_empty())
                .collect(),
            is_pc: row
                .value()
                .classes()
                .any(|value| value == "category-pcplay"),
            published_at: row
                .select(&time)
                .next()
                .and_then(|node| node.value().attr("datetime"))
                .unwrap_or("")
                .to_owned(),
        });
    }
    if empty && row_count > 0 || !empty && (row_count == 0 || items.is_empty()) {
        return Err("Gamer520 搜索结果结构无法识别，请打开原站查看".into());
    }
    let has_next = page < 100
        && document
            .select(&selector(".numeric-pagination a.next[href]"))
            .any(|link| {
                link.value()
                    .attr("href")
                    .and_then(site_url)
                    .is_some_and(|url| {
                        url.path().trim_end_matches('/') == format!("/page/{}", page + 1)
                            && url
                                .query_pairs()
                                .any(|(name, value)| name == "s" && value == query)
                    })
            });
    Ok(SearchResults {
        query: query.to_owned(),
        page,
        items,
        has_next,
        search_url: search_url.to_owned(),
    })
}
