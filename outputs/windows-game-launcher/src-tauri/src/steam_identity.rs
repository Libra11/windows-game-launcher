use std::{collections::BTreeSet, path::Path};

#[derive(Default)]
pub(crate) struct Evidence {
    pub appid: String,
    pub components: bool,
    pub conflicting_ids: bool,
}
fn read(path: &Path) -> Option<String> {
    if path.metadata().ok()?.len() > 1024 * 1024 {
        return None;
    }
    Some(String::from_utf8_lossy(&std::fs::read(path).ok()?).into_owned())
}
fn appid(value: &str) -> Option<String> {
    let value = value.trim().trim_start_matches('\u{feff}');
    let id = value.parse::<u32>().ok()?;
    (id > 0).then(|| id.to_string())
}
fn ini_ids(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim().trim_start_matches('\u{feff}');
            if line.starts_with([';', '#']) {
                return None;
            }
            let (key, value) = line.split_once('=')?;
            if !key.trim().eq_ignore_ascii_case("AppId") {
                return None;
            }
            appid(value.split([';', '#']).next().unwrap_or(""))
        })
        .collect()
}
pub(crate) fn detect(exe: &Path) -> Evidence {
    let Some(folder) = exe.parent() else {
        return Evidence::default();
    };
    let mut roots = vec![folder.to_path_buf()];
    if let Some(stem) = exe.file_stem() {
        let plugins = folder.join(format!("{}_Data/Plugins", stem.to_string_lossy()));
        roots.extend([plugins.clone(), plugins.join("x86_64"), plugins.join("x86")]);
    }
    let mut ids = BTreeSet::new();
    let mut components = false;
    for root in roots {
        components |= ["steam_api.dll", "steam_api64.dll"]
            .iter()
            .any(|name| root.join(name).is_file());
        for relative in ["steam_appid.txt", "steam_settings/steam_appid.txt"] {
            if let Some(id) = read(&root.join(relative)).and_then(|text| appid(&text)) {
                ids.insert(id);
            }
        }
        if let Some(text) = read(&root.join("steam_emu.ini")) {
            let values = ini_ids(&text);
            components |= !values.is_empty();
            ids.extend(values);
        }
    }
    let conflicting_ids = ids.len() > 1;
    Evidence {
        appid: if ids.len() == 1 {
            ids.into_iter().next().unwrap()
        } else {
            String::new()
        },
        components,
        conflicting_ids,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_only_valid_appid_values() {
        assert_eq!(
            ini_ids(";AppId=1\n[Steam]\nAppId = 2456740 ; note\nSteamId=123\nAppId=invalid\n"),
            vec!["2456740"]
        );
        assert_eq!(appid("0"), None);
    }
    #[test]
    fn detects_unity_ini_dll_and_conflicting_configs() {
        let root = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
        let plugin = root.join("GRIME II_Data/Plugins/x86_64");
        std::fs::create_dir_all(&plugin).unwrap();
        std::fs::write(plugin.join("steam_api64.dll"), []).unwrap();
        let exe = root.join("GRIME II.exe");
        assert!(detect(&exe).components);
        assert!(detect(&exe).appid.is_empty());
        std::fs::write(plugin.join("steam_emu.ini"), "[Steam]\nAppId=2456740\n").unwrap();
        assert_eq!(detect(&exe).appid, "2456740");
        std::fs::write(root.join("steam_appid.txt"), "2529790").unwrap();
        let result = detect(&exe);
        assert!(result.conflicting_ids);
        assert!(result.appid.is_empty());
        std::fs::remove_dir_all(root).unwrap();
    }
}
