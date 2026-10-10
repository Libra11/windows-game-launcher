use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

pub const MAX_TOTAL: u64 = 10 * 1024 * 1024 * 1024;
pub const MAX_DATA: u64 = 256 * 1024 * 1024;
pub const MAX_COVER: u64 = 20 * 1024 * 1024;
pub const MAX_ENTRIES: usize = 100_000;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Appearance {
    pub theme: String,
    pub theme_color: String,
    pub fonts: Vec<String>,
}
impl Appearance {
    pub fn validate(&self) -> Result<(), String> {
        if !["dark", "light"].contains(&self.theme.as_str())
            || !["sage", "blue", "violet", "rose", "amber", "teal"].contains(&self.theme_color.as_str())
            || self.fonts.is_empty() || self.fonts.len() > 64
            || self.fonts.iter().any(|font| font.trim().is_empty() || font.len() > 800 || font.chars().any(char::is_control))
        { return Err("备份外观设置无效".into()); }
        Ok(())
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Preferences {
    pub appearance: Appearance,
    pub behavior: BTreeMap<String, String>,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Dataset {
    pub snapshot_at: String,
    pub tables: BTreeMap<String, Vec<Vec<Value>>>,
    pub settings: BTreeMap<String, String>,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Counts {
    pub tags: usize,
    pub collections: usize,
    pub games: usize,
    pub favorites: usize,
    pub sessions: usize,
    pub achievements: usize,
    pub unlocks: usize,
    pub covers: usize,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FileRecord { pub size: u64, pub sha256: String }

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Manifest {
    pub format: String,
    pub format_version: u32,
    pub app_version: String,
    pub created_at: String,
    pub counts: Counts,
    pub files: BTreeMap<String, FileRecord>,
    pub missing_covers: Vec<String>,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PathMapping { pub from: String, pub to: String }

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PathOverride {
    pub game_id: String,
    pub exe_path: String,
    pub custom_unlock_path: Option<String>,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Relocation { pub mappings: Vec<PathMapping>, pub overrides: Vec<PathOverride> }

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PathPreview {
    pub game_id: String, pub title: String, pub source: String,
    pub original_path: String, pub exe_path: String, pub custom_unlock_path: String,
    pub status: String, pub record_status: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Inspection {
    pub preparation_id: String, pub manifest: Manifest, pub current: Counts,
    pub paths: Vec<PathPreview>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult { pub path: String, pub manifest: Manifest }

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Pending {
    pub restore_id: String, pub archive: String, pub sha256: String,
    pub relocation: Relocation, pub safety_backup: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RestoreNotice {
    pub id: String, pub success: bool, pub message: String, pub safety_backup: String,
    pub missing_paths: usize,
    #[serde(default)]
    pub acknowledged: bool,
}
