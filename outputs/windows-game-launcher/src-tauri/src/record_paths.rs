use crate::{achievement_platform::Profile, model::Game};
use std::path::{Path, PathBuf};

fn valid_appid(value: &str) -> bool {
    value.parse::<u32>().is_ok_and(|id| id > 0) && value.bytes().all(|b| b.is_ascii_digit())
}

fn confirmed_runtime_appid<'a>(game: &Game, profile: &'a Profile) -> Option<&'a str> {
    // 配置编号只用于查找记录；资料和解锁仍归属于用户确认的游戏。
    (profile.platform == "steam"
        && profile.confirmed_steam_appid == game.appid
        && profile.record_conflicts.is_empty()
        && profile.steam_appid != game.appid
        && valid_appid(&profile.steam_appid)
        && crate::steam_identity::detect(Path::new(&game.exe_path)).appid == profile.steam_appid)
        .then_some(profile.steam_appid.as_str())
}

pub(crate) fn runtime_candidates(
    game: &Game,
    profile: &Profile,
    appdata: Option<&Path>,
    public: Option<&Path>,
) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if !game.custom_unlock_path.trim().is_empty() {
        candidates.push(PathBuf::from(&game.custom_unlock_path));
    }
    let mut ids = Vec::new();
    if valid_appid(&game.appid) {
        ids.push(game.appid.as_str());
    }
    if let Some(id) = confirmed_runtime_appid(game, profile) {
        ids.push(id);
    }
    for id in ids {
        if let Some(root) = appdata {
            for runtime in ["GSE Saves", "Goldberg SteamEmu Saves"] {
                candidates.push(root.join(runtime).join(id).join("achievements.json"));
            }
        }
        if let Some(root) = public {
            for runtime in ["RUNE", "CODEX"] {
                candidates.push(
                    root.join("Documents/Steam")
                        .join(runtime)
                        .join(id)
                        .join("achievements.ini"),
                );
            }
        }
    }
    candidates
}

pub(crate) fn runtime_file(
    game: &Game,
    profile: &Profile,
    appdata: Option<&Path>,
    public: Option<&Path>,
) -> Option<PathBuf> {
    runtime_candidates(game, profile, appdata, public)
        .into_iter()
        .find(|file| file.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_path_precedes_standard_paths() {
        let game = Game {
            appid: "480".into(),
            custom_unlock_path: "D:/custom.ini".into(),
            ..Default::default()
        };
        let files = runtime_candidates(
            &game,
            &Profile::default(),
            Some(Path::new("C:/Roaming")),
            Some(Path::new("C:/Users/Public")),
        );
        assert_eq!(files[0], PathBuf::from("D:/custom.ini"));
        assert!(files.contains(&PathBuf::from(
            "C:/Users/Public/Documents/Steam/RUNE/480/achievements.ini"
        )));
        assert_eq!(files.len(), 5);
    }

    #[test]
    fn alternate_directory_requires_confirmed_and_unchanged_identity() {
        let root = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("steam_emu.ini"), "[Settings]\nAppId=2456740\n").unwrap();
        let game = Game {
            appid: "2529790".into(),
            exe_path: root.join("GRIME II.exe").to_string_lossy().into(),
            ..Default::default()
        };
        let mut profile = Profile {
            platform: "steam".into(),
            steam_appid: "2456740".into(),
            ..Default::default()
        };
        assert!(confirmed_runtime_appid(&game, &profile).is_none());
        profile.confirmed_steam_appid = "2529790".into();
        assert_eq!(confirmed_runtime_appid(&game, &profile), Some("2456740"));
        let files = runtime_candidates(&game, &profile, None, Some(&root));
        assert!(files.contains(&root.join("Documents/Steam/RUNE/2456740/achievements.ini")));
        profile.record_conflicts = vec!["另一个游戏".into()];
        let files = runtime_candidates(&game, &profile, None, Some(&root));
        assert!(!files.contains(&root.join("Documents/Steam/RUNE/2456740/achievements.ini")));
        assert!(files.contains(&root.join("Documents/Steam/RUNE/2529790/achievements.ini")));
        profile.record_conflicts.clear();
        std::fs::write(root.join("steam_emu.ini"), "AppId=480").unwrap();
        assert!(confirmed_runtime_appid(&game, &profile).is_none());
        std::fs::remove_dir_all(root).unwrap();
    }
}
