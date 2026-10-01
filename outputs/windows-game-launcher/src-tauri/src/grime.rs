use crate::model::Unlock;
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

pub const APPID: &str = "1123050";
const MAX_SAVE_SIZE: u64 = 16 * 1024 * 1024;

fn save_folder(appdata: Option<&Path>) -> Option<PathBuf> {
    Some(
        appdata?
            .parent()?
            .join("LocalLow/Clover Bite/GRIME/Save Files"),
    )
}

pub fn read_saves(
    appdata: Option<&Path>,
    definitions: &HashSet<String>,
) -> Result<Option<(Vec<Unlock>, PathBuf)>, String> {
    let Some(folder) = save_folder(appdata) else {
        return Ok(None);
    };
    if !folder.is_dir() {
        return Ok(None);
    }
    if definitions.is_empty() {
        return Err("缺少成就定义，请先同步 Steam 成就资料".into());
    }
    let mut files = std::fs::read_dir(&folder)
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("gd"))
        })
        .filter(|path| {
            path.file_name()
                .is_some_and(|name| name != "AGReservedGameSettings.gd")
        })
        .collect::<Vec<_>>();
    if files.is_empty() {
        return Ok(None);
    }
    files.sort();
    let mut names = HashSet::new();
    for file in &files {
        let size = std::fs::metadata(file).map_err(|e| e.to_string())?.len();
        if size > MAX_SAVE_SIZE {
            return Err(format!("存档过大：{}", file.display()));
        }
        let bytes = std::fs::read(file).map_err(|e| e.to_string())?;
        names.extend(parse_save(&bytes, definitions)?);
    }
    let mut unlocks = names
        .into_iter()
        .map(|api_name| Unlock {
            api_name,
            unlocked_at: None,
        })
        .collect::<Vec<_>>();
    unlocks.sort_by(|a, b| a.api_name.cmp(&b.api_name));
    let evidence = if files.len() == 1 {
        files.remove(0)
    } else {
        folder
    };
    Ok(Some((unlocks, evidence)))
}

fn parse_save(bytes: &[u8], definitions: &HashSet<String>) -> Result<Vec<String>, String> {
    if bytes.len() < 18
        || bytes[0] != 0
        || bytes[9..17] != [1, 0, 0, 0, 0, 0, 0, 0]
        || !bytes
            .windows(b"SyncHandler+GeneralData".len())
            .any(|part| part == b"SyncHandler+GeneralData")
        || !bytes
            .windows(b"unlockedAchievements".len())
            .any(|part| part == b"unlockedAchievements")
    {
        return Err("不是受支持的 GRIME 存档".into());
    }
    let matches = (17..bytes.len().saturating_sub(9))
        .filter(|&offset| bytes[offset] == 17)
        .filter_map(|offset| parse_achievement_array(&bytes[offset..], definitions))
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [only] => Ok(only.clone()),
        [] => Err("未定位到存档中的成就列表".into()),
        _ => Err("存档中存在多个可能的成就列表".into()),
    }
}

// GRIME 使用 .NET BinaryFormatter：17 是字符串数组，6 是字符串，13/14 是连续空值。
fn parse_achievement_array(bytes: &[u8], definitions: &HashSet<String>) -> Option<Vec<String>> {
    let object_id = i32::from_le_bytes(bytes.get(1..5)?.try_into().ok()?);
    let capacity = i32::from_le_bytes(bytes.get(5..9)?.try_into().ok()?);
    if object_id <= 0 || !(1..=1024).contains(&capacity) {
        return None;
    }
    let mut offset = 9;
    let mut count = 0;
    let mut names = Vec::new();
    while count < capacity as usize {
        match *bytes.get(offset)? {
            6 => {
                let string_id =
                    i32::from_le_bytes(bytes.get(offset + 1..offset + 5)?.try_into().ok()?);
                if string_id <= 0 {
                    return None;
                }
                offset += 5;
                let length = read_7bit_length(bytes, &mut offset)?;
                if length == 0 || length > 256 {
                    return None;
                }
                let name = std::str::from_utf8(bytes.get(offset..offset + length)?).ok()?;
                if !definitions.contains(name) {
                    return None;
                }
                names.push(name.to_owned());
                offset += length;
                count += 1;
            }
            10 => {
                offset += 1;
                count += 1;
            }
            13 => {
                let nulls = *bytes.get(offset + 1)? as usize;
                if nulls == 0 {
                    return None;
                }
                offset += 2;
                count = count.checked_add(nulls)?;
            }
            14 => {
                let nulls = i32::from_le_bytes(bytes.get(offset + 1..offset + 5)?.try_into().ok()?);
                if nulls <= 0 {
                    return None;
                }
                offset += 5;
                count = count.checked_add(nulls as usize)?;
            }
            _ => return None,
        }
        if count > capacity as usize {
            return None;
        }
    }
    (!names.is_empty()).then_some(names)
}

fn read_7bit_length(bytes: &[u8], offset: &mut usize) -> Option<usize> {
    let mut value = 0usize;
    for shift in (0..35).step_by(7) {
        let byte = *bytes.get(*offset)?;
        *offset += 1;
        value |= ((byte & 0x7f) as usize) << shift;
        if byte & 0x80 == 0 {
            return Some(value);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_achievement_array_and_rejects_invalid_save() {
        let definitions = HashSet::from(["ACH_ONE".to_owned()]);
        let mut bytes = vec![0; 17];
        bytes[9] = 1;
        bytes.extend_from_slice(b"SyncHandler+GeneralData unlockedAchievements");
        bytes.push(17);
        bytes.extend_from_slice(&39i32.to_le_bytes());
        bytes.extend_from_slice(&2i32.to_le_bytes());
        bytes.push(6);
        bytes.extend_from_slice(&40i32.to_le_bytes());
        bytes.push(7);
        bytes.extend_from_slice(b"ACH_ONE");
        bytes.extend_from_slice(&[10]);
        assert_eq!(parse_save(&bytes, &definitions).unwrap(), vec!["ACH_ONE"]);
        bytes[9] = 0;
        assert!(parse_save(&bytes, &definitions).is_err());
    }

    #[test]
    fn reads_real_save_when_supplied() {
        let Ok(path) = std::env::var("GRIME_SAVE_SAMPLE") else {
            return;
        };
        let bytes = std::fs::read(path).unwrap();
        let schema = std::env::var("GRIME_SCHEMA_SAMPLE").unwrap();
        let all = std::fs::read_to_string(schema).unwrap();
        let definitions = all.lines().map(str::to_owned).collect::<HashSet<_>>();
        assert!(definitions.len() >= 19);
        let names = parse_save(&bytes, &definitions).unwrap();
        assert_eq!(names.len(), 19);
    }
}
