use crate::{
    model::{AchievementDefinition, Game},
    steam_achievements,
};
use reqwest::Client;
use serde_json::Value;

fn client() -> Result<Client, String> {
    crate::network::client(crate::network::Service::Steam)
}

pub(crate) async fn json_get(url: &str, query: &[(&str, &str)]) -> Result<Value, String> {
    let response = client()?
        .get(url)
        .query(query)
        .send()
        .await
        .map_err(|error| {
            let service = if url.starts_with("https://store.steampowered.com/") {
                "Steam 商店"
            } else {
                "Steam 接口"
            };
            // 不输出包含 API Key 的请求 URL。
            if error.is_timeout() {
                format!("{service}连接超时，请检查网络和代理配置后重试")
            } else if error.is_connect() {
                format!("无法连接{service}，请检查网络、代理配置及代理服务是否运行")
            } else {
                format!("{service}网络请求失败，请检查网络后重试")
            }
        })?;
    let status = response.status();
    let data = response.json::<Value>().await;
    if !status.is_success() {
        return Err(steam_achievements::response_error(
            status,
            data.as_ref().ok(),
        ));
    }
    data.map_err(|_| "Steam 返回的数据格式无效".to_string())
}

fn parse_owned_games(data: &Value) -> Result<Vec<Game>, String> {
    let response = data.get("response").ok_or("Steam 未返回游戏库数据")?;
    let items = match response.get("games").and_then(Value::as_array) {
        Some(items) => items,
        None if response.get("game_count").and_then(Value::as_u64) == Some(0) => {
            return Ok(Vec::new());
        }
        None => return Err("Steam 未返回游戏列表，请检查 SteamID64 与账号游戏详情可见性".into()),
    };
    Ok(items
        .iter()
        .filter_map(|item| {
            let appid = item.get("appid")?.as_u64()?.to_string();
            let title = item
                .get("name")
                .and_then(Value::as_str)
                .filter(|name| !name.trim().is_empty())
                .map(str::to_owned)
                .unwrap_or_else(|| format!("AppID {appid}"));
            let icon = item
                .get("img_icon_url")
                .and_then(Value::as_str)
                .filter(|hash| !hash.is_empty())
                .map(|hash| format!("https://media.steampowered.com/steamcommunity/public/images/apps/{appid}/{hash}.jpg"));
            Some(Game {
                id: format!("steam-{appid}"),
                source: "steam".into(),
                appid: appid.clone(),
                title,
                exe_path: String::new(),
                launch_uri: format!("steam://rungameid/{appid}"),
                custom_unlock_path: String::new(),
                metadata_json: serde_json::json!({
                    "icon": icon,
                    "steamPlayedSeconds": item.get("playtime_forever").and_then(Value::as_u64).and_then(|minutes| minutes.checked_mul(60)),
                    "steamLastPlayed": item.get("rtime_last_played").and_then(Value::as_i64)
                        .filter(|time| *time > 0).and_then(|time| chrono::DateTime::from_timestamp(time, 0)).map(|time| time.to_rfc3339())
                }).to_string(),
                scan_status: String::new(),
                source_file: String::new(),
                last_scan: String::new(),
                schema_source: String::new(),
            })
        })
        .collect())
}

pub async fn owned_games(key: &str, steamid: &str) -> Result<Vec<Game>, String> {
    let data = json_get(
        "https://api.steampowered.com/IPlayerService/GetOwnedGames/v1/",
        &[
            ("key", key),
            ("steamid", steamid),
            ("include_appinfo", "1"),
            ("include_played_free_games", "1"),
        ],
    )
    .await?;
    parse_owned_games(&data)
}

pub async fn definitions(appid: &str, key: &str) -> Result<Vec<AchievementDefinition>, String> {
    let data = json_get(
        "https://api.steampowered.com/ISteamUserStats/GetSchemaForGame/v2/",
        &[("key", key), ("appid", appid), ("l", "schinese")],
    )
    .await?;
    steam_achievements::parse_definitions(&data)
}

pub async fn player_unlocks(
    appid: &str,
    key: &str,
    steamid: &str,
) -> Result<Vec<(String, String)>, String> {
    let data = json_get(
        "https://api.steampowered.com/ISteamUserStats/GetPlayerAchievements/v1/",
        &[
            ("key", key),
            ("appid", appid),
            ("steamid", steamid),
            ("l", "schinese"),
        ],
    )
    .await?;
    steam_achievements::parse_player_unlocks(&data)
}

pub async fn metadata(appid: &str) -> Result<Value, String> {
    crate::steam_store::metadata(appid).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_owned_games_for_steam_launch() {
        let data = serde_json::json!({"response": {"game_count": 2, "games": [
            {"appid": 480, "name": "Spacewar"},
            {"appid": 12345}
        ]}});
        let games = parse_owned_games(&data).unwrap();
        assert_eq!(games.len(), 2);
        assert_eq!(games[0].id, "steam-480");
        assert_eq!(games[0].title, "Spacewar");
        assert_eq!(games[0].launch_uri, "steam://rungameid/480");
        assert_eq!(games[1].title, "AppID 12345");
    }

    #[test]
    fn handles_empty_or_unavailable_library() {
        assert!(
            parse_owned_games(&serde_json::json!({"response": {"game_count": 0}}))
                .unwrap()
                .is_empty()
        );
        assert!(parse_owned_games(&serde_json::json!({"response": {}})).is_err());
    }
}
