use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ResourceType {
    Game, Title, MetadataReference, AchievementReference, Archive, Favorite,
    Tag, Collection, TagMember, CollectionMember, CollectionOrder,
    PlatformAccount, AutomaticUnlock, ManualUnlock, OfficialSnapshot,
    PlaySession, HistoricalBaseline, UnlockEvent, DefinitionCache, OfficialPlaytime,
}
impl ResourceType {
    pub fn key(self) -> String { serde_json::to_value(self).unwrap().as_str().unwrap().into() }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Game {
    pub source: String, pub source_identity: String, pub display_title: String,
    pub metadata_reference: String, pub achievement_reference: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextValue { pub value: String }
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BoolValue { pub value: bool }
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Named { pub name: String }
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Member { pub game_id: Uuid, pub category_id: Uuid, pub present: bool }
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CollectionOrder { pub ids: Vec<Uuid> }
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlatformAccount { pub provider: String, pub account_id: String }
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Unlock {
    pub game_id: Uuid, pub namespace: String, pub api_name: String,
    pub source: String, pub unlocked_at: Option<String>, pub present: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OfficialSnapshot {
    pub game_id: Uuid, pub provider: String, pub account_id: String, pub namespace: String,
    pub unlocks: BTreeMap<String, Option<String>>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlaySession {
    pub game_id: Uuid, pub owner_device_id: Uuid, pub checkpoint: u64,
    pub started_at: String, pub ended_at: Option<String>, pub seconds: u64,
    pub days: BTreeMap<String, u64>, pub utc_offset_seconds: i32,
    pub historical: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Baseline { pub game_id: Uuid, pub seconds: u64 }
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UnlockEvent {
    pub game_id: Uuid, pub namespace: String, pub api_name: String,
    pub recorded_at: String, pub date: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Definition {
    pub api_name: String, pub name: String, pub description: String, pub icon: String, pub hidden: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DefinitionCache {
    pub game_id: Uuid, pub namespace: String, pub language: String, pub source: String,
    pub complete: bool, pub definitions: Vec<Definition>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OfficialPlaytime {
    pub game_id: Uuid, pub provider: String, pub account_id: String,
    pub seconds: u64, pub checked_at: String,
}

pub fn decode<T: serde::de::DeserializeOwned>(v: &Value) -> Result<T, String> {
    serde_json::from_value(v.clone()).map_err(|_| "同步数据包含无效或未允许的字段".into())
}
pub fn timestamp(s: &str) -> Result<(), String> {
    chrono::DateTime::parse_from_rfc3339(s).map(|_| ()).map_err(|_| "时间格式无效".into())
}
pub fn name(s: &str) -> Result<(), String> {
    if s != s.trim() || s.is_empty() || s.chars().count() > 40 || s.chars().any(char::is_control) {
        return Err("分类名称须为 1–40 字符，不能包含控制字符或首尾空白".into());
    } Ok(())
}
pub fn identifier(s: &str) -> Result<(), String> {
    if s.is_empty() || s.len() > 256 || s.chars().any(|c| c.is_control() || c == '/' || c == '\\') {
        return Err("资源标识无效".into());
    } Ok(())
}

pub fn validate(kind: ResourceType, id: &str, value: &Value) -> Result<(), String> {
    identifier(id)?;
    match kind {
        ResourceType::Game => {
            Uuid::parse_str(id).map_err(|_| "游戏 ID 无效")?;
            let v: Game = decode(value)?;
            if !["steam", "epic", "local"].contains(&v.source.as_str()) || v.display_title.is_empty() || v.display_title.len() > 1024 || v.display_title.chars().any(char::is_control) { return Err("游戏来源或名称无效".into()); }
            identifier(&v.source_identity)?;
            if v.source == "steam" && v.source_identity.parse::<u32>().ok().is_none_or(|x| x == 0) { return Err("Steam 身份无效".into()); }
            if v.source == "local" && v.source_identity.strip_prefix("local-").is_none_or(|id|Uuid::parse_str(id).is_err()) { return Err("本地游戏身份无效".into()); }
            if v.source == "epic" && !v.source_identity.split_once(':').is_some_and(|(namespace,app)|{
                [namespace,app].iter().all(|part|!part.is_empty() && part.chars().all(|c|c.is_ascii_alphanumeric() || "-_.".contains(c)))
            }) { return Err("Epic 身份必须包含有效的 namespace 和 appName".into()); }
            if !v.metadata_reference.is_empty() { identifier(&v.metadata_reference)?; }
            if !v.achievement_reference.is_empty() { identifier(&v.achievement_reference)?; }
        }
        ResourceType::Title | ResourceType::MetadataReference | ResourceType::AchievementReference => {
            let v: TextValue = decode(value)?;
            if v.value.len() > 1024 || v.value.chars().any(char::is_control) { return Err("文本无效".into()); }
            if kind != ResourceType::Title && !v.value.is_empty() { identifier(&v.value)?; }
        }
        ResourceType::Favorite | ResourceType::Archive => { let _: BoolValue = decode(value)?; }
        ResourceType::Tag | ResourceType::Collection => { Uuid::parse_str(id).map_err(|_| "分类 ID 无效")?; name(&decode::<Named>(value)?.name)?; }
        ResourceType::TagMember | ResourceType::CollectionMember => {
            let v: Member = decode(value)?;
            if id != format!("{}:{}", v.game_id, v.category_id) { return Err("成员身份不匹配".into()); }
        }
        ResourceType::CollectionOrder => {
            let v: CollectionOrder = decode(value)?;
            if id != "order" || v.ids.iter().collect::<std::collections::HashSet<_>>().len() != v.ids.len() { return Err("收藏夹顺序无效".into()); }
        }
        ResourceType::PlatformAccount => {
            let v: PlatformAccount = decode(value)?;
            if !["steam", "epic"].contains(&v.provider.as_str()) || v.provider != id { return Err("平台身份无效".into()); }
            identifier(&v.account_id)?;
        }
        ResourceType::ManualUnlock | ResourceType::AutomaticUnlock => {
            let v: Unlock = decode(value)?;
            identifier(&v.namespace)?; identifier(&v.api_name)?;
            if let Some(time) = v.unlocked_at { timestamp(&time)?; }
            if (kind == ResourceType::ManualUnlock && v.source != "manual") || (kind == ResourceType::AutomaticUnlock && (!v.present || !["local", "xbox-local"].contains(&v.source.as_str()))) { return Err("解锁来源无效".into()); }
            if id != unlock_id(v.game_id, &v.namespace, &v.api_name, &v.source) { return Err("解锁身份不匹配".into()); }
        }
        ResourceType::OfficialSnapshot => {
            let v: OfficialSnapshot = decode(value)?;
            if id != v.game_id.to_string() || !["steam", "epic"].contains(&v.provider.as_str()) { return Err("官方快照身份无效".into()); }
            identifier(&v.namespace)?; identifier(&v.account_id)?;
            for (key, time) in v.unlocks { identifier(&key)?; if let Some(time) = time { timestamp(&time)?; } }
        }
        ResourceType::PlaySession => {
            Uuid::parse_str(id).map_err(|_| "会话 ID 无效")?;
            let v: PlaySession = decode(value)?; timestamp(&v.started_at)?;
            if let Some(end) = &v.ended_at { timestamp(end)?; }
            if v.seconds > i64::MAX as u64 || v.checkpoint > i64::MAX as u64 || v.utc_offset_seconds.unsigned_abs() > 86400 { return Err("计时数据超出范围".into()); }
            let mut total = 0u64;
            for (day, seconds) in &v.days {
                chrono::NaiveDate::parse_from_str(day, "%Y-%m-%d").map_err(|_| "统计日期无效")?;
                total = total.checked_add(*seconds).ok_or("计时溢出")?;
            }
            if total > v.seconds || (!v.historical && total != v.seconds) { return Err("逐日计时与会话不一致".into()); }
        }
        ResourceType::HistoricalBaseline => { let v: Baseline = decode(value)?; if id != v.game_id.to_string() || v.seconds > i64::MAX as u64 { return Err("历史基线无效".into()); } }
        ResourceType::UnlockEvent => {
            let v: UnlockEvent = decode(value)?; timestamp(&v.recorded_at)?;
            chrono::NaiveDate::parse_from_str(&v.date, "%Y-%m-%d").map_err(|_| "统计日期无效")?;
            if id != unlock_id(v.game_id, &v.namespace, &v.api_name, "event") { return Err("事件身份无效".into()); }
        }
        ResourceType::DefinitionCache => {
            let v: DefinitionCache = decode(value)?;
            if id != v.game_id.to_string() || v.definitions.len() > 10000 { return Err("定义缓存身份或数量无效".into()); }
            identifier(&v.namespace)?;
            let mut names = std::collections::HashSet::new();
            for d in v.definitions { identifier(&d.api_name)?; if !names.insert(d.api_name) || (!d.icon.is_empty() && !d.icon.starts_with("https://")) { return Err("成就定义无效".into()); } }
        }
        ResourceType::OfficialPlaytime => { let v: OfficialPlaytime = decode(value)?; timestamp(&v.checked_at)?; if id != v.game_id.to_string() || !["steam", "epic"].contains(&v.provider.as_str()) || v.seconds > i64::MAX as u64 { return Err("官方时长无效".into()); } identifier(&v.account_id)?; }
    }
    Ok(())
}

pub fn unlock_id(game: Uuid, namespace: &str, api: &str, source: &str) -> String {
    crate::digest(&(game, namespace, api, source)).expect("字符串可以序列化")
}

#[cfg(test)]
mod identity_tests {
    #[test]
    fn epic_requires_both_identity_components() {
        let id=uuid::Uuid::new_v4().to_string();
        let mut payload=serde_json::json!({"source":"epic","sourceIdentity":"namespace:app-name","displayTitle":"游戏","metadataReference":"","achievementReference":""});
        assert!(super::validate(super::ResourceType::Game,&id,&payload).is_ok());
        for value in ["app-only",":app","namespace:","namespace:app?action=other","namespace:app:extra"] {
            payload["sourceIdentity"]=value.into();
            assert!(super::validate(super::ResourceType::Game,&id,&payload).is_err());
        }
    }
}
