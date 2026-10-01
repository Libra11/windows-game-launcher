use crate::steam;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashSet;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SearchGame {
    appid: String,
    name: String,
    image: String,
}

#[tauri::command]
pub(crate) async fn search_steam_games(query: String) -> Result<Vec<SearchGame>, String> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(Vec::new());
    }
    if query.chars().count() > 100 {
        return Err("搜索名称不能超过 100 个字符".into());
    }
    let data = steam::json_get(
        "https://store.steampowered.com/api/storesearch/",
        &[("term", query), ("l", "schinese"), ("cc", "CN")],
    )
    .await?;
    let items = data
        .get("items")
        .and_then(Value::as_array)
        .ok_or("Steam 未返回有效的搜索结果，请稍后重试")?;
    let mut seen = HashSet::new();
    Ok(items
        .iter()
        .filter_map(|item| {
            if item.get("type")?.as_str()? != "app" {
                return None;
            }
            let id = item.get("id")?.as_u64()?;
            let name = item.get("name")?.as_str()?.trim();
            if id == 0 || id > u32::MAX as u64 || name.is_empty() || !seen.insert(id) {
                return None;
            }
            let image = item
                .get("tiny_image")
                .and_then(Value::as_str)
                .and_then(|url| reqwest::Url::parse(url).ok())
                .filter(|url| url.scheme() == "https")
                .map(|url| url.to_string())
                .unwrap_or_default();
            Some(SearchGame {
                appid: id.to_string(),
                name: name.to_owned(),
                image,
            })
        })
        .take(12)
        .collect())
}
