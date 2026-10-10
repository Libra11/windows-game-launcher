use crate::{epic_auth, model::Game};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};

pub(crate) enum Error {
    Unauthorized,
    Message(String),
}
impl From<String> for Error {
    fn from(value: String) -> Self {
        Self::Message(value)
    }
}

async fn get(token: &str, url: reqwest::Url) -> Result<Value, Error> {
    let response = epic_auth::client()?
        .get(url)
        .bearer_auth(token)
        .send()
        .await
        .map_err(epic_auth::network_error)?;
    if response.status().as_u16() == 401 {
        return Err(Error::Unauthorized);
    }
    if !response.status().is_success() {
        return Err(Error::Message(match response.status().as_u16() {
            403 => "Epic 授权没有游戏库读取权限，请重新登录".into(),
            429 => "Epic 请求过于频繁，请稍后再导入".into(),
            _ => "Epic 游戏库服务暂时不可用，请稍后重试".into(),
        }));
    }
    response
        .json()
        .await
        .map_err(|_| Error::Message("Epic 游戏库响应格式无效".into()))
}

fn page(data: &Value) -> Result<(Vec<Value>, Option<String>), Error> {
    let records = data
        .get("records")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::Message("Epic 未返回有效的游戏库列表".into()))?
        .clone();
    let metadata = data
        .get("responseMetadata")
        .and_then(Value::as_object)
        .ok_or_else(|| Error::Message("Epic 游戏库分页信息无效".into()))?;
    let cursor = match metadata.get("nextCursor") {
        None | Some(Value::Null) => None,
        Some(Value::String(value)) if value.is_empty() => None,
        Some(Value::String(value)) => Some(value.clone()),
        _ => return Err(Error::Message("Epic 游戏库分页游标无效".into())),
    };
    Ok((records, cursor))
}

fn text<'a>(data: &'a Value, key: &str) -> &'a str {
    data.get(key).and_then(Value::as_str).unwrap_or("").trim()
}
fn identifier(value: &str) -> String {
    value
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

fn parse_game(item: &Value, metadata: &Value) -> Option<Game> {
    let name = text(item, "appName");
    let namespace = text(item, "namespace");
    let catalog = text(item, "catalogItemId");
    if name.is_empty()
        || namespace.is_empty()
        || catalog.is_empty()
        || namespace == "ue"
        || text(item, "sandboxType") == "PRIVATE"
        || metadata
            .get("mainGameItem")
            .is_some_and(|value| !value.is_null())
        || metadata
            .get("categories")
            .and_then(Value::as_array)
            .is_some_and(|items| {
                items
                    .iter()
                    .any(|i| matches!(text(i, "path"), "mods" | "plugins" | "assets"))
            })
    {
        return None;
    }
    let covers = crate::epic_artwork::covers(metadata);
    let heroes = crate::epic_artwork::heroes(metadata);
    let title = text(metadata, "title");
    let launch_id = format!(
        "{}%3A{}%3A{}",
        identifier(namespace),
        identifier(catalog),
        identifier(name)
    );
    Some(Game {
        id:format!("epic-{name}"), source:"epic".into(), appid:String::new(),
        title:if title.is_empty() { name } else { title }.into(), exe_path:String::new(),
        launch_uri:format!("com.epicgames.launcher://apps/{launch_id}?action=launch&silent=true"),
        custom_unlock_path:String::new(),
        metadata_json:serde_json::json!({"epicAppName":name,"epicNamespace":namespace,"epicCatalogItemId":catalog,
            "cover":covers.first().cloned().unwrap_or_default(),
            "libraryCovers":covers,"libraryHeroes":heroes,
            "description":text(metadata,"description")}).to_string(),
        scan_status:String::new(), source_file:String::new(), last_scan:String::new(), schema_source:String::new(),
    })
}

async fn game(token: String, item: Value) -> Result<Option<Game>, Error> {
    let namespace = text(&item, "namespace");
    let catalog = text(&item, "catalogItemId");
    let mut url = reqwest::Url::parse(
        "https://catalog-public-service-prod06.ol.epicgames.com/catalog/api/shared/namespace/",
    )
    .map_err(|_| Error::Message("Epic 资料地址无效".into()))?;
    url.path_segments_mut()
        .map_err(|_| Error::Message("Epic 资料地址无效".into()))?
        .pop_if_empty()
        .push(namespace)
        .push("bulk")
        .push("items");
    url.query_pairs_mut()
        .append_pair("id", catalog)
        .append_pair("includeDLCDetails", "true")
        .append_pair("includeMainGameDetails", "true")
        .append_pair("locale", "zh-CN");
    let data = get(&token, url).await?;
    let metadata = data
        .get(catalog)
        .filter(|v| v.is_object())
        .ok_or_else(|| Error::Message("Epic 未返回部分游戏资料，请稍后重新导入".into()))?;
    Ok(parse_game(&item, metadata))
}

pub(crate) async fn refresh_game(token: &str, stored: &Game) -> Result<Game, Error> {
    let metadata: Value = serde_json::from_str(&stored.metadata_json)
        .map_err(|_| Error::Message("Epic 游戏资料无效，请重新导入".into()))?;
    let item = serde_json::json!({
        "appName":text(&metadata,"epicAppName"),
        "namespace":text(&metadata,"epicNamespace"),
        "catalogItemId":text(&metadata,"epicCatalogItemId"),
        "sandboxType":"PUBLIC",
    });
    if ["appName", "namespace", "catalogItemId"].iter().any(|field| text(&item,field).is_empty()) {
        return Err(Error::Message("Epic 游戏身份缺失，请重新导入账号游戏库".into()));
    }
    game(token.to_owned(), item).await?.filter(|game| game.id == stored.id)
        .ok_or_else(|| Error::Message("Epic 返回的游戏资料身份不一致".into()))
}

pub(crate) async fn owned_games(token: &str) -> Result<Vec<Game>, Error> {
    let mut cursor = None;
    let mut cursors = HashSet::new();
    let mut items = BTreeMap::new();
    loop {
        let mut url = reqwest::Url::parse(
            "https://library-service.live.use1a.on.epicgames.com/library/api/public/items",
        )
        .map_err(|_| Error::Message("Epic 游戏库地址无效".into()))?;
        url.query_pairs_mut().append_pair("includeMetadata", "true");
        if let Some(cursor) = cursor.as_deref() {
            url.query_pairs_mut().append_pair("cursor", cursor);
        }
        let (records, next) = page(&get(token, url).await?)?;
        for item in records {
            if !text(&item, "appName").is_empty()
                && !text(&item, "namespace").is_empty()
                && !text(&item, "catalogItemId").is_empty()
                && text(&item, "namespace") != "ue"
                && text(&item, "sandboxType") != "PRIVATE"
            {
                items.insert(text(&item, "appName").to_owned(), item);
            }
        }
        let Some(next) = next else {
            break;
        };
        if !cursors.insert(next.clone()) {
            return Err(Error::Message(
                "Epic 返回重复分页游标，已停止导入，请稍后重试".into(),
            ));
        }
        cursor = Some(next);
    }
    // 限制并发资料请求，完整读取成功后再统一写入游戏库。
    let mut pending = items.into_values();
    let mut tasks = tokio::task::JoinSet::new();
    let mut games = Vec::new();
    loop {
        while tasks.len() < 4 {
            let Some(item) = pending.next() else {
                break;
            };
            tasks.spawn(game(token.to_owned(), item));
        }
        let Some(result) = tasks.join_next().await else {
            break;
        };
        if let Some(game) =
            result.map_err(|_| Error::Message("Epic 资料读取任务失败，请重试".into()))??
        {
            games.push(game);
        }
    }
    games.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(games)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn item() -> Value {
        serde_json::json!({"appName":"game name","namespace":"ns","catalogItemId":"catalog","sandboxType":"PUBLIC"})
    }
    #[test]
    fn remote_games_do_not_need_installation_and_keep_epic_identity() {
        let game = parse_game(&item(), &serde_json::json!({"title":"测试游戏","keyImages":[{"type":"DieselGameBoxTall","url":"https://example.com/cover.jpg"}]})).unwrap();
        assert_eq!(game.title, "测试游戏");
        assert!(game.exe_path.is_empty());
        assert!(game.appid.is_empty());
        assert_eq!(
            game.launch_uri,
            "com.epicgames.launcher://apps/ns%3Acatalog%3Agame%20name?action=launch&silent=true"
        );
        assert!(game.metadata_json.contains("https://example.com/cover.jpg"));
        assert!(parse_game(&item(), &serde_json::json!({"mainGameItem":{"id":"base"}})).is_none());
        let mut private = item();
        private["sandboxType"] = serde_json::json!("PRIVATE");
        assert!(parse_game(&private, &serde_json::json!({})).is_none());
    }
    #[test]
    fn validates_pagination_and_empty_library() {
        let (records, cursor) =
            page(&serde_json::json!({"records":[],"responseMetadata":{"nextCursor":"next"}}))
                .unwrap_or_else(|_| panic!("valid page"));
        assert!(records.is_empty());
        assert_eq!(cursor.as_deref(), Some("next"));
        assert!(page(&serde_json::json!({"records":[],"responseMetadata":{}})).is_ok());
        assert!(page(&serde_json::json!({"records":{}})).is_err());
    }
}
