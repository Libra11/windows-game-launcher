use serde::{Deserialize, Serialize};

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Query {
    #[serde(default = "default_period")]
    pub period: String,
    #[serde(default = "all")]
    pub source: String,
    pub day: Option<String>,
}
fn default_period() -> String {
    "30".into()
}
fn all() -> String {
    "all".into()
}

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Summary {
    pub game_count: usize,
    pub steam_seconds: Option<u64>,
    pub steam_known: usize,
    pub steam_games: usize,
    pub steam_owned_games: usize,
    pub steam_family_games: usize,
    pub steam_cached: bool,
    pub steam_checked_at: String,
    pub local_seconds: u64,
    pub unlocked: usize,
    pub manual_unlocked: usize,
    pub completed_games: usize,
    pub completion_rate: Option<f64>,
}
#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Period {
    pub seconds: u64,
    pub active_days: usize,
    pub sessions: usize,
    pub average_seconds: u64,
    pub new_unlocks: usize,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Day {
    pub date: String,
    pub recorded: bool,
    pub seconds: u64,
    pub unlocks: usize,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Platform {
    pub source: String,
    pub games: usize,
    pub seconds: u64,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GameStats {
    pub game_id: String,
    pub title: String,
    pub source: String,
    pub metadata_json: String,
    pub local_seconds: u64,
    pub period_seconds: u64,
    pub steam_seconds: Option<u64>,
    pub steam_state: String,
    pub steam_checked_at: String,
    pub total_achievements: usize,
    pub unlocked: usize,
    pub manual_unlocked: usize,
    pub definition_state: String,
    pub completion_rate: Option<f64>,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Event {
    pub game_id: String,
    pub game_title: String,
    pub api_name: String,
    pub name: String,
    pub icon: String,
    pub source: String,
    pub date: String,
    pub recorded_at: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Snapshot {
    pub started_at: String,
    pub start_date: String,
    pub from: String,
    pub to: String,
    pub generated_at: String,
    pub summary: Summary,
    pub period: Period,
    pub daily: Vec<Day>,
    pub platforms: Vec<Platform>,
    pub games: Vec<GameStats>,
    pub recent_unlocks: Vec<Event>,
    pub day_unlocks: Vec<Event>,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Session {
    pub id: String,
    pub game_id: String,
    pub game_title: String,
    pub source: String,
    pub started_at: String,
    pub ended_at: String,
    pub seconds: u64,
    pub daily_recorded: bool,
    #[serde(skip_serializing)]
    pub dates: Vec<String>,
}
#[derive(Serialize)]
pub(crate) struct SessionPage {
    pub items: Vec<Session>,
    pub total: usize,
}
