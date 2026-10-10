use super::{auth::Session, catalog::Catalog};
use crate::{db, model::Game};
use rusqlite::Connection;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashSet;

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ImportResult {
    pub added: usize,
    pub shared: usize,
    pub owned: usize,
    pub unavailable: usize,
    pub excluded: usize,
    pub family_name: String,
    pub has_family: bool,
    pub time_games: usize,
}

pub(crate) fn is_shared(game: &Game) -> bool {
    game.source == "steam" && serde_json::from_str::<Value>(&game.metadata_json).ok()
        .is_some_and(|info| info.pointer("/steamFamily/shared").and_then(Value::as_bool) == Some(true))
}

pub(crate) fn require_available(game: &Game) -> Result<(), String> {
    let info: Value = serde_json::from_str(&game.metadata_json).unwrap_or_default();
    if game.source == "steam" && info.pointer("/steamFamily/shared").and_then(Value::as_bool) == Some(true)
        && info.pointer("/steamFamily/available").and_then(Value::as_bool) == Some(false)
    {
        return Err("此游戏的家庭共享已不可用，请重新导入家庭库确认授权".into());
    }
    Ok(())
}

pub(crate) fn save(conn: &mut Connection, session: &Session, catalog: Catalog) -> Result<ImportResult, String> {
    let transaction = conn.transaction().map_err(|error| error.to_string())?;
    let mut result = ImportResult {
        excluded: catalog.excluded, family_name: session.family_name.clone(),
        has_family: session.family_id.is_some(), ..Default::default()
    };
    let mut available = HashSet::new();
    result.time_games = super::playtime::save(&transaction, &session.steam_id, &catalog)?.games;
    let synced_at = chrono::Utc::now().to_rfc3339();
    for item in catalog.items {
        let id = format!("steam-{}", item.appid);
        available.insert(id.clone());
        let existing = db::game(&transaction, &id)?;
        if existing.as_ref().is_some_and(|game| game.source != "steam" || game.appid != item.appid) {
            return Err("游戏库中的 Steam 编号存在冲突，已停止导入".into());
        }
        if existing.is_none() { result.added += 1; }
        if item.shared { result.shared += 1; } else { result.owned += 1; }
        let game = existing.unwrap_or_else(|| Game {
            id, source: "steam".into(), appid: item.appid.clone(), title: item.title,
            launch_uri: format!("steam://rungameid/{}", item.appid), metadata_json: "{}".into(),
            ..Default::default()
        });
        let mut info: Value = serde_json::from_str(&game.metadata_json).map_err(|_| "已有 Steam 游戏资料无效")?;
        if !info.is_object() { return Err("已有 Steam 游戏资料格式无效".into()); }
        info["steamFamily"] = serde_json::json!({
            "shared":item.shared,"available":true,"accountId":session.steam_id,
            "familyGroupId":session.family_id,"ownerSteamIds":item.owners,"syncedAt":synced_at,
        });
        // 不导入家庭成员的游玩时长，不覆盖封面、安装路径、成就与收藏。
        if db::game(&transaction, &game.id)?.is_none() { db::upsert_game(&transaction, &game)?; }
        db::save_metadata(&transaction, &game.id, &info)?;
    }
    for game in db::games(&transaction)? {
        if available.contains(&game.id) || !is_shared(&game) { continue; }
        let mut info: Value = serde_json::from_str(&game.metadata_json).map_err(|_| "Steam 家庭共享资料无效")?;
        let was_available = info.pointer("/steamFamily/available").and_then(Value::as_bool) != Some(false);
        info["steamFamily"]["available"] = false.into();
        info["steamFamily"]["syncedAt"] = synced_at.clone().into();
        db::save_metadata(&transaction, &game.id, &info)?;
        if was_available { result.unavailable += 1; }
    }
    super::auth::save(&transaction, session)?;
    transaction.commit().map_err(|error| error.to_string())?;
    Ok(result)
}

pub(crate) fn promote_owned(conn: &Connection, appid: &str, steam_id: &str) -> Result<(), String> {
    let Some(game) = db::game(conn, &format!("steam-{appid}"))? else { return Ok(()) };
    if !is_shared(&game) { return Ok(()); }
    let mut info: Value = serde_json::from_str(&game.metadata_json).map_err(|_| "Steam 家庭共享资料无效")?;
    info["steamFamily"]["shared"] = false.into();
    info["steamFamily"]["available"] = true.into();
    info["steamFamily"]["accountId"] = steam_id.into();
    db::save_metadata(conn, &game.id, &info)
}
