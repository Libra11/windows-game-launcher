//! Social payloads are independent from private library/save resources.
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Privacy {
    pub invisible: bool,
    pub library: bool,
    pub playtime: bool,
    pub achievements: bool,
    pub hidden_games: Vec<String>,
}
impl Default for Privacy {
    fn default() -> Self {
        Self {
            invisible: false,
            library: true,
            playtime: true,
            achievements: true,
            hidden_games: vec![],
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Playing {
    pub key: String,
    pub title: String,
    pub session_id: String,
}
impl Playing {
    pub fn valid(&self) -> bool {
        !self.key.is_empty()
            && self.key.len() <= 300
            && !self.title.trim().is_empty()
            && self.title.chars().count() <= 200
            && uuid::Uuid::parse_str(&self.session_id).is_ok()
            && !self
                .key
                .chars()
                .chain(self.title.chars())
                .any(char::is_control)
    }
}
