use super::store;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use youji_cloud_protocol::save_policy::DirectoryRule;
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub auto_backup: bool,
    pub auto_upload: bool,
    pub directory_rule: Option<DirectoryRule>,
    pub baseline: Option<String>,
    pub last_success: Option<String>,
    pub notice: Option<String>,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub profile_id: String,
    pub kind: String,
    pub generation: i64,
    pub phase: String,
    pub attempts: i64,
    pub next_at: i64,
    pub error: Option<String>,
}
pub fn init(c: &Connection) -> Result<(), String> {
    c.execute_batch("CREATE TABLE IF NOT EXISTS save_automation(profile_id TEXT PRIMARY KEY,data TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS save_tasks(profile_id TEXT NOT NULL,kind TEXT NOT NULL,generation INTEGER NOT NULL DEFAULT 1,phase TEXT NOT NULL DEFAULT 'pending',attempts INTEGER NOT NULL DEFAULT 0,next_at INTEGER NOT NULL DEFAULT 0,error TEXT,PRIMARY KEY(profile_id,kind));
CREATE TABLE IF NOT EXISTS save_local_flags(snapshot_id TEXT PRIMARY KEY,pinned INTEGER NOT NULL DEFAULT 0,available INTEGER NOT NULL DEFAULT 1);
CREATE TABLE IF NOT EXISTS save_upload_handles(snapshot_id TEXT PRIMARY KEY,upload_id TEXT NOT NULL);").map_err(|e|e.to_string())
}
pub fn settings(c: &Connection, id: &str) -> Result<Settings, String> {
    let raw: Option<String> = c
        .query_row(
            "SELECT data FROM save_automation WHERE profile_id=?1",
            [id],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    raw.map(|s| serde_json::from_str(&s).map_err(|e| e.to_string()))
        .transpose()
        .map(|x| x.unwrap_or_default())
}
pub fn put(c: &Connection, id: &str, v: &Settings) -> Result<(), String> {
    c.execute("INSERT INTO save_automation VALUES(?1,?2) ON CONFLICT(profile_id) DO UPDATE SET data=excluded.data",params![id,serde_json::to_string(v).map_err(|e|e.to_string())?]).map_err(|e|e.to_string())?;
    Ok(())
}
pub fn enqueue(c: &Connection, id: &str, kind: &str) -> Result<(), String> {
    c.execute("INSERT INTO save_tasks(profile_id,kind) VALUES(?1,?2) ON CONFLICT(profile_id,kind) DO UPDATE SET generation=generation+1,phase=CASE WHEN phase='attention' THEN phase ELSE 'pending' END,next_at=0",params![id,kind]).map_err(|e|e.to_string())?;
    Ok(())
}
pub fn list(c: &Connection) -> Result<Vec<Task>, String> {
    let mut q=c.prepare("SELECT profile_id,kind,generation,phase,attempts,next_at,error FROM save_tasks ORDER BY next_at,kind").map_err(|e|e.to_string())?;
    let rows = q
        .query_map([], |r| {
            Ok(Task {
                profile_id: r.get(0)?,
                kind: r.get(1)?,
                generation: r.get(2)?,
                phase: r.get(3)?,
                attempts: r.get(4)?,
                next_at: r.get(5)?,
                error: r.get(6)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<_, _>>().map_err(|e| e.to_string())
}
pub fn finish(c: &Connection, t: &Task) -> Result<(), String> {
    c.execute(
        "DELETE FROM save_tasks WHERE profile_id=?1 AND kind=?2 AND generation=?3",
        params![t.profile_id, t.kind, t.generation],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
pub fn defer(
    c: &Connection,
    t: &Task,
    phase: &str,
    seconds: i64,
    error: Option<&str>,
) -> Result<(), String> {
    c.execute("UPDATE save_tasks SET phase=?3,next_at=?4,error=?5,attempts=attempts+CASE WHEN ?3='retrying' THEN 1 ELSE 0 END WHERE profile_id=?1 AND kind=?2 AND generation=?6",params![t.profile_id,t.kind,phase,chrono::Utc::now().timestamp()+seconds,error,t.generation]).map_err(|e|e.to_string())?;
    Ok(())
}
pub fn all_profiles(c: &Connection) -> Result<Vec<store::Binding>, String> {
    let mut q = c
        .prepare("SELECT data FROM save_profiles")
        .map_err(|e| e.to_string())?;
    let rows = q
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(|e| e.to_string())?;
    rows.map(|r| serde_json::from_str(&r.map_err(|e| e.to_string())?).map_err(|e| e.to_string()))
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn queue_survives_reopen_and_old_completion_cannot_drop_new_event() {
        let dir = tempfile::tempdir().unwrap();
        let c = store::open(dir.path()).unwrap();
        enqueue(&c, "p", "backup").unwrap();
        let t = list(&c).unwrap().remove(0);
        enqueue(&c, "p", "backup").unwrap();
        finish(&c, &t).unwrap();
        drop(c);
        assert_eq!(
            list(&store::open(dir.path()).unwrap()).unwrap()[0].generation,
            2
        );
    }
    #[test]
    fn existing_profiles_default_to_manual() {
        let dir = tempfile::tempdir().unwrap();
        let s = settings(&store::open(dir.path()).unwrap(), "old").unwrap();
        assert!(!s.auto_backup && !s.auto_upload);
    }
}
