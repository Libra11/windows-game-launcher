use crate::{model::Unlock, source};
use std::{
    collections::{BTreeMap, HashSet},
    io::Read,
    path::Path,
};

const MAX_RECORD_BYTES: u64 = 2 * 1024 * 1024;

pub(crate) fn read(
    file: &Path,
    definitions: &HashSet<String>,
    require_match: bool,
) -> Result<Vec<Unlock>, String> {
    let handle = std::fs::File::open(file).map_err(|e| format!("无法读取记录：{e}"))?;
    let mut bytes = Vec::new();
    handle
        .take(MAX_RECORD_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("无法读取记录：{e}"))?;
    if bytes.len() as u64 > MAX_RECORD_BYTES {
        return Err("无法读取记录：文件超过 2 MB 限制".into());
    }
    let text = std::str::from_utf8(&bytes)
        .map_err(|_| "记录格式无效：文件不是 UTF-8 文本")?
        .trim_start_matches('\u{feff}');
    let ini = file
        .extension()
        .is_some_and(|s| s.eq_ignore_ascii_case("ini"));
    if (ini || require_match) && definitions.is_empty() {
        return Err("缺少成就定义：请先更新资料，核对记录所属游戏".into());
    }
    let items = if ini {
        parse_ini(text, definitions)
    } else {
        source::parse_runtime(text)
    }
    .map_err(|e| format!("记录格式无效：{e}"))?;
    if require_match
        && items
            .iter()
            .any(|item| !definitions.contains(&item.api_name))
    {
        return Err("记录格式无效：解锁标识与当前游戏的成就定义不匹配".into());
    }
    Ok(items)
}

fn parse_ini(input: &str, definitions: &HashSet<String>) -> Result<Vec<Unlock>, String> {
    let mut sections = BTreeMap::<String, BTreeMap<String, String>>::new();
    let mut section = String::new();
    for line in input.lines().map(str::trim) {
        if line.is_empty() || line.starts_with([';', '#']) {
            continue;
        }
        if line.starts_with('[') {
            section = line
                .strip_prefix('[')
                .and_then(|s| s.strip_suffix(']'))
                .filter(|s| !s.is_empty())
                .ok_or("INI 分区无效")?
                .to_owned();
            if sections.insert(section.clone(), BTreeMap::new()).is_some() {
                return Err("INI 存在重复分区".into());
            }
            continue;
        }
        let (key, value) = line.split_once('=').ok_or("INI 字段无效")?;
        let fields = sections.get_mut(&section).ok_or("INI 字段缺少分区")?;
        if fields
            .insert(key.trim().to_ascii_lowercase(), value.trim().into())
            .is_some()
        {
            return Err("INI 存在重复字段".into());
        }
    }
    let mut recognized = false;
    let mut result = Vec::new();
    for (api_name, fields) in sections {
        // 索引分区只枚举标识，不能作为解锁依据。
        if api_name.eq_ignore_ascii_case("SteamAchievements") {
            recognized = true;
            continue;
        }
        let Some(achieved) = fields.get("achieved") else {
            continue;
        };
        recognized = true;
        if !definitions.contains(&api_name) {
            return Err("解锁标识与当前游戏的成就定义不匹配".into());
        }
        match achieved.as_str() {
            "0" => continue,
            "1" => (),
            _ => return Err("Achieved 必须为 0 或 1".into()),
        }
        let unlocked_at = match fields.get("unlocktime") {
            None => None,
            Some(time) => {
                let seconds: i64 = time.parse().map_err(|_| "UnlockTime 无效")?;
                if seconds == 0 {
                    None
                } else {
                    Some(
                        chrono::DateTime::from_timestamp(seconds, 0)
                            .filter(|_| seconds > 0)
                            .ok_or("UnlockTime 无效")?
                            .to_rfc3339(),
                    )
                }
            }
        };
        result.push(Unlock {
            api_name,
            unlocked_at,
        });
    }
    if !recognized {
        return Err("未找到受支持的 INI 解锁字段".into());
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn definitions() -> HashSet<String> {
        ["APPETIZER", "ANVIL"]
            .into_iter()
            .map(String::from)
            .collect()
    }

    #[test]
    fn reads_real_ini_unlock_and_ignores_index_and_progress() {
        let sample = include_str!("../fixtures/runtime-rune.ini");
        let items = parse_ini(sample, &definitions()).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].api_name, "APPETIZER");
        assert_eq!(
            items[0].unlocked_at.as_deref(),
            Some("2026-10-01T04:03:48+00:00")
        );
        let locked = "[ANVIL]\nAchieved=0\nCurProgress=100\nMaxProgress=100\n[SteamAchievements]\n00000=ANVIL\nCount=1";
        assert!(parse_ini(locked, &definitions()).unwrap().is_empty());
    }

    #[test]
    fn rejects_wrong_game_and_partial_or_invalid_writes() {
        assert!(parse_ini(
            include_str!("../fixtures/runtime-rune.ini"),
            &["OTHER_GAME".into()].into()
        )
        .is_err());
        assert!(parse_ini("[APPETIZER", &definitions()).is_err());
        assert!(parse_ini("[APPETIZER]\nAchieved=1\nAchieved=0", &definitions()).is_err());
        assert!(parse_ini("[APPETIZER]\nAchieved=1\nUnlockTime=bad", &definitions()).is_err());
        assert!(parse_ini("[APPETIZER]\nCurProgress=100", &definitions()).is_err());
    }

    #[test]
    fn reads_json_and_requires_definitions_for_alternate_identity() {
        let file = std::env::temp_dir().join(format!("{}.json", uuid::Uuid::new_v4()));
        std::fs::write(
            &file,
            r#"{"APPETIZER":{"earned":true},"WRONG_GAME":{"earned":true}}"#,
        )
        .unwrap();
        assert!(read(&file, &HashSet::new(), true)
            .unwrap_err()
            .contains("缺少成就定义"));
        assert!(read(&file, &definitions(), true).is_err());
        assert_eq!(read(&file, &HashSet::new(), false).unwrap().len(), 2);
        std::fs::remove_file(file).unwrap();
    }
}
