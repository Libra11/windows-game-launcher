use super::api::number;
use serde_json::Value;
use std::collections::HashSet;

pub(crate) struct Item {
    pub appid: String,
    pub title: String,
    pub owners: Vec<String>,
    pub shared: bool,
    pub played_seconds: Option<u64>,
    pub last_played: String,
}

pub(crate) struct Catalog {
    pub items: Vec<Item>,
    pub excluded: usize,
}

pub(crate) fn parse(data: &Value, steam_id: &str) -> Result<Catalog, String> {
    // 不能把缺失列表当成有效空库，否则会错误撤销已有共享状态。
    let rows = data.get("apps").and_then(Value::as_array)
        .ok_or("Steam 未返回完整的共享游戏列表，已保留原游戏库")?;
    let mut catalog = Catalog { items: Vec::new(), excluded: 0 };
    let mut seen = HashSet::new();
    for row in rows {
        let appid = row.get("appid").and_then(number)
            .filter(|id| *id > 0 && *id <= u32::MAX as u64)
            .ok_or("Steam 共享游戏编号无效，已停止导入")?.to_string();
        if !seen.insert(appid.clone()) { return Err("Steam 返回重复的共享游戏编号，已停止导入".into()); }
        let excluded = match row.get("exclude_reason") {
            Some(value) => number(value).ok_or("Steam 共享游戏授权状态无效")?,
            None => 0,
        };
        let owned = row.get("owner_steamids").and_then(Value::as_array).is_some_and(|owners| {
            owners.iter().any(|owner| number(owner).map(|id| id.to_string()).as_deref() == Some(steam_id))
        });
        // 被排除且非本人拥有的项目不需要补齐拥有者资料，不阻塞其他可共享游戏。
        if excluded != 0 && !owned { catalog.excluded += 1; continue; }
        let app_type = row.get("app_type").and_then(number).ok_or("Steam 共享游戏类型缺失")?;
        if app_type != 1 { catalog.excluded += 1; continue; }
        let owner_values = row.get("owner_steamids").and_then(Value::as_array)
            .ok_or("Steam 共享游戏拥有者资料缺失")?;
        let owners = owner_values.iter().map(|value| number(value).filter(|id| *id > 0)
            .map(|id| id.to_string()).ok_or("Steam 共享游戏拥有者身份无效"))
            .collect::<Result<Vec<_>, _>>()?;
        if owners.is_empty() { return Err("Steam 共享游戏没有有效拥有者".into()); }
        let shared = !owners.iter().any(|owner| owner == steam_id);
        let title = row.get("name").and_then(Value::as_str).filter(|name| !name.trim().is_empty())
            .map(str::to_owned).unwrap_or_else(|| format!("AppID {appid}"));
        // 此字段为请求账号的个人分钟数，不使用家庭使用摘要或拥有者汇总。
        let played_seconds = row.get("rt_playtime").and_then(number).and_then(|minutes| minutes.checked_mul(60));
        let last_played = row.get("rt_last_played").and_then(number)
            .and_then(|time| i64::try_from(time).ok()).filter(|time| *time > 0)
            .and_then(|time| chrono::DateTime::from_timestamp(time, 0)).map(|time| time.to_rfc3339()).unwrap_or_default();
        catalog.items.push(Item { appid, title, owners, shared, played_seconds, last_played });
    }
    Ok(catalog)
}
