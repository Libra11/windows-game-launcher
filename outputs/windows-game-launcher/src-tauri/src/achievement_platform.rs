use crate::{db, model::Game};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Profile {
    pub platform: String,
    pub reason: String,
    pub title_id: String,
    pub scid: String,
    #[serde(default)]
    pub store_id: String,
    #[serde(default)]
    pub public_source: String,
    pub steam_appid: String,
    #[serde(default)]
    pub confirmed_steam_appid: String,
    pub automatic: bool,
    #[serde(default)]
    pub definition_error: String,
    #[serde(default)]
    pub record_conflicts: Vec<String>,
}

// 只比较当前配置，不递归加载平台资料；旧配置不能证明当前记录归属。
fn record_conflicts(
    conn: &Connection,
    appid: &str,
    runtime_id: &str,
    exclude: &str,
) -> Result<Vec<String>, String> {
    if runtime_id.is_empty() || appid.is_empty() || runtime_id == appid {
        return Ok(Vec::new());
    }
    Ok(db::games(conn)?
        .into_iter()
        .filter(|other| {
            other.id != exclude
                && !other.appid.is_empty()
                && other.appid != appid
                && (other.appid == runtime_id
                    || (other.source == "local"
                        && detect(&other.exe_path).steam_appid == runtime_id))
        })
        .map(|other| other.title)
        .collect())
}

fn read_small(path: &Path) -> Option<String> {
    (path.metadata().ok()?.len() < 1024 * 1024)
        .then(|| std::fs::read_to_string(path).ok())
        .flatten()
}

pub(crate) fn detect(exe: &str) -> Profile {
    let Some(folder) = Path::new(exe).parent() else {
        return Profile::default();
    };
    let mut profile = Profile {
        platform: "unknown".into(),
        reason: "未找到可靠的平台标识，请选择成就平台；仅有关联资料不能确认版本".into(),
        ..Default::default()
    };
    let config = std::fs::read_dir(folder).ok().and_then(|entries| {
        entries.flatten().find(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .eq_ignore_ascii_case("MicrosoftGame.config")
        })
    });
    if let Some(text) = config.and_then(|entry| read_small(&entry.path())) {
        if let Ok(xml) = roxmltree::Document::parse(text.trim_start_matches('\u{feff}')) {
            let title = xml
                .descendants()
                .find(|n| n.tag_name().name() == "TitleId")
                .and_then(|n| n.text())
                .unwrap_or("")
                .trim();
            if let Ok(id) = u32::from_str_radix(title.trim_start_matches("0x"), 16) {
                if id != 0 {
                    profile.title_id = format!("{id:08x}");
                    profile.scid = xml
                        .descendants()
                        .find(|n| {
                            n.tag_name().name() == "ExtendedAttribute"
                                && n.attribute("Name") == Some("Xbox.Services.Configuration")
                        })
                        .and_then(|n| n.attribute("Value"))
                        .unwrap_or("")
                        .to_owned();
                    profile.store_id = xml
                        .descendants()
                        .find(|n| n.tag_name().name() == "StoreId")
                        .and_then(|n| n.text())
                        .unwrap_or("")
                        .trim()
                        .to_owned();
                    profile.public_source =
                        crate::xbox::public::default_source(&profile.title_id).into();
                    profile.platform = "xbox".into();
                    profile.reason = "检测到 Xbox 配置及 Title ID；运行时接口用于进一步确认".into();
                }
            }
        }
    }
    let steam = crate::steam_identity::detect(Path::new(exe));
    profile.steam_appid = steam.appid;
    if steam.components || !profile.steam_appid.is_empty() || steam.conflicting_ids {
        if profile.platform == "xbox" {
            profile.platform = "conflict".into();
            profile.reason =
                "同时发现 Xbox 和 Steam 标识，请确认成就平台；目录可能包含其他游戏遗留文件".into();
        } else {
            profile.platform = "steam".into();
            profile.reason = if steam.conflicting_ids {
                "检测到 Steam 组件，但配置包含多个不同 AppID，请手动确认对应游戏"
            } else if profile.steam_appid.is_empty() {
                "检测到 Steam 接口组件，未找到有效 AppID，请搜索并确认对应游戏"
            } else {
                "检测到本地 Steam 配置；配置编号需与所选游戏核对，自动解锁还需受支持的真实记录"
            }
            .into();
        }
    }
    profile.automatic = profile.title_id == "64439fe5"
        && Path::new(exe)
            .file_name()
            .is_some_and(|s| s.to_string_lossy().eq_ignore_ascii_case("WellDweller.exe"))
        && folder.join("GDKExtension.dll").is_file();
    profile
}

pub(crate) fn for_game(conn: &Connection, game: &Game) -> Result<Profile, String> {
    if game.source == "epic" {
        return Ok(Profile { platform: "epic".into(), reason: "Epic 账号官方成就同步".into(), automatic:true, definition_error:db::setting(conn, &format!("achievement_definition_error:{}", game.id))?, ..Default::default() });
    }
    if game.source == "steam" {
        return Ok(Profile {
            platform: "steam".into(),
            reason: "Steam 官方游戏库".into(),
            steam_appid: game.appid.clone(),
            automatic: true,
            ..Default::default()
        });
    }
    let saved = db::setting(conn, &format!("achievement_platform:{}", game.id))?;
    if !saved.is_empty() {
        let mut p: Profile = serde_json::from_str(&saved).map_err(|_| "成就平台配置无效")?;
        if p.public_source.is_empty() {
            p.public_source = crate::xbox::public::default_source(&p.title_id).into();
        }
        if p.store_id.is_empty() {
            p.store_id = detect(&game.exe_path).store_id;
        }
        p.definition_error =
            db::setting(conn, &format!("achievement_definition_error:{}", game.id))?;
        p.record_conflicts = record_conflicts(conn, &game.appid, &p.steam_appid, &game.id)?;
        return Ok(p);
    }
    let mut profile = detect(&game.exe_path);
    if game.schema_source == "Steam Web API"
        || game.schema_source == "本地定义文件"
        || game.schema_source == "本地记录（仅已解锁）"
    {
        profile.platform = "steam".into();
    }
    // 已验证的接口身份优先于该游戏目录中无关的 Steam 配置。
    if crate::xbox_local::supported(game) {
        profile.platform = "xbox".into();
    }
    profile.definition_error =
        db::setting(conn, &format!("achievement_definition_error:{}", game.id))?;
    profile.record_conflicts = record_conflicts(conn, &game.appid, &profile.steam_appid, &game.id)?;
    Ok(profile)
}

pub(crate) fn namespace(conn: &Connection, game: &Game) -> Result<String, String> {
    let p = for_game(conn, game)?;
    Ok(match p.platform.as_str() {
        "xbox" => format!("xbox:{}", p.title_id),
        "steam" => game.appid.clone(),
        "epic" => format!("epic:{}", game.id),
        _ => format!("unmatched:{}", game.id),
    })
}

pub(crate) fn save(
    conn: &Connection,
    game: &Game,
    platform: &str,
    public_source: &str,
    steam_identity_confirmed: bool,
) -> Result<(), String> {
    let mut p = detect(&game.exe_path);
    if !["steam", "xbox", "none"].contains(&platform) {
        return Err("请选择明确的成就平台".into());
    }
    if platform == "xbox" && p.title_id.is_empty() {
        return Err("未检测到 Xbox Title ID，无法查询对应成就，请选择正确的游戏启动文件".into());
    }
    if platform == "steam" && game.appid.is_empty() {
        return Err("Steam 成就需要关联正确的 Steam 游戏".into());
    }
    if platform == "steam"
        && !p.steam_appid.is_empty()
        && p.steam_appid != game.appid
        && !steam_identity_confirmed
    {
        return Err(format!(
            "目录中的 Steam AppID {} 与所选游戏 {} 不一致，请明确确认以所选游戏获取资料，或更换启动文件",
            p.steam_appid, game.appid
        ));
    }
    if platform == "steam" && p.steam_appid != game.appid && !p.steam_appid.is_empty() {
        p.confirmed_steam_appid = game.appid.clone();
        p.reason = format!(
            "已手动确认 Steam 资料 AppID {}；本地配置编号 {} 不一致，自动解锁仍需匹配的真实记录",
            game.appid, p.steam_appid
        );
    }
    p.platform = platform.into();
    if platform == "xbox" && !public_source.trim().is_empty() {
        crate::xbox::public::validate_url(public_source.trim())?;
        p.public_source = public_source.trim().into();
    }
    db::set_setting(
        conn,
        &format!("achievement_platform:{}", game.id),
        &serde_json::to_string(&p).map_err(|e| e.to_string())?,
    )
}

#[tauri::command]
pub(crate) fn detect_game_platform(
    state: tauri::State<'_, crate::AppState>,
    exe_path: String,
    appid: Option<String>,
    game_id: Option<String>,
) -> Result<Profile, String> {
    if !Path::new(&exe_path).is_file() {
        return Err("请选择存在的启动文件".into());
    }
    let mut profile = detect(&exe_path);
    let conn = crate::lock_db(&state)?;
    profile.record_conflicts = record_conflicts(
        &conn,
        appid.as_deref().unwrap_or(""),
        &profile.steam_appid,
        game_id.as_deref().unwrap_or(""),
    )?;
    Ok(profile)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn detects_shared_configuration_and_refreshes_after_repair() {
        let folder = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("steam_emu.ini"), "AppId=2456740").unwrap();
        let conn = db::open(&folder.join("games.sqlite")).unwrap();
        let other = Game { id:"other".into(), source:"local".into(), title:"GRIME II".into(), appid:"2529790".into(), exe_path:folder.join("game.exe").to_string_lossy().into(), ..Default::default() };
        db::upsert_game(&conn, &other).unwrap();
        assert_eq!(record_conflicts(&conn, "1875580", "2456740", "mina").unwrap(), vec!["GRIME II"]);
        assert!(record_conflicts(&conn, "1875580", "2456740", "other").unwrap().is_empty());
        assert!(record_conflicts(&conn, "2529790", "2456740", "mina").unwrap().is_empty());
        std::fs::write(folder.join("steam_emu.ini"), "AppId=2529790").unwrap();
        assert!(record_conflicts(&conn, "1875580", "2456740", "mina").unwrap().is_empty());
        assert_eq!(record_conflicts(&conn, "1875580", "2529790", "mina").unwrap(), vec!["GRIME II"]);
        drop(conn);
        std::fs::remove_dir_all(folder).unwrap();
    }
    #[test]
    fn recognizes_xbox_and_reports_conflicting_steam_id() {
        let folder = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(
            folder.join("MicrosoftGame.config"),
            "<Game><TitleId>64439FE5</TitleId></Game>",
        )
        .unwrap();
        let exe = folder.join("game.exe").to_string_lossy().into_owned();
        assert_eq!(detect(&exe).platform, "xbox");
        std::fs::write(folder.join("steam_appid.txt"), "480").unwrap();
        assert_eq!(detect(&exe).platform, "conflict");
        std::fs::remove_dir_all(folder).unwrap();
    }
    #[test]
    fn recognizes_unity_plugin_steam_identity() {
        let folder = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
        let settings = folder.join("GRIME_Data/Plugins/x86_64/steam_settings");
        std::fs::create_dir_all(&settings).unwrap();
        std::fs::write(settings.join("steam_appid.txt"), "1123050\n").unwrap();
        let profile = detect(&folder.join("GRIME.exe").to_string_lossy());
        assert_eq!(profile.platform, "steam");
        assert_eq!(profile.steam_appid, "1123050");
        std::fs::remove_dir_all(folder).unwrap();
    }
    #[test]
    fn mismatched_steam_identity_requires_explicit_confirmation() {
        let folder = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("steam_emu.ini"), "[Steam]\nAppId=2456740").unwrap();
        let conn = db::open(&folder.join("games.sqlite")).unwrap();
        let game = Game {
            id: "local-grime2".into(),
            source: "local".into(),
            appid: "2529790".into(),
            exe_path: folder.join("GRIME II.exe").to_string_lossy().into_owned(),
            ..Default::default()
        };
        assert_eq!(detect(&game.exe_path).platform, "steam");
        assert!(save(&conn, &game, "steam", "", false).is_err());
        save(&conn, &game, "steam", "", true).unwrap();
        let profile = for_game(&conn, &game).unwrap();
        assert_eq!(profile.steam_appid, "2456740");
        assert_eq!(profile.confirmed_steam_appid, "2529790");
        assert_eq!(namespace(&conn, &game).unwrap(), "2529790");
        assert_eq!(
            std::fs::read_to_string(folder.join("steam_emu.ini")).unwrap(),
            "[Steam]\nAppId=2456740"
        );
        drop(conn);
        std::fs::remove_dir_all(folder).unwrap();
    }
}
