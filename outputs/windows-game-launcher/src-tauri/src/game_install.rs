use crate::{db, installation::{Info, Status}, lock_db, model::Game, AppState};
use tauri::{Emitter, Manager};

#[tauri::command]
pub(crate) async fn install_game(app: tauri::AppHandle, game_id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || install(&app, &game_id))
        .await.map_err(|error| error.to_string())?
}

fn install(app: &tauri::AppHandle, game_id: &str) -> Result<(), String> {
    let state = app.state::<AppState>();
    let _gate = crate::maintenance::lock_operation(&state.operation_gate, &state.maintenance)?;
    let game = db::game(&*lock_db(&state)?, game_id)?.ok_or("游戏不存在")?;
    let runtime = state.runtime.lock().map_err(|_| "运行状态暂时不可用")?.info(game_id);
    let installation = if game.source == "steam" {
        let info = state.installation.check_before_launch(&game)?;
        // 即使重新检查后拒绝安装，也让界面得到最新状态。
        let _ = app.emit("library-changed", game_id);
        Some(info)
    } else {
        None
    };
    let uri = prepare(&game, installation.as_ref(), &runtime.state)?;
    // 安装只打开平台客户端，不预留游戏运行会话、不触发计时或自动最小化。
    open_install_uri(&uri, &game.source)
}

fn prepare(game: &Game, installation: Option<&Info>, runtime: &str) -> Result<String, String> {
    if matches!(runtime, "starting" | "running") {
        return Err("游戏正在启动或运行中，不能打开安装入口".into());
    }
    crate::steam_family::require_available(game)?;
    match game.source.as_str() {
        "steam" => {
            let info = installation.ok_or("尚未确认 Steam 安装状态")?;
            match info.state {
                Status::NotInstalled => {}
                Status::Installed => return Err("游戏已安装，请使用开始游戏".into()),
                _ => return Err(if info.reason.is_empty() { "尚未确认 Steam 安装状态".into() } else { info.reason.clone() }),
            }
            let appid = game.appid.parse::<u32>().ok().filter(|id| *id != 0).ok_or("Steam AppID 无效")?;
            Ok(format!("steam://install/{appid}"))
        }
        "epic" => {
            let metadata: serde_json::Value = serde_json::from_str(&game.metadata_json)
                .map_err(|_| "Epic 游戏资料无效，请重新导入游戏库")?;
            let name = metadata.get("epicAppName").and_then(serde_json::Value::as_str)
                .filter(|name| !name.trim().is_empty()).ok_or("Epic 游戏标识缺失，请重新导入游戏库")?;
            if game.id != format!("epic-{name}") {
                return Err("Epic 游戏标识不一致，请重新导入游戏库".into());
            }
            Ok(format!("com.epicgames.launcher://apps/{}?action=install", crate::epic_library::identifier(name)))
        }
        _ => Err("此游戏没有平台安装入口".into()),
    }
}

#[cfg(windows)]
fn open_install_uri(uri: &str, source: &str) -> Result<(), String> {
    use windows_sys::Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL};
    let operation: Vec<u16> = "open\0".encode_utf16().collect();
    let target: Vec<u16> = uri.encode_utf16().chain(Some(0)).collect();
    let result = unsafe {
        ShellExecuteW(std::ptr::null_mut(), operation.as_ptr(), target.as_ptr(),
            std::ptr::null(), std::ptr::null(), SW_SHOWNORMAL)
    } as isize;
    if result > 32 {
        Ok(())
    } else {
        let client = if source == "steam" { "Steam" } else { "Epic Games Launcher" };
        Err(format!("无法打开安装入口，请确认已安装 {client}，且客户端链接关联正常（错误 {result}）"))
    }
}

#[cfg(not(windows))]
fn open_install_uri(_uri: &str, _source: &str) -> Result<(), String> {
    Err("游戏安装入口仅在 Windows 桌面端提供".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn steam() -> Game {
        Game { id:"steam-480".into(), source:"steam".into(), appid:"480".into(), ..Default::default() }
    }
    fn info(state: Status) -> Info {
        Info { state, reason:"检测说明".into(), checked_at:String::new() }
    }
    #[test]
    fn only_confirmed_uninstalled_steam_games_can_request_installation() {
        assert_eq!(prepare(&steam(), Some(&info(Status::NotInstalled)), "idle").unwrap(), "steam://install/480");
        for status in [Status::Installed, Status::Checking, Status::Unknown, Status::ClientMissing] {
            assert!(prepare(&steam(), Some(&info(status)), "idle").is_err());
        }
        assert!(prepare(&steam(), None, "idle").is_err());
    }
    #[test]
    fn active_games_and_expired_sharing_cannot_request_installation() {
        let mut game = steam();
        for runtime in ["starting", "running"] {
            assert!(prepare(&game, Some(&info(Status::NotInstalled)), runtime).is_err());
        }
        game.metadata_json = r#"{"steamFamily":{"shared":true,"available":false}}"#.into();
        assert!(prepare(&game, Some(&info(Status::NotInstalled)), "idle").is_err());
        game.metadata_json = r#"{"steamFamily":{"shared":true,"available":true}}"#.into();
        assert!(prepare(&game, Some(&info(Status::NotInstalled)), "idle").is_ok());
    }
    #[test]
    fn epic_install_uses_encoded_app_name_and_does_not_require_installation_detection() {
        let name = "game name/?&action=launch#中文";
        let game = Game { id:format!("epic-{name}"), source:"epic".into(),
            launch_uri:"https://untrusted.example".into(),
            metadata_json:serde_json::json!({"epicAppName":name}).to_string(), ..Default::default() };
        assert_eq!(prepare(&game, None, "idle").unwrap(),
            "com.epicgames.launcher://apps/game%20name%2F%3F%26action%3Dlaunch%23%E4%B8%AD%E6%96%87?action=install");
        assert!(prepare(&game, None, "running").is_err());
        assert!(prepare(&Game { id:"epic-other".into(), ..game.clone() }, None, "idle").is_err());
        assert!(prepare(&Game { metadata_json:"{}".into(), ..game }, None, "idle").is_err());
    }
    #[test]
    fn invalid_platform_and_appid_are_rejected() {
        assert!(prepare(&Game { source:"local".into(), ..steam() }, None, "idle").is_err());
        for appid in ["0", "-1", "480?x=1", "4294967296"] {
            assert!(prepare(&Game { appid:appid.into(), ..steam() }, Some(&info(Status::NotInstalled)), "idle").is_err());
        }
    }
}
