use std::path::{Path, PathBuf};
use tauri::Manager;
use youji_cloud_protocol::save_policy::DirectoryRule;
pub fn bases(
    app: &tauri::AppHandle,
    game_id: &str,
) -> Result<Vec<(String, PathBuf, Option<String>)>, String> {
    let mut out = Vec::new();
    #[cfg(windows)]
    {
        use windows::Win32::{
            System::Com::CoTaskMemFree,
            UI::Shell::{
                FOLDERID_Documents, FOLDERID_LocalAppData, FOLDERID_LocalAppDataLow, FOLDERID_RoamingAppData,
                FOLDERID_SavedGames, SHGetKnownFolderPath, KF_FLAG_DEFAULT,
            },
        };
        for (name, id) in [
            ("localAppData", FOLDERID_LocalAppData),
            ("localLow", FOLDERID_LocalAppDataLow),
            ("roamingAppData", FOLDERID_RoamingAppData),
            ("documents", FOLDERID_Documents),
            ("savedGames", FOLDERID_SavedGames),
        ] {
            unsafe {
                if let Ok(ptr) = SHGetKnownFolderPath(&id, KF_FLAG_DEFAULT, None) {
                    let s = ptr.to_string();
                    CoTaskMemFree(Some(ptr.0.cast()));
                    if let Ok(s) = s {
                        out.push((name.into(), PathBuf::from(s), None));
                    }
                }
            }
        }
    }
    let state = app.state::<crate::AppState>();
    let c = crate::lock_db(&state)?;
    let game = crate::db::game(&c, game_id)?.ok_or("游戏不存在")?;
    let account = crate::db::setting(&c, "steam_id")?;
    drop(c);
    if !game.exe_path.is_empty() {
        if let Some(parent) = Path::new(&game.exe_path).parent() {
            out.push(("gameInstall".into(), parent.into(), None));
        }
    }
    if let Ok(steam) = steamlocate::locate() {
        if let Some(account) = account
            .parse::<u64>()
            .ok()
            .and_then(|v| v.checked_sub(76561197960265728))
            .filter(|v| *v <= u32::MAX as u64)
        {
            out.push((
                "steamUserData".into(),
                steam.path().join("userdata").join(account.to_string()),
                Some(account.to_string()),
            ));
        }
        if game.source == "steam" {
            if let Ok(id) = game.appid.parse::<u32>() {
                if let Ok(libraries) = steam.library_paths() {
                    for path in libraries {
                        if let Ok(library) = steamlocate::Library::from_dir(&path) {
                            if let Some(Ok(app)) = library.app(id) {
                                if app.app_id == id
                                    && !app.install_dir.is_empty()
                                    && Path::new(&app.install_dir)
                                        .components()
                                        .all(|c| matches!(c, std::path::Component::Normal(_)))
                                {
                                    out.push((
                                        "gameInstall".into(),
                                        library.resolve_app_dir(&app),
                                        None,
                                    ));
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(out)
}
pub fn infer(root: &Path, bases: &[(String, PathBuf, Option<String>)]) -> Option<DirectoryRule> {
    let root = root.canonicalize().ok()?;
    // 最具体的基目录优先，禁止记录绝对路径或用户目录名。
    let mut sorted = bases.to_vec();
    sorted.sort_by_key(|(_, p, _)| std::cmp::Reverse(p.components().count()));
    sorted.into_iter().find_map(|(base, path, account)| {
        let path = path.canonicalize().ok()?;
        let relative = root.strip_prefix(path).ok()?.to_str()?.replace('\\', "/");
        let rule = DirectoryRule {
            base,
            relative,
            account,
        };
        rule.validate().ok()?;
        Some(rule)
    })
}
pub fn resolve(
    rule: &DirectoryRule,
    bases: &[(String, PathBuf, Option<String>)],
) -> Result<Vec<String>, String> {
    rule.validate()?;
    let mut found = Vec::new();
    for (kind, base, account) in bases {
        if kind != &rule.base || account != &rule.account {
            continue;
        }
        let candidate = base.join(&rule.relative);
        if candidate.exists() {
            let root = youji_save_archive::validate_root(&candidate)?;
            let value = root.to_string_lossy().into_owned();
            if !found.contains(&value) {
                found.push(value);
            }
        }
    }
    Ok(found)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn relocation_uses_relative_rule_and_never_guesses_between_candidates() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a");
        let b = dir.path().join("b");
        std::fs::create_dir_all(a.join("Game/save")).unwrap();
        std::fs::create_dir_all(b.join("Game/save")).unwrap();
        let rule = infer(
            &a.join("Game/save"),
            &[("documents".into(), a.clone(), None)],
        )
        .unwrap();
        assert_eq!(rule.relative, "Game/save");
        assert_eq!(
            resolve(&rule, &[("documents".into(), b.clone(), None)])
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            resolve(
                &rule,
                &[("documents".into(), a, None), ("documents".into(), b, None)]
            )
            .unwrap()
            .len(),
            2
        );
    }
}
