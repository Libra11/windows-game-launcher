use super::store;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::{fs, path::{Path, PathBuf}};
#[derive(Clone, Serialize, Deserialize)]
pub struct Job {
    pub id: String, pub profile_id: String, pub snapshot_id: String,
    pub root: PathBuf, pub staged: PathBuf, pub rollback: PathBuf,
    pub safety: PathBuf, pub expected_hash: String, pub phase: String,
}
fn persist(c: &Connection, job: &Job) -> Result<(), String> {
    c.execute("INSERT INTO save_restore_jobs VALUES(?1,?2) ON CONFLICT(id) DO UPDATE SET data=excluded.data",
        rusqlite::params![job.id,serde_json::to_string(job).map_err(|e| e.to_string())?]).map_err(|e| e.to_string())?;
    Ok(())
}
fn hash_directory(root: &Path, data: &Path) -> Result<String, String> {
    let archive = data.join(format!("check-{}.zip", uuid::Uuid::new_v4()));
    // 临时验证归档由 TempDir 生命周期回收，不操作用户目录。
    let temp = tempfile::tempdir_in(data).map_err(|e| e.to_string())?;
    let manifest = youji_save_archive::create(root, &temp.path().join(archive.file_name().unwrap()))?;
    youji_save_archive::manifest_hash(&manifest)
}
pub fn prepare(c: &Connection, data: &Path, b: &store::Binding, s: &store::Snapshot) -> Result<Job, String> {
    let root = youji_save_archive::validate_root(Path::new(&b.root))?;
    let parent = root.parent().ok_or("不能恢复磁盘根目录")?;
    let id = uuid::Uuid::new_v4().to_string();
    let staged = parent.join(format!(".youji-stage-{id}"));
    let rollback = parent.join(format!(".youji-rollback-{id}"));
    let manifest = youji_save_archive::extract(Path::new(&s.archive), &staged)?;
    if youji_save_archive::manifest_hash(&manifest)? != s.manifest_hash { return Err("快照清单校验失败".into()); }
    let safety = data.join(format!("safety-{id}.zip"));
    youji_save_archive::create(&root, &safety)?;
    let job = Job { id, profile_id:b.id.clone(), snapshot_id:s.id.clone(), root, staged, rollback,
        safety, expected_hash:s.manifest_hash.clone(), phase:"prepared".into() };
    persist(c, &job)?;
    Ok(job)
}
pub fn apply(c: &Connection, data: &Path, mut job: Job) -> Result<(), String> {
    // 每一步先记录意图，启动恢复同时检查实际目录，覆盖 rename 成功但日志未写入的窗口。
    job.phase = "moving_original".into(); persist(c, &job)?;
    fs::rename(&job.root, &job.rollback).map_err(|e| e.to_string())?;
    job.phase = "installing".into(); persist(c, &job)?;
    if let Err(error) = fs::rename(&job.staged, &job.root) {
        fs::rename(&job.rollback, &job.root).map_err(|rollback| format!("恢复失败：{error}；回滚失败：{rollback}"))?;
        job.phase = "rolled_back".into(); persist(c, &job)?;
        return Err(error.to_string());
    }
    finish(c, data, &mut job)
}
fn finish(c: &Connection, data: &Path, job: &mut Job) -> Result<(), String> {
    if hash_directory(&job.root, data)? != job.expected_hash { return Err("恢复后的文件校验失败，保留恢复日志和原目录".into()); }
    let tx = c.unchecked_transaction().map_err(|e| e.to_string())?;
    let mut b = store::binding(&tx, &job.profile_id)?;
    b.parent_id = Some(job.snapshot_id.clone());
    store::put_binding(&tx, &b)?;
    job.phase = "complete".into(); persist(&tx, job)?;
    tx.commit().map_err(|e| e.to_string())
}
pub fn recover(c: &Connection, data: &Path) -> Result<(), String> {
    let mut stmt = c.prepare("SELECT data FROM save_restore_jobs").map_err(|e| e.to_string())?;
    let rows = stmt.query_map([], |r| r.get::<_,String>(0)).map_err(|e| e.to_string())?;
    let jobs: Vec<Job> = rows.map(|r| serde_json::from_str(&r.map_err(|e| e.to_string())?).map_err(|e| e.to_string())).collect::<Result<_,String>>()?;
    drop(stmt);
    for mut job in jobs {
        if matches!(job.phase.as_str(), "complete" | "rolled_back") { continue; }
        for path in [&job.root, &job.staged, &job.rollback] {
            if path.exists() { youji_save_archive::check_tree_path(path)?; }
        }
        if job.root.exists() && job.rollback.exists() && !job.staged.exists() {
            if finish(c, data, &mut job).is_ok() { continue; }
            // 新目录校验失败时保留失败内容，再放回原目录。
            let failed = job.root.parent().ok_or("恢复路径无效")?.join(format!(".youji-failed-{}",job.id));
            fs::rename(&job.root, failed).map_err(|e| e.to_string())?;
            fs::rename(&job.rollback, &job.root).map_err(|e| e.to_string())?;
        } else if !job.root.exists() && job.rollback.exists() {
            fs::rename(&job.rollback, &job.root).map_err(|e| e.to_string())?;
        } else if !(job.root.exists() && !job.rollback.exists()) {
            return Err("存档恢复目录状态不明确，已阻止启动，请保留恢复日志和目录".into());
        }
        job.phase = "rolled_back".into(); persist(c, &job)?;
    }
    Ok(())
}
#[cfg(test)]
#[path = "restore_tests.rs"]
mod tests;
