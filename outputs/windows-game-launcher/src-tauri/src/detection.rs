use crate::{db, grime, lock_db, record_paths, scanner, AppState};
use serde::Serialize;
use tauri::Manager;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    path: String,
    exists: bool,
    kind: &'static str,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    state: &'static str,
    message: String,
    schema_source: String,
    source_file: String,
    last_scan: String,
    definitions: usize,
    unlocked: usize,
    candidates: Vec<Candidate>,
}

#[tauri::command]
pub(crate) fn check_detection(app: tauri::AppHandle, game_id: String) -> Result<Report, String> {
    let state = app.state::<AppState>();
    let mut game = db::game(&*lock_db(&state)?, &game_id)?.ok_or("游戏不存在")?;
    if game.source != "local" {
        return Err("Steam 游戏通过官方接口同步成就".into());
    }
    scanner::scan_now(app.clone(), game_id.clone())?;
    game = db::game(&*lock_db(&state)?, &game_id)?.ok_or("游戏不存在")?;
    let achievements = db::achievements(&*lock_db(&state)?, &game_id)?;
    let profile = crate::achievement_platform::for_game(&*lock_db(&state)?, &game)?;
    let mut candidates: Vec<_> = record_paths::runtime_candidates(
        &game,
        &profile,
        state.appdata.as_deref(),
        state.public.as_deref(),
    )
    .into_iter()
    .map(|path| Candidate {
        exists: path.is_file(),
        path: path.to_string_lossy().into_owned(),
        kind: "解锁记录",
    })
    .collect();
    if game.appid == grime::APPID {
        if let Some(folder) = state
            .appdata
            .as_deref()
            .and_then(|path| path.parent())
            .map(|path| path.join("LocalLow/Clover Bite/GRIME/Save Files"))
        {
            candidates.push(Candidate {
                exists: folder.is_dir(),
                path: folder.to_string_lossy().into_owned(),
                kind: "GRIME 存档",
            });
        }
    }
    let status = report_state(&game, achievements.len());
    Ok(Report {
        state: status,
        message: game.scan_status,
        schema_source: game.schema_source,
        source_file: game.source_file,
        last_scan: game.last_scan,
        definitions: achievements.len(),
        unlocked: achievements
            .iter()
            .filter(|item| item.unlocked_at.is_some())
            .count(),
        candidates,
    })
}

fn report_state(game: &crate::model::Game, definitions: usize) -> &'static str {
    if game.schema_source == crate::xbox_local::SCHEMA
        || game.schema_source == crate::xbox::SCHEMA
        || game.scan_status.contains("Xbox 本地")
    {
        return if game.scan_status.contains("失败") {
            "error"
        } else if game.scan_status.starts_with("已读取") {
            "ready"
        } else {
            "missing_record"
        };
    }
    let detected = game.scan_status.starts_with("已读取");
    let partial = game.schema_source == "本地记录（仅已解锁）";
    if game.appid.is_empty() {
        "needs_appid"
    } else if game.scan_status.contains("缺少成就定义") {
        "needs_definitions"
    } else if game.scan_status.contains("格式无效")
        || game.scan_status.contains("无法读取")
        || game.scan_status.contains("失败")
    {
        "error"
    } else if definitions == 0 || partial {
        "needs_definitions"
    } else if game.scan_status.starts_with("当前无法自动检测") {
        "unsupported"
    } else if detected {
        "ready"
    } else {
        "missing_record"
    }
}

#[tauri::command]
pub(crate) fn set_record_path(
    app: tauri::AppHandle,
    game_id: String,
    path: String,
) -> Result<Report, String> {
    if !path.is_empty() && !std::path::Path::new(&path).is_file() {
        return Err("记录文件不存在".into());
    }
    let state = app.state::<AppState>();
    let mut initialized = state.initialized.lock().map_err(|_| "扫描状态暂时不可用")?;
    {
        let conn = lock_db(&state)?;
        let mut game = db::game(&conn, &game_id)?.ok_or("游戏不存在")?;
        if game.source != "local" {
            return Err("只有本地游戏可以指定成就记录".into());
        }
        game.custom_unlock_path = path;
        db::upsert_game(&conn, &game)?;
    }
    initialized.remove(&game_id);
    drop(initialized);
    check_detection(app, game_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn definitions_alone_do_not_claim_automatic_detection() {
        let mut game = crate::model::Game {
            appid: "480".into(),
            schema_source: "Steam Web API".into(),
            scan_status: "未找到解锁记录文件".into(),
            ..Default::default()
        };
        assert_eq!(report_state(&game, 20), "missing_record");
        game.scan_status = "已读取本地记录".into();
        assert_eq!(report_state(&game, 20), "ready");
        game.schema_source = "本地记录（仅已解锁）".into();
        assert_eq!(report_state(&game, 2), "needs_definitions");
        game.scan_status = "记录格式无效：不是解锁记录".into();
        assert_eq!(report_state(&game, 2), "error");
        game.appid.clear();
        assert_eq!(report_state(&game, 2), "needs_appid");
    }
}
