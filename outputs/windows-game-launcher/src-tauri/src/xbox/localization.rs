use crate::model::AchievementDefinition;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Translation {
    id: String,
    english_description: String,
    name: String,
    description: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Catalog {
    title_id: String,
    items: Vec<Translation>,
}

// 两平台条件逐项核对后的显示文本；Xbox 编号、图标和解锁来源保持原值。
pub(super) fn apply(namespace: &str, items: &mut [AchievementDefinition]) -> Result<bool, String> {
    if namespace != "xbox:64439fe5" {
        return Ok(false);
    }
    let catalog: Catalog = serde_json::from_str(include_str!("../../fixtures/xbox-well-zh.json"))
        .map_err(|_| "中文成就资料无效")?;
    if catalog.title_id != "64439fe5" {
        return Err("中文成就资料身份不一致".into());
    }
    let mut changed = false;
    for item in items {
        let Some(text) = catalog
            .items
            .iter()
            .find(|text| item.api_name == format!("{namespace}:{}", text.id))
        else {
            continue;
        };
        // 条件不一致时保留原文，避免将其他条件误配成中文。
        if item.description != text.english_description && item.description != text.description {
            continue;
        }
        if item.name != text.name || item.description != text.description {
            item.name = text.name.clone();
            item.description = text.description.clone();
            changed = true;
        }
    }
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_identity_and_rejects_changed_conditions() {
        let mut items = vec![AchievementDefinition {
            api_name: "xbox:64439fe5:4".into(),
            name: "Warming Up".into(),
            description: "Kill 100 enemies".into(),
            icon: "https://example.com/icon.png".into(),
            hidden: false,
        }];
        assert!(apply("xbox:64439fe5", &mut items).unwrap());
        assert_eq!(items[0].name, "热身");
        assert_eq!(items[0].api_name, "xbox:64439fe5:4");
        assert_eq!(items[0].icon, "https://example.com/icon.png");
        items[0].description = "Kill 200 enemies".into();
        assert!(!apply("xbox:64439fe5", &mut items).unwrap());
        assert!(!apply("xbox:480", &mut items).unwrap());
    }
}
