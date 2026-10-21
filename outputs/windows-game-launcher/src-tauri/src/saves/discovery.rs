//! Bounded, read-only discovery. Rules suggest directories; only saves_bind persists a choice.
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant, UNIX_EPOCH},
};
#[derive(Deserialize)]
struct Catalog {
    games: Vec<Rule>,
}
#[derive(Deserialize)]
struct Rule {
    title: String,
    steam: u64,
    patterns: Vec<Pattern>,
}
#[derive(Deserialize)]
struct Pattern {
    base: String,
    relative: String,
    stores: Vec<String>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    path: String,
    game: String,
    evidence: String,
    files: u64,
    bytes: u64,
    modified: Option<u64>,
    incomplete: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultSet {
    candidates: Vec<Candidate>,
    matched: bool,
    limited: bool,
}
struct Budget {
    left: usize,
    deadline: Instant,
    limited: bool,
}
impl Budget {
    fn tick(&mut self) -> bool {
        if self.left == 0 || Instant::now() > self.deadline {
            self.limited = true;
            false
        } else {
            self.left -= 1;
            true
        }
    }
}
fn matches(pattern: &str, value: &str) -> bool {
    let p: Vec<_> = pattern.to_lowercase().chars().collect();
    let s: Vec<_> = value.to_lowercase().chars().collect();
    let (mut i, mut j, mut star, mut retry) = (0, 0, None, 0);
    while j < s.len() {
        if i < p.len() && (p[i] == '?' || p[i] == s[j]) {
            i += 1;
            j += 1;
        } else if i < p.len() && p[i] == '*' {
            star = Some(i);
            i += 1;
            retry = j;
        } else if let Some(k) = star {
            retry += 1;
            j = retry;
            i = k + 1;
        } else {
            return false;
        }
    }
    while i < p.len() && p[i] == '*' {
        i += 1;
    }
    i == p.len()
}
fn expand(base: &Path, relative: &str, budget: &mut Budget) -> Vec<PathBuf> {
    if relative
        .split('/')
        .any(|c| c.is_empty() || c == "." || c == ".." || c.contains(['\\', ':', '<', '>', '[']))
        || relative.contains("**")
    {
        return vec![];
    }
    let mut paths = vec![base.to_path_buf()];
    for part in relative.split('/') {
        let mut next = Vec::new();
        for path in paths {
            if !budget.tick() || youji_save_archive::check_tree_path(&path).is_err() {
                continue;
            }
            if part.contains(['*', '?']) || part == "{account}" {
                if let Ok(entries) = fs::read_dir(&path) {
                    for entry in entries.flatten() {
                        if !budget.tick() {
                            break;
                        }
                        let name = entry.file_name().to_string_lossy().into_owned();
                        let accepted = if part == "{account}" {
                            !name.is_empty()
                                && name.bytes().all(|b| b.is_ascii_digit())
                                && entry.path().is_dir()
                        } else {
                            matches(part, &name)
                        };
                        if accepted {
                            next.push(entry.path());
                        }
                    }
                }
            } else {
                let child = path.join(part);
                if child.exists() {
                    next.push(child);
                }
            }
        }
        paths = next;
    }
    paths
}
fn summarize(root: &Path, budget: &mut Budget) -> (u64, u64, Option<u64>, bool) {
    let (mut files, mut bytes, mut modified, mut incomplete) = (0, 0, None, false);
    let mut stack = vec![(root.to_path_buf(), 0)];
    while let Some((path, depth)) = stack.pop() {
        if !budget.tick() {
            incomplete = true;
            break;
        }
        if depth > 32 || youji_save_archive::check_tree_path(&path).is_err() {
            incomplete = true;
            continue;
        }
        let Ok(meta) = fs::symlink_metadata(&path) else {
            incomplete = true;
            continue;
        };
        if meta.is_file() {
            files += 1;
            bytes += meta.len();
            if let Ok(time) = meta
                .modified()
                .and_then(|t| t.duration_since(UNIX_EPOCH).map_err(std::io::Error::other))
            {
                modified = Some(modified.unwrap_or(0).max(time.as_secs()));
            }
        } else if meta.is_dir() {
            match fs::read_dir(path) {
                Ok(entries) => {
                    for entry in entries {
                        if !budget.tick() {
                            incomplete = true;
                            break;
                        }
                        match entry {
                            Ok(e) => stack.push((e.path(), depth + 1)),
                            Err(_) => incomplete = true,
                        }
                    }
                }
                Err(_) => incomplete = true,
            }
        }
    }
    (files, bytes, modified, incomplete)
}
fn discover(
    catalog: &Catalog,
    source: &str,
    appid: &str,
    title: &str,
    bases: &[(String, PathBuf, Option<String>)],
    excluded: &[PathBuf],
) -> ResultSet {
    let id = appid.parse::<u64>().unwrap_or(0);
    // Local imports also explicitly store the Steam AppID used to fetch game metadata.
    let by_id = matches!(source, "steam" | "local") && id > 0;
    let rules: Vec<_> = catalog
        .games
        .iter()
        .filter(|r| {
            if by_id {
                r.steam == id
            } else {
                r.title.trim().eq_ignore_ascii_case(title.trim())
            }
        })
        .collect();
    let mut result = ResultSet {
        candidates: vec![],
        matched: !rules.is_empty(),
        limited: false,
    };
    let mut seen = HashSet::new();
    let mut budget = Budget {
        left: 20000,
        deadline: Instant::now() + Duration::from_secs(3),
        limited: false,
    };
    for rule in rules {
        for pattern in &rule.patterns {
            if source != "local"
                && !pattern.stores.is_empty()
                && !pattern.stores.iter().any(|s| s == source)
            {
                continue;
            }
            for (kind, base, account) in bases {
                if kind != &pattern.base {
                    continue;
                }
                for hit in expand(base, &pattern.relative, &mut budget) {
                    if youji_save_archive::check_tree_path(&hit).is_err() {
                        continue;
                    }
                    let dir = if hit.is_dir() {
                        hit.as_path()
                    } else {
                        hit.parent().unwrap_or(base)
                    };
                    let Ok(root) = youji_save_archive::validate_root(dir) else {
                        continue;
                    };
                    let Ok(base) = base.canonicalize() else {
                        continue;
                    };
                    if root == base
                        || !root.starts_with(&base)
                        || excluded
                            .iter()
                            .any(|p| root.starts_with(p) || p.starts_with(&root))
                    {
                        continue;
                    }
                    if !seen.insert(root.to_string_lossy().to_lowercase()) {
                        continue;
                    }
                    let (files, bytes, modified, incomplete) = summarize(&root, &mut budget);
                    let mut evidence = if by_id {
                        "Steam 游戏 ID 与内置存档规则匹配".to_string()
                    } else {
                        "游戏名称与内置存档规则匹配，请核对是否为同一游戏".to_string()
                    };
                    if let Some(account) = account {
                        evidence.push_str(&format!("；Steam 账号目录 {account}"));
                    }
                    result.candidates.push(Candidate {
                        path: root.to_string_lossy().into(),
                        game: rule.title.clone(),
                        evidence,
                        files,
                        bytes,
                        modified,
                        incomplete,
                    });
                    if result.candidates.len() >= 64 {
                        budget.limited = true;
                        break;
                    }
                }
                if result.candidates.len() >= 64 {
                    break;
                }
            }
            if result.candidates.len() >= 64 {
                break;
            }
        }
    }
    result.limited = budget.limited;
    result.candidates.sort_by(|a, b| a.path.cmp(&b.path));
    result
}
#[tauri::command]
pub async fn saves_discover(app: tauri::AppHandle, game_id: String) -> Result<ResultSet, String> {
    tauri::async_runtime::spawn_blocking(move || {
        use tauri::Manager;
        let state = app.state::<crate::AppState>();
        let c = crate::lock_db(&state)?;
        let game = crate::db::game(&c, &game_id)?.ok_or("游戏不存在")?;
        drop(c);
        let mut bases = super::paths::bases(&app, &game_id)?;
        // Enumerate accounts as separate candidates; never choose the first Steam account.
        bases.retain(|(kind, _, _)| kind != "steamUserData");
        if let Ok(steam) = steamlocate::locate() {
            let userdata = steam.path().join("userdata");
            if youji_save_archive::check_tree_path(&userdata).is_ok() {
                if let Ok(entries) = fs::read_dir(userdata) {
                    for entry in entries.take(64).flatten() {
                        let account = entry.file_name().to_string_lossy().into_owned();
                        if !account.is_empty() && account.bytes().all(|b| b.is_ascii_digit()) {
                            bases.push(("steamUserData".into(), entry.path(), Some(account)));
                        }
                    }
                }
            }
        }
        let data = super::directory(&app)?;
        let c = super::store::open(&data)?;
        let mut excluded = vec![data.parent().unwrap().to_path_buf()];
        for profile in super::tasks::all_profiles(&c)? {
            excluded.push(PathBuf::from(profile.root));
        }
        let excluded: Vec<_> = excluded
            .into_iter()
            .filter_map(|p| p.canonicalize().ok())
            .collect();
        let catalog: Catalog = serde_json::from_str(include_str!("../../data/save-rules.json"))
            .map_err(|e| e.to_string())?;
        Ok(discover(
            &catalog,
            &game.source,
            &game.appid,
            &game.title,
            &bases,
            &excluded,
        ))
    })
    .await
    .map_err(|e| e.to_string())?
}
#[cfg(test)]
mod tests {
    use super::*;
    fn rules() -> Catalog {
        serde_json::from_value(serde_json::json!({"games":[{"title":"Test Game","steam":123,"patterns":[{"base":"documents","relative":"Test/*/*.sav","stores":[]}]}]})).unwrap()
    }
    #[test]
    fn exact_identity_multiple_candidates_and_no_writes() {
        let temp = tempfile::tempdir().unwrap();
        for account in ["a", "b"] {
            let p = temp.path().join("Test").join(account);
            fs::create_dir_all(&p).unwrap();
            fs::write(p.join("slot.sav"), b"progress").unwrap();
        }
        let bases = vec![("documents".into(), temp.path().into(), None)];
        let r = discover(&rules(), "steam", "123", "renamed", &bases, &[]);
        assert_eq!(r.candidates.len(), 2);
        assert_eq!(r.candidates[0].files, 1);
        assert_eq!(r.candidates[0].bytes, 8);
        assert_eq!(
            discover(&rules(), "local", "123", "中文名称", &bases, &[])
                .candidates
                .len(),
            2
        );
        assert!(!discover(&rules(), "steam", "456", "Test Game", &bases, &[]).matched);
        assert!(discover(&rules(), "local", "", "test game", &bases, &[]).matched);
        assert!(!discover(&rules(), "epic", "", "Test", &bases, &[]).matched);
        assert_eq!(
            discover(
                &rules(),
                "local",
                "",
                "Test Game",
                &bases,
                &[temp.path().join("Test/a").canonicalize().unwrap()]
            )
            .candidates
            .len(),
            1
        );
        assert_eq!(
            fs::read(temp.path().join("Test/a/slot.sav")).unwrap(),
            b"progress"
        );
    }
    #[test]
    fn unsafe_patterns_shared_root_and_limits() {
        let temp = tempfile::tempdir().unwrap();
        let mut b = Budget {
            left: 100,
            deadline: Instant::now() + Duration::from_secs(1),
            limited: false,
        };
        assert!(expand(temp.path(), "../*", &mut b).is_empty());
        assert!(expand(temp.path(), "C:/bad", &mut b).is_empty());
        fs::write(temp.path().join("slot.sav"), b"x").unwrap();
        let mut c = rules();
        c.games[0].patterns[0].relative = "*.sav".into();
        assert!(discover(
            &c,
            "steam",
            "123",
            "",
            &[("documents".into(), temp.path().into(), None)],
            &[]
        )
        .candidates
        .is_empty());
        b.left = 0;
        assert!(summarize(temp.path(), &mut b).3);
        assert!(b.limited);
    }
    #[test]
    fn account_placeholder_never_selects_config_files_or_non_account_folders() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir_all(temp.path().join("EldenRing/76561198000000000")).unwrap();
        fs::create_dir_all(temp.path().join("EldenRing/settings")).unwrap();
        fs::write(temp.path().join("EldenRing/GraphicsConfig.xml"), b"config").unwrap();
        let mut budget = Budget {
            left: 100,
            deadline: Instant::now() + Duration::from_secs(1),
            limited: false,
        };
        let found = expand(temp.path(), "EldenRing/{account}", &mut budget);
        assert_eq!(found, vec![temp.path().join("EldenRing/76561198000000000")]);
    }
    #[test]
    fn bundled_rules_have_known_games() {
        let c: Catalog = serde_json::from_str(include_str!("../../data/save-rules.json")).unwrap();
        assert!(c.games.iter().any(|g| g.steam == 1245620));
        assert!(c.games.iter().any(|g| g.steam == 1030300));
        assert!(matches("*.sav", "Slot.SAV"));
        assert!(!matches("*.sav", "Slot.txt"));
    }
}
