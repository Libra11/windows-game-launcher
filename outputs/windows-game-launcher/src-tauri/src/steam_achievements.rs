use crate::model::AchievementDefinition;
use serde_json::Value;

pub(crate) const NO_ACHIEVEMENTS_SOURCE: &str = "Steam Web API（暂无成就）";

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
