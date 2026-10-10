use super::model::*;
use std::collections::HashSet;

fn normalized(path: &str) -> String { path.replace('\\', "/").trim_end_matches('/').to_owned() }
fn absolute(path: &str) -> bool {
    let value = path.replace('\\', "/");
    let bytes = value.as_bytes();
    (bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && &bytes[1..3] == b":/")
        || (value.starts_with("//") && value[2..].split('/').filter(|part| !part.is_empty()).count() >= 2)
}
fn valid(path: &str) -> bool {
    absolute(path) && !path.chars().any(char::is_control) && !normalized(path).split('/').any(|part| part == ".." || part == ".")
}
pub fn mapped(path: &str, mappings: &[PathMapping]) -> String {
    let path = normalized(path);
    let parts: Vec<_> = path.split('/').collect();
    let best = mappings.iter().filter(|mapping| {
        let source = normalized(&mapping.from);
        let prefix: Vec<_> = source.split('/').collect();
        prefix.len() <= parts.len() && prefix.iter().zip(&parts).all(|(left, right)| left.to_lowercase() == right.to_lowercase())
    }).max_by_key(|mapping| normalized(&mapping.from).len());
    best.map_or(path.clone(), |mapping| {
        let count = normalized(&mapping.from).split('/').count();
        let rest = parts[count..].join("/");
        if rest.is_empty() { normalized(&mapping.to) } else { format!("{}/{}", normalized(&mapping.to), rest) }
    })
}
fn status(path: &str) -> String {
    if path.is_empty() { return "not_configured".into(); }
    match std::path::Path::new(path).try_exists() {
        Ok(true) if std::path::Path::new(path).is_file() => "exists".into(),
        Ok(_) => "missing".into(), Err(_) => "inaccessible".into(),
    }
}
pub fn apply(data: &mut Dataset, relocation: &Relocation) -> Result<Vec<PathPreview>, String> {
    if relocation.mappings.len() > 100 || relocation.overrides.len() > 100_000 { return Err("路径重定位项目过多".into()); }
    let mut roots = HashSet::new();
    for mapping in &relocation.mappings {
        if !valid(&mapping.from) || !valid(&mapping.to) || !roots.insert(normalized(&mapping.from).to_lowercase()) {
            return Err("目录映射必须为完整且不重复的 Windows 路径".into());
        }
    }
    let mut overrides = std::collections::HashMap::new();
    for item in &relocation.overrides {
        if !valid(&item.exe_path) || !item.exe_path.to_lowercase().ends_with(".exe")
            || item.custom_unlock_path.as_ref().is_some_and(|path| !path.is_empty() && !valid(path))
            || overrides.insert(item.game_id.as_str(), item).is_some()
        { return Err("单款游戏程序或记录路径无效".into()); }
    }
    let mut result = Vec::new();
    for row in data.tables.get_mut("games").ok_or("游戏列表缺失")? {
        let id = row[0].as_str().unwrap().to_owned();
        let original = row[4].as_str().unwrap().to_owned();
        let record = row[6].as_str().unwrap();
        let mut exe = mapped(&original, &relocation.mappings);
        let mut custom = mapped(record, &relocation.mappings);
        if let Some(item) = overrides.remove(id.as_str()) {
            exe = item.exe_path.clone();
            if let Some(path) = &item.custom_unlock_path { custom = path.clone(); }
        }
        result.push(PathPreview { game_id:id, title:row[3].as_str().unwrap().into(), source:row[1].as_str().unwrap().into(),
            original_path:original, status:status(&exe), record_status:status(&custom), exe_path:exe.clone(), custom_unlock_path:custom.clone() });
        row[4] = exe.into(); row[6] = custom.into();
    }
    if !overrides.is_empty() { return Err("路径修改引用了不存在的游戏".into()); }
    Ok(result)
}
