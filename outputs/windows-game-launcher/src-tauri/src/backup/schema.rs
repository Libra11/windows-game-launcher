use super::model::*;
use serde_json::Value;
use std::collections::HashSet;

pub struct Table { pub name: &'static str, pub columns: &'static [&'static str] }
pub const TABLES: &[Table] = &[
    Table { name:"games", columns:&["id","source","appid","title","exe_path","launch_uri","custom_unlock_path","metadata_json","scan_status","source_file","last_scan","schema_source"] },
    Table { name:"library_tags", columns:&["id","name"] },
    Table { name:"library_collections", columns:&["id","name","position"] },
    Table { name:"game_tags", columns:&["game_id","tag_id"] },
    Table { name:"game_collections", columns:&["game_id","collection_id"] },
    Table { name:"achievements", columns:&["appid","api_name","name","description","icon","hidden"] },
    Table { name:"unlocks", columns:&["game_id","api_name","source","unlocked_at","evidence"] },
    Table { name:"game_activity", columns:&["game_id","favorite","last_played","played_seconds"] },
    Table { name:"play_sessions", columns:&["id","game_id","started_at","ended_at","seconds"] },
    Table { name:"daily_playtime", columns:&["session_id","game_id","date","seconds"] },
    Table { name:"statistics_unlock_events", columns:&["game_id","namespace","api_name","recorded_at","date"] },
];
pub const BEHAVIOR: &[&str] = &["minimize_on_launch","close_to_tray","achievement_notifications","achievement_overlay_options"];

pub fn portable_setting(key: &str) -> bool {
    ["steam_id","statistics_started_at"].contains(&key) || [
        "achievement_platform:", "statistics_official_baseline:",
        "steam_playtime:", "steam_family_playtime:", "cover_cache:",
    ].iter().any(|prefix| key.starts_with(prefix))
}

pub fn cover_name(file: &str) -> bool {
    let Some((stem, ext)) = file.rsplit_once('.') else { return false };
    uuid::Uuid::parse_str(stem).is_ok() && ["jpg","png","gif","webp","avif"].contains(&ext)
}

pub fn counts(data: &Dataset, covers: usize) -> Counts {
    let length = |name| data.tables.get(name).map_or(0, Vec::len);
    Counts { tags:length("library_tags"), collections:length("library_collections"), games:length("games"), favorites:data.tables.get("game_activity").map_or(0, |rows| rows.iter().filter(|row| row[1].as_i64() == Some(1)).count()),
        sessions:length("play_sessions"), achievements:length("achievements"), unlocks:length("unlocks"), covers }
}

pub fn validate(data: &Dataset, preferences: &Preferences) -> Result<(), String> {
    preferences.appearance.validate()?;
    if chrono::DateTime::parse_from_rfc3339(&data.snapshot_at).is_err() { return Err("备份快照时间无效".into()); }
    if data.tables.len() != TABLES.len() || data.settings.keys().any(|key| !portable_setting(key))
        || preferences.behavior.keys().any(|key| !BEHAVIOR.contains(&key.as_str()))
    { return Err("备份包含不支持的数据或设置".into()); }
    for table in TABLES {
        let rows = data.tables.get(table.name).ok_or("备份数据表缺失")?;
        let mut unique = HashSet::new();
        for row in rows {
            if row.len() != table.columns.len() || row.iter().any(|value| {
                !(value.is_null() || value.is_string() || value.as_i64().is_some())
            }) { return Err(format!("备份 {} 数据格式无效", table.name)); }
            for (index, column) in table.columns.iter().enumerate() {
                let value = &row[index];
                if ["seconds","played_seconds","favorite","hidden","position"].contains(column) {
                    let number = value.as_i64().filter(|value| *value >= 0).ok_or("备份计数或时长无效")?;
                    if ["favorite","hidden"].contains(column) && number > 1 { return Err("备份开关数值无效".into()); }
                } else if !(value.is_string() || (*column == "unlocked_at" && value.is_null())) {
                    return Err("备份记录字段类型无效".into());
                }
                if *column == "date" && chrono::NaiveDate::parse_from_str(value.as_str().unwrap_or(""), "%Y-%m-%d").is_err() {
                    return Err("备份统计日期无效".into());
                }
                if ["started_at","ended_at","unlocked_at","recorded_at","last_played"].contains(column) {
                    if let Some(text) = value.as_str().filter(|text| !text.is_empty()) {
                        if chrono::DateTime::parse_from_rfc3339(text).is_err() { return Err("备份记录时间无效".into()); }
                    }
                }
            }
            let key: Vec<_> = match table.name {
                "games" | "game_activity" | "play_sessions" | "library_tags" | "library_collections" => vec![row[0].clone()],
                "achievements" | "unlocks" | "daily_playtime" | "game_tags" | "game_collections" => vec![row[0].clone(),row[1 + usize::from(table.name == "daily_playtime")].clone()],
                _ => vec![row[0].clone(),row[1].clone(),row[2].clone()],
            };
            if !unique.insert(serde_json::to_string(&key).unwrap()) { return Err(format!("备份 {} 存在重复记录", table.name)); }
        }
    }
    let mut ids = HashSet::new();
    for row in &data.tables["games"] {
        let id = row[0].as_str().filter(|id| !id.is_empty()).ok_or("游戏标识无效")?;
        if !ids.insert(id) || !["steam","epic","local"].contains(&row[1].as_str().unwrap_or(""))
            || row.iter().any(|value| !value.is_string()) { return Err("备份游戏数据无效或重复".into()); }
        let meta: Value = serde_json::from_str(row[7].as_str().unwrap()).map_err(|_| "游戏资料格式无效")?;
        if !meta.is_object() { return Err("游戏资料不是有效对象".into()); }
        let source = row[1].as_str().unwrap();
        let appid = row[2].as_str().unwrap();
        let uri = row[5].as_str().unwrap();
        if (source == "steam" && (appid.parse::<u32>().ok().is_none_or(|id| id == 0) || uri != format!("steam://rungameid/{appid}")))
            || (source == "epic" && !uri.starts_with("com.epicgames.launcher://apps/"))
            || (source == "local" && !uri.is_empty())
        { return Err("备份游戏平台启动地址无效".into()); }
    }
    let mut positions=HashSet::new();
    for (entity,relation) in [("library_tags","game_tags"),("library_collections","game_collections")] {
        let mut names=HashSet::new();let mut entities=HashSet::new();
        for row in &data.tables[entity] {
            let id=row[0].as_str().ok_or("分类标识无效")?;
            uuid::Uuid::parse_str(id).map_err(|_|"分类标识无效")?;
            let name=row[1].as_str().ok_or("分类名称无效")?;
            if crate::organization::store::name(name)? != name || !names.insert(name.to_lowercase()) {return Err("分类名称重复或无效".into());}
            entities.insert(id);
            if entity=="library_collections" && !positions.insert(row[2].as_i64().ok_or("收藏夹顺序无效")?) {return Err("收藏夹顺序重复".into());}
        }
        for row in &data.tables[relation] {
            if !row[0].as_str().is_some_and(|id|ids.contains(id)) || !row[1].as_str().is_some_and(|id|entities.contains(id)) {return Err("分类关联引用不存在的游戏或分类".into());}
        }
    }
    let mut sessions = std::collections::HashMap::new();
    for row in &data.tables["play_sessions"] {
        let id = row[0].as_str().ok_or("游玩会话无效")?;
        if sessions.insert(id, row[1].as_str().unwrap_or("")).is_some() { return Err("备份游玩会话重复".into()); }
    }
    for name in ["unlocks","game_activity","play_sessions","daily_playtime","statistics_unlock_events"] {
        for row in &data.tables[name] {
            let index = if ["play_sessions","daily_playtime"].contains(&name) { 1 } else { 0 };
            if !row[index].as_str().is_some_and(|id| ids.contains(id)) { return Err("备份记录引用了不存在的游戏".into()); }
            if name == "daily_playtime" && sessions.get(row[0].as_str().unwrap_or("")) != Some(&row[1].as_str().unwrap_or("")) {
                return Err("每日统计与游玩会话不一致".into());
            }
        }
    }
    for (key, text) in &data.settings {
        if key.starts_with("cover_cache:") {
            let value: Value = serde_json::from_str(text).map_err(|_| "封面索引无效")?;
            if !value.get("file").and_then(Value::as_str).is_some_and(cover_name) { return Err("封面索引文件名无效".into()); }
        } else if key.starts_with("steam_playtime:") || key.starts_with("steam_family_playtime:") || key.starts_with("achievement_platform:") {
            let value: Value = serde_json::from_str(text).map_err(|_| "平台缓存或关联资料无效")?;
            if !value.is_object() { return Err("平台缓存或关联资料格式无效".into()); }
        }
    }
    for key in ["minimize_on_launch","close_to_tray","achievement_notifications"] {
        if preferences.behavior.get(key).is_some_and(|value| !["true","false",""].contains(&value.as_str())) {
            return Err("备份行为开关无效".into());
        }
    }
    if let Some(text) = preferences.behavior.get("achievement_overlay_options").filter(|text| !text.is_empty()) {
        let value: Value = serde_json::from_str(text).map_err(|_| "成就提示设置无效")?;
        if !value.is_object() { return Err("成就提示设置无效".into()); }
    }
    Ok(())
}
