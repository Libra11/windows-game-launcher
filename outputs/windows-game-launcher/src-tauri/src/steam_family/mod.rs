mod api;
mod auth;
mod catalog;
mod login;
pub(crate) mod playtime;
mod store;

pub(crate) use store::{is_shared, promote_owned, require_available};
use crate::{db, lock_db, AppState};
use std::sync::atomic::Ordering;
use tauri::{Emitter, Manager};

#[tauri::command]
pub(crate) async fn steam_family_begin_login(app: tauri::AppHandle) -> Result<(), String> {
    login::begin(&app).await
}

#[tauri::command]
pub(crate) fn steam_family_connection_status(app: tauri::AppHandle) -> Result<auth::Status, String> {
    auth::status(&app)
}

#[tauri::command]
pub(crate) async fn steam_family_disconnect(app: tauri::AppHandle) -> Result<(), String> {
    auth::disconnect(&app).await
}

#[tauri::command]
pub(crate) async fn refresh_steam_family_playtime(app: tauri::AppHandle) -> Result<playtime::ResultInfo, String> {
    playtime::refresh(&app).await
}

#[tauri::command]
pub(crate) async fn import_steam_family(app: tauri::AppHandle) -> Result<store::ImportResult, String> {
    let _guard = auth::AUTH_LOCK.lock().await;
    let generation = auth::LOGIN_GENERATION.load(Ordering::Acquire);
    let mut session = auth::session(&app)?;
    let family = api::family(&session.token, &session.steam_id).await?;
    session.family_name = family.name;
    session.family_id = family.id;
    let catalog = if let Some(id) = &session.family_id {
        let response = api::library(&session.token, &session.steam_id, id).await?;
        catalog::parse(&response, &session.steam_id)?
    } else {
        catalog::Catalog { items: Vec::new(), excluded: 0 }
    };
    if auth::LOGIN_GENERATION.load(Ordering::Acquire) != generation {
        return Err("Steam 家庭库连接已变更，已丢弃旧导入结果".into());
    }
    let result = {
        let state = app.state::<AppState>();
        let mut conn = lock_db(&state)?;
        let steam_id = db::setting(&conn, "steam_id")?;
        if !steam_id.is_empty() && steam_id != session.steam_id {
            return Err("Steam 账号已变更，已丢弃旧家庭库结果".into());
        }
        store::save(&mut conn, &session, catalog)?
    };
    crate::library_commands::start_metadata_refresh(app.clone());
    let _ = app.emit("library-changed", ());
    Ok(result)
}

#[cfg(test)]
mod tests;
