use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::Path;
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Binding {
    pub id: String, pub game_id: String, pub name: String, pub root: String,
    pub cloud_id: Option<String>, pub parent_id: Option<String>, pub revision: i64,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub id: String, pub profile_id: String, pub archive: String,
    pub manifest_hash: String, pub archive_hash: String, pub bytes: u64,
    pub parent_ids: Vec<String>, pub base_revision: i64, pub created_at: String,
    pub uploaded: bool, pub upload_started: bool, pub error: Option<String>,
}
pub fn open(dir: &Path) -> Result<Connection, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let c = Connection::open(dir.join("saves.sqlite")).map_err(|e| e.to_string())?;
    c.busy_timeout(std::time::Duration::from_secs(10)).map_err(|e| e.to_string())?;
    c.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;
        CREATE TABLE IF NOT EXISTS save_profiles(id TEXT PRIMARY KEY,game_id TEXT NOT NULL,data TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS save_snapshots(id TEXT PRIMARY KEY,profile_id TEXT NOT NULL,data TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS save_restore_jobs(id TEXT PRIMARY KEY,data TEXT NOT NULL);").map_err(|e| e.to_string())?;
    Ok(c)
}
pub fn binding(c: &Connection, id: &str) -> Result<Binding, String> {
    let data: String = c.query_row("SELECT data FROM save_profiles WHERE id=?1", [id], |r| r.get(0)).map_err(|_| "存档档案不存在")?;
    serde_json::from_str(&data).map_err(|e| e.to_string())
}
pub fn put_binding(c: &Connection, b: &Binding) -> Result<(), String> {
    c.execute("INSERT INTO save_profiles VALUES(?1,?2,?3) ON CONFLICT(id) DO UPDATE SET data=excluded.data",
        params![b.id,b.game_id,serde_json::to_string(b).map_err(|e| e.to_string())?]).map_err(|e| e.to_string())?;
    Ok(())
}
pub fn profiles(c: &Connection, game: &str) -> Result<Vec<Binding>, String> {
    let mut stmt = c.prepare("SELECT data FROM save_profiles WHERE game_id=?1 ORDER BY rowid").map_err(|e| e.to_string())?;
    let rows = stmt.query_map([game], |r| r.get::<_,String>(0)).map_err(|e| e.to_string())?;
    rows.map(|r| serde_json::from_str(&r.map_err(|e| e.to_string())?).map_err(|e| e.to_string())).collect()
}
pub fn put_snapshot(c: &Connection, s: &Snapshot) -> Result<(), String> {
    c.execute("INSERT INTO save_snapshots VALUES(?1,?2,?3) ON CONFLICT(id) DO UPDATE SET data=excluded.data",
        params![s.id,s.profile_id,serde_json::to_string(s).map_err(|e| e.to_string())?]).map_err(|e| e.to_string())?;
    Ok(())
}
pub fn snapshots(c: &Connection, profile: &str) -> Result<Vec<Snapshot>, String> {
    let mut stmt = c.prepare("SELECT data FROM save_snapshots WHERE profile_id=?1 ORDER BY rowid DESC").map_err(|e| e.to_string())?;
    let rows = stmt.query_map([profile], |r| r.get::<_,String>(0)).map_err(|e| e.to_string())?;
    rows.map(|r| serde_json::from_str(&r.map_err(|e| e.to_string())?).map_err(|e| e.to_string())).collect()
}
pub fn snapshot(c: &Connection, id: &str) -> Result<Snapshot, String> {
    let data: Option<String> = c.query_row("SELECT data FROM save_snapshots WHERE id=?1", [id], |r| r.get(0)).optional().map_err(|e| e.to_string())?;
    serde_json::from_str(&data.ok_or("本地快照不存在")?).map_err(|e| e.to_string())
}
