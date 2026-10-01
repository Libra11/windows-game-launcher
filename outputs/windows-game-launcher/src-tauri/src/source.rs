use crate::model::{AchievementDefinition, Game, Unlock};
use serde_json::Value;
use std::path::{Path, PathBuf};

pub fn parse_runtime(input: &str) -> Result<Vec<Unlock>, String> {
    let value: Value = serde_json::from_str(input).map_err(|e| e.to_string())?;
    let object = value
        .as_object()
        .ok_or("解锁记录必须是以成就标识为键的 JSON 对象")?;
    let mut result = Vec::new();
    for (api_name, state) in object {
        let Some(fields) = state.as_object() else {
            continue;
        };
        let earned = ["earned", "Achieved", "achieved"]
            .iter()
            .find_map(|key| fields.get(*key))
            .is_some_and(|v| v == true || v == 1 || v == "1");
        if !earned {
            continue;
        }
        let seconds = ["earned_time", "UnlockTime", "unlock_time"]
            .iter()
            .find_map(|key| fields.get(*key))
            .and_then(|v| v.as_i64().or_else(|| v.as_str()?.parse().ok()));
        let unlocked_at = seconds
            .and_then(|s| chrono::DateTime::from_timestamp(s, 0))
            .map(|date| date.to_rfc3339());
        result.push(Unlock {
            api_name: api_name.clone(),
            unlocked_at,
        });
    }
    Ok(result)
}

pub fn parse_local_schema(
    input: &str,
    schema_file: &Path,
) -> Result<Vec<AchievementDefinition>, String> {
    let value: Value = serde_json::from_str(input).map_err(|e| e.to_string())?;
    let array = value.as_array().ok_or("本地成就定义必须是 JSON 数组")?;
    Ok(array
        .iter()
        .filter_map(|item| {
            let api_name = item.get("name")?.as_str()?.to_owned();
            let icon = item.get("icon").and_then(Value::as_str).unwrap_or("");
            Some(AchievementDefinition {
                name: item
                    .get("displayName")
                    .and_then(Value::as_str)
                    .unwrap_or(&api_name)
                    .to_owned(),
                api_name,
                description: item
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned(),
                icon: if icon.is_empty() {
                    String::new()
                } else {
                    schema_file
                        .parent()?
                        .join(icon)
                        .to_string_lossy()
                        .into_owned()
                },
                hidden: item
                    .get("hidden")
                    .is_some_and(|v| v == true || v == 1 || v == "1"),
            })
        })
        .collect())
}

pub fn local_schema_path(game: &Game) -> Option<PathBuf> {
    let folder = Path::new(&game.exe_path).parent()?;
    let mut candidates = vec![folder.join("steam_settings").join("achievements.json")];
    if game.appid == "1123050" {
        candidates.push(folder.join("GRIME_Data/Plugins/x86_64/steam_settings/achievements.json"));
    }
    candidates.into_iter().find(|path| path.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reads_goldberg_runtime_and_ignores_locked() {
        let sample = include_str!("../fixtures/runtime-initial.json");
        let result = parse_runtime(sample).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].api_name, "ACH_FIRST_WIN");
        assert!(result[0].unlocked_at.is_some());
    }
    #[test]
    fn reads_local_schema_separately_from_runtime() {
        let items = parse_local_schema(
            include_str!("../fixtures/definitions.json"),
            Path::new("C:/Game/steam_settings/achievements.json"),
        )
        .unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].name, "初次胜利");
    }
    #[test]
    fn does_not_treat_definition_array_as_unlock_file() {
        assert!(parse_runtime(r#"[{"name":"ACH_ONE"}]"#).is_err());
    }
}
