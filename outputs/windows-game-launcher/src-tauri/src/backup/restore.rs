use super::{archive, model::*, paths, schema};
use crate::db;
use rusqlite::{types::Value as SqlValue, Connection};
use serde_json::Value;
use std::{collections::HashMap, io::Write, path::Path};

fn sql_value(value: &Value) -> Result<SqlValue, String> {
    match value {
        Value::Null => Ok(SqlValue::Null), Value::String(text) => Ok(SqlValue::Text(text.clone())),
        Value::Number(number) => number.as_i64().map(SqlValue::Integer).ok_or_else(|| "备份整数超出范围".into()),
        _ => Err("备份记录包含无效类型".into()),
    }
}

pub fn replace(conn: &mut Connection, data: &Dataset, preferences: &Preferences, restore_id: &str) -> Result<(), String> {
    schema::validate(data, preferences)?;
    let proxy = db::setting(conn, "network_proxy")?;
    let tx = conn.transaction().map_err(|error| error.to_string())?;
    let result = (|| {
    // 标识和列名来自固定白名单，绝不执行备份中提供的 SQL 或数据库 schema。
    for table in schema::TABLES.iter().rev() { tx.execute(&format!("DELETE FROM {}", table.name), []).map_err(|error| error.to_string())?; }
    tx.execute("DELETE FROM settings", []).map_err(|error| error.to_string())?;
    for table in schema::TABLES {
        let placeholders = vec!["?"; table.columns.len()].join(",");
        let query = format!("INSERT INTO {} ({}) VALUES({})", table.name, table.columns.join(","), placeholders);
        let mut statement = tx.prepare(&query).map_err(|error| error.to_string())?;
        for row in &data.tables[table.name] {
            let values = row.iter().map(sql_value).collect::<Result<Vec<_>, _>>()?;
            statement.execute(rusqlite::params_from_iter(values)).map_err(|error| error.to_string())?;
        }
    }
    for (key, value) in &data.settings { db::set_setting(&tx, key, value)?; }
    for (key, value) in &preferences.behavior { db::set_setting(&tx, key, value)?; }
    // 目标机代理原地保留；所有账号凭证和运行恢复状态都不从源电脑携带。
    if !proxy.is_empty() { db::set_setting(&tx, "network_proxy", &proxy)?; }
    db::set_setting(&tx, "runtime_recovery", "[]")?;
    db::set_setting(&tx, "backup_restore_completed_id", restore_id)?;
    db::set_setting(&tx, "restore_pending_appearance", &serde_json::json!({"id":restore_id,"appearance":preferences.appearance}).to_string())?;
    Ok::<_, String>(())
    })();
    if let Err(error) = result {
        tx.rollback().map_err(|rollback| format!("{error}；回滚失败：{rollback}"))?;
        return Err(error);
    }
    tx.commit().map_err(|error| error.to_string())
}

fn install_covers(loaded: &mut archive::Loaded, destination: &Path) -> Result<Vec<std::path::PathBuf>, String> {
    std::fs::create_dir_all(destination).map_err(|error| error.to_string())?;
    let mut copied = HashMap::<String, String>::new(); let mut created = Vec::new();
    let result = (|| {
        for (key, text) in &mut loaded.data.settings {
            if !key.starts_with("cover_cache:") { continue; }
            let mut info: Value = serde_json::from_str(text).map_err(|_| "封面索引无效")?;
            let original = info["file"].as_str().ok_or("封面索引缺少文件名")?.to_owned();
            let name = if let Some(name) = copied.get(&original) { name.clone() } else {
                let extension = original.rsplit_once('.').ok_or("封面文件名无效")?.1;
                let name = format!("{}.{}", uuid::Uuid::new_v4(), extension);
                let path = destination.join(&name);
                let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&path).map_err(|error| error.to_string())?;
                created.push(path);
                let bytes = std::fs::read(loaded.directory.path().join("covers").join(&original)).map_err(|error| error.to_string())?;
                file.write_all(&bytes).map_err(|error| error.to_string())?; file.sync_all().map_err(|error| error.to_string())?;
                copied.insert(original, name.clone()); name
            };
            info["file"] = name.into(); *text = info.to_string();
        }
        Ok::<_, String>(())
    })();
    if let Err(error) = result {
        for path in created { let _ = std::fs::remove_file(path); }
        return Err(error);
    }
    Ok(created)
}

pub fn startup(conn: &mut Connection, directory: &Path, covers: &Path) {
    let pending_path = directory.join("pending-backup-restore.json");
    if !pending_path.is_file() { return; }
    let mut notice = RestoreNotice { id:uuid::Uuid::new_v4().to_string(), success:false,
        message:String::new(), safety_backup:String::new(), missing_paths:0, acknowledged:false };
    let mut installed = Vec::new(); let mut staged_archive = None;
    let result = (|| {
        let pending: Pending = serde_json::from_reader(std::fs::File::open(&pending_path).map_err(|error| error.to_string())?).map_err(|_| "待恢复任务无效")?;
        uuid::Uuid::parse_str(&pending.restore_id).map_err(|_| "恢复标识无效")?;
        if pending.archive != format!("restore-{}.youji-backup", pending.restore_id) { return Err("待恢复文件位置无效".into()); }
        notice.id = pending.restore_id.clone(); notice.safety_backup = pending.safety_backup;
        // 即使上次提交后进程中断或任务文件删除失败，也不能再次覆盖用户的新数据。
        if db::setting(conn, "backup_restore_completed_id")? == pending.restore_id {
            let path = directory.join("backup-staging").join(&pending.archive);
            if std::fs::symlink_metadata(&path).is_ok_and(|meta| meta.is_file() && !meta.file_type().is_symlink()) { staged_archive = Some(path); }
            return Ok(());
        }
        let path = directory.join("backup-staging").join(&pending.archive);
        if std::fs::symlink_metadata(&path).map_err(|error| error.to_string())?.file_type().is_symlink() { return Err("待恢复文件不能是链接".into()); }
        staged_archive = Some(path.clone());
        if archive::file_hash(&path)? != pending.sha256 { return Err("待恢复备份已损坏或改变".into()); }
        let mut loaded = archive::read(&path)?;
        let preview = paths::apply(&mut loaded.data, &pending.relocation)?;
        notice.missing_paths = preview.iter().filter(|item| item.status == "missing" || item.status == "inaccessible").count();
        installed = install_covers(&mut loaded, covers)?;
        replace(conn, &loaded.data, &loaded.preferences, &pending.restore_id)?;
        Ok::<_, String>(())
    })();
    match result {
        Ok(()) => { notice.success = true; notice.message = "数据已恢复，账号需要重新连接。缺失的游戏路径可继续在设置中定位。".into(); }
        Err(error) => {
            for path in installed { let _ = std::fs::remove_file(path); }
            notice.message = format!("恢复未完成，原数据已保留：{error}");
        }
    }
    // 这些都是本次恢复创建的工作文件；不删除用户选择的备份或原缓存。
    let _ = archive::atomic_json(&directory.join("backup-restore-notice.json"), &notice);
    let _ = std::fs::remove_file(pending_path);
    if let Some(path) = staged_archive { let _ = std::fs::remove_file(path); }
}

pub fn appearance_script(conn: &Connection) -> Result<Option<String>, String> {
    let text = db::setting(conn, "restore_pending_appearance")?;
    if text.is_empty() { return Ok(None); }
    let value: Value = serde_json::from_str(&text).map_err(|_| "恢复外观信息无效")?;
    let appearance: Appearance = serde_json::from_value(value["appearance"].clone()).map_err(|_| "恢复外观信息无效")?;
    appearance.validate()?;
    let generic = ["serif","sans-serif","monospace","system-ui","cursive","fantasy","ui-serif","ui-sans-serif","ui-monospace","ui-rounded","emoji","math","fangsong"];
    let css = appearance.fonts.iter().map(|name| {
        let lower = name.to_lowercase();
        if generic.contains(&lower.as_str()) { lower } else { serde_json::to_string(name).unwrap() }
    }).collect::<Vec<_>>().join(", ");
    let fonts = serde_json::json!({"families":appearance.fonts,"css":css}).to_string();
    Ok(Some(format!("window.__youjiApplyRestoredAppearance=()=>{{localStorage.setItem('launcher-theme',{});localStorage.setItem('launcher-theme-color',{});localStorage.setItem('launcher-font-settings',{});window.__youjiRestoredPreferencesId={};}};try{{window.__youjiApplyRestoredAppearance();}}catch{{}}",
        serde_json::to_string(&appearance.theme).unwrap(), serde_json::to_string(&appearance.theme_color).unwrap(), serde_json::to_string(&fonts).unwrap(), value["id"])))
}
