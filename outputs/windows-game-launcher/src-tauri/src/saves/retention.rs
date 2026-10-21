use super::{store, tasks};
use rusqlite::Connection;
use std::{
    collections::{HashMap, HashSet},
    path::Path,
};
use youji_cloud_protocol::save_policy::{retained, Version};
pub fn flags(c: &Connection) -> Result<HashMap<String, serde_json::Value>, String> {
    let mut q = c
        .prepare("SELECT snapshot_id,pinned,available FROM save_local_flags")
        .map_err(|e| e.to_string())?;
    let rows = q
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                serde_json::json!({"pinned":r.get::<_,bool>(1)?,"available":r.get::<_,bool>(2)?}),
            ))
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<_, _>>().map_err(|e| e.to_string())
}
pub fn prune(
    c: &Connection,
    dir: &Path,
    b: &store::Binding,
    settings: &tasks::Settings,
) -> Result<(), String> {
    if !settings.auto_backup && !settings.auto_upload {
        return Ok(());
    }
    let snapshots = store::snapshots(c, &b.id)?;
    let flags = flags(c)?;
    let referenced: HashSet<_> = snapshots.iter().flat_map(|s| s.parent_ids.iter()).collect();
    let pending: HashSet<_> = snapshots
        .iter()
        .filter(|s| settings.auto_upload && !s.uploaded)
        .flat_map(|s| s.parent_ids.iter())
        .collect();
    let versions: Vec<_> = snapshots
        .iter()
        .filter(|s| flags.get(&s.id).is_none_or(|f| f["available"] != false))
        .map(|s| Version {
            id: s.id.clone(),
            unix_micros: chrono::DateTime::parse_from_rfc3339(&s.created_at)
                .map(|v| v.timestamp_micros())
                .unwrap_or(0),
            protected: b.parent_id.as_ref() == Some(&s.id)
                || !referenced.contains(&s.id)
                || pending.contains(&s.id)
                || (settings.auto_upload && !s.uploaded)
                || flags.get(&s.id).is_some_and(|f| f["pinned"] == true),
        })
        .collect();
    let keep = retained(&versions, chrono::Utc::now().timestamp_micros());
    for s in snapshots {
        if keep.contains(&s.id) {
            continue;
        }
        let path = Path::new(&s.archive);
        if path.exists() {
            youji_save_archive::check_tree_path(path)?;
            if path.parent().and_then(|p| p.canonicalize().ok())
                != Some(dir.canonicalize().map_err(|e| e.to_string())?)
            {
                return Err("快照清理路径不在备份目录".into());
            }
            let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if name != format!("{}.zip", s.id) && name != format!("download-{}.zip", s.id) {
                return Err("快照文件名无效".into());
            }
            std::fs::remove_file(path).map_err(|e| e.to_string())?;
        }
        c.execute("INSERT INTO save_local_flags(snapshot_id,available) VALUES(?1,0) ON CONFLICT(snapshot_id) DO UPDATE SET available=0",[&s.id]).map_err(|e|e.to_string())?;
    }
    Ok(())
}
pub fn safety(c: &Connection, dir: &Path) -> Result<(), String> {
    let mut q = c
        .prepare("SELECT data FROM save_restore_jobs ORDER BY rowid DESC")
        .map_err(|e| e.to_string())?;
    let rows = q
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(|e| e.to_string())?;
    let mut completed = HashMap::<String, usize>::new();
    for raw in rows {
        let job: super::restore::Job =
            serde_json::from_str(&raw.map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        if !matches!(job.phase.as_str(), "complete" | "rolled_back") {
            continue;
        }
        let count = completed.entry(job.profile_id.clone()).or_default();
        *count += 1;
        if *count <= 3 {
            continue;
        }
        if job.safety.exists() {
            youji_save_archive::check_tree_path(&job.safety)?;
            if job.safety.parent() != Some(dir)
                || job.safety.file_name().and_then(|v| v.to_str())
                    != Some(&format!("safety-{}.zip", job.id))
            {
                return Err("安全备份路径不符".into());
            }
            std::fs::remove_file(&job.safety).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
