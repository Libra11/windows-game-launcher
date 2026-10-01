use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Game {
    pub id: String,
    pub source: String,
    pub appid: String,
    pub title: String,
    pub exe_path: String,
    pub launch_uri: String,
    pub custom_unlock_path: String,
    pub metadata_json: String,
    pub scan_status: String,
    pub source_file: String,
    pub last_scan: String,
    pub schema_source: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Achievement {
    pub api_name: String,
    pub name: String,
    pub description: String,
    pub icon: String,
    pub hidden: bool,
    pub unlocked_at: Option<String>,
    pub unlock_source: Option<String>,
}

#[derive(Clone, Debug)]
pub struct AchievementDefinition {
    pub api_name: String,
    pub name: String,
    pub description: String,
    pub icon: String,
    pub hidden: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unlock {
    pub api_name: String,
    pub unlocked_at: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnlockEvent {
    pub game_id: String,
    pub game_title: String,
    pub api_name: String,
    pub achievement_name: String,
}
