use crate::steam;
use serde::Serialize;
use serde_json::{json, Value};
use std::{collections::HashSet, future::Future};

const ASSET_BASE: &str = "https://shared.fastly.steamstatic.com/store_item_assets/";
pub(crate) const METADATA_VERSION: u64 = 1;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SearchGame {
    appid: String,
    name: String,
    image: String,
}

async fn service(method: &str, mut input: Value, region: &str) -> Result<Value, String> {
    input["context"] = json!({"language": "schinese", "country_code": region});
    let input = input.to_string();
    steam::json_get(
        &format!("https://api.steampowered.com/{method}/v1/"),
        &[("input_json", &input)],
    )
    .await
}

async fn region_fallback<F, Fut>(
    mut fetch: F,
    unavailable: impl Fn(&Value) -> Result<bool, String>,
) -> Result<Value, String>
where
    F: FnMut(&'static str) -> Fut,
    Fut: Future<Output = Result<Value, String>>,
{
    let china = fetch("CN").await?;
    if unavailable(&china)? {
        fetch("US").await
    } else {
        Ok(china)
    }
}

fn items(data: &Value) -> Result<&[Value], String> {
    let response = data.get("response").ok_or("Steam 服务未返回有效结果")?;
    if let Some(items) = response.get("store_items").and_then(Value::as_array) {
        return Ok(items);
    }
    if response
        .pointer("/metadata/total_matching_records")
        .and_then(Value::as_u64)
        == Some(0)
    {
        return Ok(&[]);
    }
    Err("Steam 服务返回的游戏列表格式无效，请稍后重试".into())
}

fn available(item: &Value) -> bool {
    item.get("item_type").and_then(Value::as_u64) == Some(0)
        && item.get("success").and_then(Value::as_u64) == Some(1)
        && item.get("visible").and_then(Value::as_bool) != Some(false)
}

// 文件名、语言和哈希来自官方资源清单，CDN 使用原有的 Fastly 域名。
fn asset_url(assets: &Value, field: &str) -> Option<String> {
    let filename = assets.get(field)?.as_str()?.trim();
    if filename.is_empty() {
        return None;
    }
    let template = assets.get("asset_url_format")?.as_str()?;
    if !template.contains("${FILENAME}") {
        return None;
    }
    let path = template.replace("${FILENAME}", filename);
    let url = if path.starts_with("https://") {
        reqwest::Url::parse(&path).ok()?
    } else {
        reqwest::Url::parse(ASSET_BASE).ok()?.join(&path).ok()?
    };
    (url.scheme() == "https").then(|| url.to_string())
}

fn asset_urls(assets: &Value, fields: &[&str]) -> Vec<String> {
    let mut seen = HashSet::new();
    fields
        .iter()
        .filter_map(|field| asset_url(assets, field))
        .filter(|url| seen.insert(url.clone()))
        .collect()
}

pub(crate) async fn search(query: &str) -> Result<Vec<SearchGame>, String> {
    search_regions(|region| async move {
        service(
            "IStoreQueryService/SearchSuggestions",
            json!({
                "search_term": query,
                "max_results": 12,
                "filters": {"type_filters": {"include_apps": true}},
                "data_request": {"include_assets": true}
            }),
            region,
        )
        .await
    })
    .await
}

async fn search_regions<F, Fut>(fetch: F) -> Result<Vec<SearchGame>, String>
where
    F: FnMut(&'static str) -> Fut,
    Fut: Future<Output = Result<Value, String>>,
{
    let data = region_fallback(fetch, |data| Ok(search_results(data)?.is_empty())).await?;
    search_results(&data)
}

fn search_results(data: &Value) -> Result<Vec<SearchGame>, String> {
    let mut seen = HashSet::new();
    Ok(items(data)?
        .iter()
        .filter_map(|item| {
            if !available(item) {
                return None;
            }
            let id = item.get("appid")?.as_u64()?;
            let name = item.get("name")?.as_str()?.trim();
            if id == 0 || id > u32::MAX as u64 || name.is_empty() || !seen.insert(id) {
                return None;
            }
            let image = asset_urls(
                &item["assets"],
                &["header", "small_capsule", "library_capsule"],
            )
            .into_iter()
            .next()
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

pub(crate) async fn metadata(appid: &str) -> Result<Value, String> {
    let id = appid
        .parse::<u32>()
        .ok()
        .filter(|id| *id != 0)
        .ok_or("Steam AppID 无效")?;
    metadata_regions(id, |region| store_item(id, region)).await
}

async fn store_item(appid: u32, region: &str) -> Result<Value, String> {
    service("IStoreBrowseService/GetItems", json!({
        "ids": [{"appid": appid}],
        "data_request": {"include_assets": true, "include_basic_info": true, "include_release": true}
    }), region).await
}

fn metadata_item(data: &Value, appid: u32) -> Result<Option<&Value>, String> {
    Ok(items(data)?.iter().find(|item| {
        item.get("appid").and_then(Value::as_u64) == Some(appid as u64) && available(item)
    }))
}

fn metadata_access_denied(data: &Value, appid: u32) -> Result<bool, String> {
    Ok(items(data)?.iter().any(|item| {
        // 拒绝访问时 appid 为 0，使用请求条目的 id 核对，不能接受其他游戏的错误。
        item.get("id").and_then(Value::as_u64) == Some(appid as u64)
            && item.get("item_type").and_then(Value::as_u64) == Some(0)
            && item.get("success").and_then(Value::as_u64) == Some(15)
            && item.get("appid").and_then(Value::as_u64)
                .is_some_and(|id| id == 0 || id == appid as u64)
    }))
}

pub(crate) async fn cdn_sources(appid: &str, wide: bool) -> Result<Vec<String>, String> {
    let id = appid.parse::<u32>().ok().filter(|id| *id != 0).ok_or("Steam AppID 无效")?;
    cdn_sources_regions(id, wide, |region| store_item(id, region)).await
}

async fn cdn_sources_regions<F, Fut>(appid: u32, wide: bool, mut fetch: F) -> Result<Vec<String>, String>
where
    F: FnMut(&'static str) -> Fut,
    Fut: Future<Output = Result<Value, String>>,
{
    if !metadata_access_denied(&fetch("CN").await?, appid)?
        || !metadata_access_denied(&fetch("US").await?, appid)?
    {
        return Ok(Vec::new());
    }
    let base = format!("{ASSET_BASE}steam/apps/{appid}");
    let image = if wide { "library_hero" } else { "library_600x900" };
    Ok(vec![format!("{base}/{image}_2x.jpg"), format!("{base}/{image}.jpg"), format!("{base}/header.jpg")])
}

async fn metadata_regions<F, Fut>(appid: u32, fetch: F) -> Result<Value, String>
where
    F: FnMut(&'static str) -> Fut,
    Fut: Future<Output = Result<Value, String>>,
{
    let data = region_fallback(fetch, |data| Ok(metadata_item(data, appid)?.is_none())).await?;
    let item = metadata_item(&data, appid)?.ok_or("Steam 服务没有这款游戏的资料")?;
    let name = item
        .get("name")
        .and_then(Value::as_str)
        .filter(|name| !name.trim().is_empty())
        .ok_or("Steam 服务返回的游戏名称为空")?;
    let assets = &item["assets"];
    let release = &item["release"];
    let release_date = release
        .get("custom_release_date_message")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .or_else(|| {
            release
                .get("steam_release_date")
                .and_then(Value::as_i64)
                .filter(|time| *time > 0)
                .and_then(|time| chrono::DateTime::from_timestamp(time, 0))
                .map(|date| {
                    date.with_timezone(&chrono::Local)
                        .format("%Y-%m-%d")
                        .to_string()
                })
        })
        .unwrap_or_default();
    let developers: Vec<&str> = item
        .pointer("/basic_info/developers")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|creator| creator.get("name").and_then(Value::as_str))
        .collect();
    Ok(json!({
        "name": name,
        "cover": asset_url(assets, "header").unwrap_or_default(),
        "libraryCovers": asset_urls(assets, &["library_capsule_2x", "library_capsule"]),
        "libraryHeroes": asset_urls(assets, &["library_hero_2x", "library_hero"]),
        "description": item.pointer("/basic_info/short_description").and_then(Value::as_str).unwrap_or(""),
        "releaseDate": release_date,
        "developers": developers,
        "steamStoreVersion": METADATA_VERSION
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game() -> Value {
        json!({
            "item_type": 0, "id": 4162040, "appid": 4162040,
            "success": 1, "visible": true, "name": "绝区零",
            "assets": {
                "asset_url_format": "steam/apps/4162040/${FILENAME}?t=1788905636",
                "header": "header-hash/header_schinese.jpg",
                "library_capsule": "8cb11f8041f830d14b7d8288cc166608802dc939/library_capsule_schinese.jpg",
                "library_capsule_2x": "8cb11f8041f830d14b7d8288cc166608802dc939/library_capsule_schinese_2x.jpg",
                "library_hero": "hero-hash/library_hero.jpg"
            },
            "basic_info": {"short_description": "中文游戏简介", "developers": [{"name": "COGNOSPHERE"}]},
            "release": {"steam_release_date": 1781647250}
        })
    }

    fn response(games: Vec<Value>) -> Value {
        json!({"response": {"store_items": games}})
    }

    fn denied(appid: u32) -> Value {
        json!({"item_type": 0, "id": appid, "appid": 0, "success": 15, "visible": false, "name": ""})
    }

    #[test]
    fn cdn_sources_require_both_regions_to_deny_the_requested_app() {
        tauri::async_runtime::block_on(async {
            for wide in [false, true] {
                let mut requests = Vec::new();
                let urls = cdn_sources_regions(760620, wide, |region| {
                    requests.push(region);
                    std::future::ready(Ok(response(vec![denied(760620)])))
                }).await.unwrap();
                assert_eq!(requests, ["CN", "US"]);
                let base = format!("{ASSET_BASE}steam/apps/760620");
                let image = if wide { "library_hero" } else { "library_600x900" };
                assert_eq!(urls, [format!("{base}/{image}_2x.jpg"), format!("{base}/{image}.jpg"), format!("{base}/header.jpg")]);
            }
            // 兜底只提供图片候选，资料接口仍报告没有商店资料。
            assert!(metadata_regions(760620, |_| std::future::ready(Ok(response(vec![denied(760620)])))).await.is_err());
        });
    }

    #[test]
    fn cdn_sources_reject_other_errors_empty_available_or_mismatched_items() {
        tauri::async_runtime::block_on(async {
            let mut other_error = denied(760620);
            other_error["success"] = json!(20);
            let mut wrong_type = denied(760620);
            wrong_type["item_type"] = json!(1);
            let mut conflicting_app = denied(760620);
            conflicting_app["appid"] = json!(480);
            let mut invalid_code = denied(760620);
            invalid_code["success"] = json!("15");
            let mut available_game = game();
            available_game["id"] = json!(760620);
            available_game["appid"] = json!(760620);
            let errors = [
                Err("连接超时".to_owned()),
                Ok(json!({"response": {}})),
                Ok(response(vec![])),
                Ok(response(vec![other_error])),
                Ok(response(vec![denied(480)])),
                Ok(response(vec![wrong_type])),
                Ok(response(vec![conflicting_app])),
                Ok(response(vec![invalid_code])),
                Ok(response(vec![available_game])),
            ];
            for error in errors {
                for bad_region in ["CN", "US"] {
                    let mut requests = Vec::new();
                    let result = cdn_sources_regions(760620, false, |region| {
                        requests.push(region);
                        std::future::ready(if region == bad_region {
                            error.clone()
                        } else {
                            Ok(response(vec![denied(760620)]))
                        })
                    }).await;
                    assert!(result.is_err() || result.unwrap().is_empty(), "意外对 {bad_region} 的 {error:?} 启用 CDN");
                    if bad_region == "US" {
                        assert_eq!(requests, ["CN", "US"]);
                    } else {
                        assert_eq!(requests, ["CN"]);
                    }
                }
            }
        });
    }

    #[test]
    fn resolves_localized_hashed_and_legacy_assets_from_service_template() {
        let data = game();
        assert_eq!(asset_url(&data["assets"], "library_capsule_2x").unwrap(),
            format!("{ASSET_BASE}steam/apps/4162040/8cb11f8041f830d14b7d8288cc166608802dc939/library_capsule_schinese_2x.jpg?t=1788905636"));
        let legacy = json!({"asset_url_format": "steam/apps/1245620/${FILENAME}?t=2", "library_capsule": "library_600x900.jpg"});
        assert_eq!(
            asset_url(&legacy, "library_capsule").unwrap(),
            format!("{ASSET_BASE}steam/apps/1245620/library_600x900.jpg?t=2")
        );
        assert!(asset_url(
            &json!({"library_capsule": "missing-template.jpg"}),
            "library_capsule"
        )
        .is_none());
        assert!(asset_url(
            &json!({"asset_url_format": "fixed.jpg", "library_capsule": "a.jpg"}),
            "library_capsule"
        )
        .is_none());
    }

    #[test]
    fn parses_service_search_and_filters_packages_hidden_and_duplicate_apps() {
        let mut package = game();
        package["item_type"] = json!(1);
        let mut hidden = game();
        hidden["visible"] = json!(false);
        let mut failed = game();
        failed["success"] = json!(15);
        let games =
            search_results(&response(vec![package, hidden, failed, game(), game()])).unwrap();
        assert_eq!(games.len(), 1);
        assert_eq!(games[0].appid, "4162040");
        assert!(games[0].image.contains("header-hash/header_schinese.jpg"));
        assert!(
            search_results(&json!({"response": {"metadata": {"total_matching_records": 0}}}))
                .unwrap()
                .is_empty()
        );
        assert!(search_results(&json!({"response": {}})).is_err());
    }

    #[test]
    fn search_uses_us_only_for_empty_valid_china_results() {
        tauri::async_runtime::block_on(async {
            for empty in [false, true] {
                let mut requests = Vec::new();
                let result = search_regions(|region| {
                    requests.push(region);
                    std::future::ready(Ok(if empty && region == "CN" {
                        response(vec![])
                    } else {
                        response(vec![game()])
                    }))
                })
                .await
                .unwrap();
                assert_eq!(requests, if empty { vec!["CN", "US"] } else { vec!["CN"] });
                assert_eq!(result[0].name, "绝区零");
            }
            let mut requests = Vec::new();
            assert!(search_regions(|region| {
                requests.push(region);
                std::future::ready(Ok(response(vec![])))
            })
            .await
            .unwrap()
            .is_empty());
            assert_eq!(requests, ["CN", "US"]);
        });
    }

    #[test]
    fn errors_do_not_masquerade_as_empty_results() {
        tauri::async_runtime::block_on(async {
            for reply in [Err("连接超时".to_owned()), Ok(json!({"response": {}}))] {
                let mut requests = Vec::new();
                assert!(search_regions(|region| {
                    requests.push(region);
                    std::future::ready(reply.clone())
                })
                .await
                .is_err());
                assert_eq!(requests, ["CN"]);
            }
            let mut requests = Vec::new();
            let error = search_regions(|region| {
                requests.push(region);
                std::future::ready(if region == "CN" {
                    Ok(response(vec![]))
                } else {
                    Err("美区连接超时".to_owned())
                })
            })
            .await
            .err()
            .unwrap();
            assert_eq!(requests, ["CN", "US"]);
            assert_eq!(error, "美区连接超时");
        });
    }

    #[test]
    fn metadata_uses_country_fallback_and_retains_separate_library_artwork() {
        tauri::async_runtime::block_on(async {
            let mut unavailable = game();
            unavailable["success"] = json!(15);
            unavailable["visible"] = json!(false);
            for china in [response(vec![game()]), response(vec![unavailable])] {
                let available_in_china = available(&china["response"]["store_items"][0]);
                let mut requests = Vec::new();
                let meta = metadata_regions(4162040, |region| {
                    requests.push(region);
                    std::future::ready(Ok(if region == "CN" {
                        china.clone()
                    } else {
                        response(vec![game()])
                    }))
                })
                .await
                .unwrap();
                assert_eq!(
                    requests,
                    if available_in_china {
                        vec!["CN"]
                    } else {
                        vec!["CN", "US"]
                    }
                );
                assert_eq!(meta["name"], "绝区零");
                assert_eq!(meta["description"], "中文游戏简介");
                assert_eq!(meta["developers"], json!(["COGNOSPHERE"]));
                assert_eq!(meta["libraryCovers"].as_array().unwrap().len(), 2);
                assert!(meta["libraryCovers"][0]
                    .as_str()
                    .unwrap()
                    .contains("_2x.jpg"));
                assert!(meta["libraryHeroes"][0]
                    .as_str()
                    .unwrap()
                    .contains("hero-hash"));
                assert_eq!(meta["steamStoreVersion"], METADATA_VERSION);
            }
            // 不接受其他游戏的资料，即使它有有效封面。
            assert!(
                metadata_regions(480, |_| std::future::ready(Ok(response(vec![game()]))))
                    .await
                    .is_err()
            );
        });
    }

    #[test]
    #[ignore = "手动验证 Steam 服务和图片网络连接"]
    fn live_services_and_official_image_urls() {
        tauri::async_runtime::block_on(async {
            for (query, appid) in [
                ("绝区零", "4162040"),
                ("ELDEN RING", "1245620"),
                ("Hollow Knight", "367520"),
            ] {
                let games = search(query).await.unwrap();
                assert!(
                    games.iter().any(|game| game.appid == appid),
                    "名称搜索未返回 {appid}"
                );
                let meta = metadata(appid).await.unwrap();
                assert!(!meta["description"].as_str().unwrap().is_empty());
                for field in ["libraryCovers", "libraryHeroes"] {
                    let urls = meta[field].as_array().unwrap();
                    assert!(!urls.is_empty(), "{appid} 缺少 {field}");
                    let response = reqwest::Client::new()
                        .head(urls[0].as_str().unwrap())
                        .timeout(std::time::Duration::from_secs(10))
                        .send()
                        .await
                        .unwrap();
                    assert!(
                        response.status().is_success(),
                        "{appid} {field}: {}",
                        response.status()
                    );
                    assert!(response
                        .headers()
                        .get("content-type")
                        .unwrap()
                        .to_str()
                        .unwrap()
                        .starts_with("image/"));
                }
                println!("已验证 {appid}：名称搜索、简介、竖版封面及横幅");
            }
        });
    }
}
