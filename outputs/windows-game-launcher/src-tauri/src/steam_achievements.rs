use crate::model::AchievementDefinition;
use serde_json::Value;
use std::collections::HashSet;

pub(crate) const NO_ACHIEVEMENTS_SOURCE: &str = "Steam Web API（暂无成就）";

pub(crate) fn parse_game_definitions(data: &Value, appid: &str) -> Result<Vec<AchievementDefinition>, String> {
    let appid = appid.parse::<u32>().ok().filter(|id| *id > 0)
        .ok_or("Steam AppID 无效")?;
    let items = data.pointer("/response/achievements").and_then(Value::as_array)
        .ok_or("Steam 未返回有效的成就定义列表")?;
    let mut seen = HashSet::new();
    let mut definitions = Vec::with_capacity(items.len());
    for item in items {
        let api_name = item.get("internal_name").and_then(Value::as_str)
            .filter(|name| !name.trim().is_empty()).ok_or("Steam 成就定义缺少标识")?;
        if !seen.insert(api_name) { return Err("Steam 成就定义包含重复标识".into()); }
        let archived = optional_bool(item, "archived")?;
        if archived { continue; }
        let name = optional_text(item, "localized_name")?;
        let description = optional_text(item, "localized_desc")?;
        let image = optional_text(item, "icon")?;
        let icon = if image.is_empty() {
            String::new()
        } else {
            let (name, extension) = image.split_once('.').ok_or("Steam 成就图标格式无效")?;
            if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
                || !matches!(extension, "jpg" | "png") {
                return Err("Steam 成就图标格式无效".into());
            }
            format!("https://shared.fastly.steamstatic.com/community_assets/images/apps/{appid}/{image}")
        };
        definitions.push(AchievementDefinition {
            api_name: api_name.to_owned(),
            name: if name.trim().is_empty() { api_name } else { name }.to_owned(),
            description: description.to_owned(),
            icon,
            hidden: optional_bool(item, "hidden")?,
        });
    }
    Ok(definitions)
}

fn optional_text<'a>(item: &'a Value, field: &str) -> Result<&'a str, String> {
    match item.get(field) {
        None => Ok(""),
        Some(value) => value.as_str().ok_or_else(|| format!("Steam 成就定义字段 {field} 格式无效")),
    }
}

fn optional_bool(item: &Value, field: &str) -> Result<bool, String> {
    match item.get(field) {
        None => Ok(false),
        Some(value) => value.as_bool().ok_or_else(|| format!("Steam 成就定义字段 {field} 格式无效")),
    }
}

pub(crate) fn parse_definitions(data: &Value) -> Result<Vec<AchievementDefinition>, String> {
    let game = data
        .get("game")
        .and_then(Value::as_object)
        .ok_or("Steam 未返回有效的成就定义数据")?;
    let stats = match game.get("availableGameStats") {
        None => return Ok(Vec::new()),
        Some(stats) => stats.as_object().ok_or("Steam 成就定义格式无效")?,
    };
    let items = match stats.get("achievements") {
        None => return Ok(Vec::new()),
        Some(items) => items.as_array().ok_or("Steam 成就定义格式无效")?,
    };
    items
        .iter()
        .map(|item| {
            let api_name = item
                .get("name")
                .and_then(Value::as_str)
                .filter(|name| !name.is_empty())
                .ok_or("Steam 成就定义缺少标识")?
                .to_owned();
            Ok(AchievementDefinition {
                name: item
                    .get("displayName")
                    .and_then(Value::as_str)
                    .unwrap_or(&api_name)
                    .to_owned(),
                api_name,
                description: item
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned(),
                icon: item
                    .get("icon")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned(),
                hidden: item.get("hidden").is_some_and(|v| v == true || v == 1),
            })
        })
        .collect()
}

pub(crate) fn parse_player_unlocks(data: &Value) -> Result<Vec<(String, String)>, String> {
    if data.pointer("/playerstats/success") != Some(&Value::Bool(true)) {
        return Err("Steam 未返回成就状态，请检查账号资料可见性与 AppID".into());
    }
    let items = data
        .pointer("/playerstats/achievements")
        .and_then(Value::as_array)
        .ok_or("Steam 未返回成就列表")?;
    Ok(items
        .iter()
        .filter_map(|item| {
            if item.get("achieved") != Some(&Value::from(1)) {
                return None;
            }
            let name = item.get("apiname")?.as_str()?.to_owned();
            let seconds = item.get("unlocktime")?.as_i64()?;
            let time = chrono::DateTime::from_timestamp(seconds, 0)?.to_rfc3339();
            Some((name, time))
        })
        .collect())
}

pub(crate) fn response_error(status: reqwest::StatusCode, data: Option<&Value>) -> String {
    // 只映射已知错误，避免把接口中的凭据或原始响应展示给用户。
    let reason = match data
        .and_then(|data| data.pointer("/playerstats/error"))
        .and_then(Value::as_str)
    {
        Some("Requested app has no stats") => "Steam 未提供此游戏的个人成就记录，已保留缓存",
        Some("Profile is not public") => "Steam 账号游戏资料未公开，无法同步个人成就，已保留缓存",
        Some("Invalid API key") => "Steam API Key 无效，请检查设置",
        _ => return format!("Steam 接口返回 HTTP {status}"),
    };
    reason.into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn portal_game_achievements_accept_normal_icon_filenames() {
        let data: Value = serde_json::from_str(include_str!("../fixtures/steam-game-achievements-portal.json")).unwrap();
        let items = parse_game_definitions(&data, "400").unwrap();
        assert_eq!(items.len(), 15);
        assert!(items.iter().any(|item| item.icon == "https://shared.fastly.steamstatic.com/community_assets/images/apps/400/portal_getportalguns.jpg"));
        for filename in ["portal_getportalguns.jpg", "achievement_Test-01.png"] {
            let data = json!({"response":{"achievements":[{"internal_name":"A","icon":filename}]}});
            assert!(parse_game_definitions(&data, "400").is_ok());
        }
        for filename in ["../icon.jpg", "..\\icon.jpg", "folder/icon.jpg", "/icon.jpg",
            "https://example.test/icon.jpg", "%2e%2e%2ficon.jpg", "icon.jpg?x=1",
            "icon.jpg#part", "icon..jpg", ".jpg", "icon.jpg\u{0}"] {
            let data = json!({"response":{"achievements":[{"internal_name":"A","icon":filename}]}});
            assert!(parse_game_definitions(&data, "400").is_err(), "accepted unsafe icon: {filename:?}");
        }
    }

    #[test]
    fn game_achievements_include_hidden_descriptions_and_validate_icons() {
        let data = json!({"response":{"achievements":[{
            "internal_name":"81003", "localized_name":"山中斗狼第三难",
            "localized_desc":"名是假的，不必留情！", "hidden":true,
            "icon":"cf52ec300e1eb11164aba082a69973b746872f67.jpg", "archived":false
        }, {"internal_name":"OLD", "archived":true}]}});
        let items = parse_game_definitions(&data, "2358720").unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].api_name, "81003");
        assert_eq!(items[0].description, "名是假的，不必留情！");
        assert!(items[0].hidden);
        assert_eq!(items[0].icon, "https://shared.fastly.steamstatic.com/community_assets/images/apps/2358720/cf52ec300e1eb11164aba082a69973b746872f67.jpg");
        assert!(parse_game_definitions(&json!({"response":{"achievements":[]}}), "2358720").unwrap().is_empty());
        for invalid in [
            json!({"response":{}}),
            json!({"response":{"achievements":null}}),
            json!({"response":{"achievements":[{}]}}),
            json!({"response":{"achievements":[{"internal_name":"A"},{"internal_name":"A"}]}}),
            json!({"response":{"achievements":[{"internal_name":"A","localized_desc":null}]}}),
            json!({"response":{"achievements":[{"internal_name":"A","hidden":"true"}]}}),
            json!({"response":{"achievements":[{"internal_name":"A","icon":"../icon.jpg"}]}}),
        ] { assert!(parse_game_definitions(&invalid, "2358720").is_err()); }
        assert!(parse_game_definitions(&data, "../2358720").is_err());
    }

    #[test]
    fn games_without_achievements_are_valid_empty_schemas() {
        for data in [
            json!({"game":{}}),
            json!({"game":{"availableGameStats":{"stats":[]}}}),
            json!({"game":{"availableGameStats":{"achievements":[]}}}),
        ] {
            assert!(parse_definitions(&data).unwrap().is_empty());
        }
    }

    #[test]
    fn malformed_responses_are_not_classified_as_no_achievements() {
        for data in [
            json!({}),
            json!({"game":null}),
            json!({"game":{"availableGameStats":null}}),
            json!({"game":{"availableGameStats":{"achievements":{}}}}),
            json!({"game":{"availableGameStats":{"achievements":[{}]}}}),
        ] {
            assert!(parse_definitions(&data).is_err());
        }
    }

    #[test]
    fn parses_chinese_definitions_and_official_unlocks() {
        let definitions = parse_definitions(&json!({"game":{"availableGameStats":{"achievements":[{"name":"FIRST","displayName":"初次冒险","description":"完成教程","hidden":1,"icon":"https://example.test/icon.jpg"}]}}})).unwrap();
        assert_eq!(definitions[0].name, "初次冒险");
        assert!(definitions[0].hidden);
        let unlocks = parse_player_unlocks(&json!({"playerstats":{"success":true,"achievements":[{"apiname":"FIRST","achieved":1,"unlocktime":1704067200},{"apiname":"SECOND","achieved":0,"unlocktime":0}]}})).unwrap();
        assert_eq!(
            unlocks,
            vec![("FIRST".into(), "2024-01-01T00:00:00+00:00".into())]
        );
    }

    #[test]
    fn unavailable_player_records_do_not_become_empty_unlocks() {
        assert!(parse_player_unlocks(
            &json!({"playerstats":{"success":false,"error":"Requested app has no stats"}})
        )
        .is_err());
        let status = reqwest::StatusCode::BAD_REQUEST;
        assert!(response_error(
            status,
            Some(&json!({"playerstats":{"error":"Requested app has no stats"}}))
        )
        .contains("已保留缓存"));
        assert!(response_error(
            status,
            Some(&json!({"playerstats":{"error":"Profile is not public"}}))
        )
        .contains("未公开"));
        assert_eq!(
            response_error(status, Some(&json!({"error":"secret"}))),
            "Steam 接口返回 HTTP 400 Bad Request"
        );
    }
}
