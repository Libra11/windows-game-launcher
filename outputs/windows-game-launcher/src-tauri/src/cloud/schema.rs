use rusqlite::Connection;

pub fn initialize(c: &Connection) -> Result<(), String> {
    let version: i64 = c.query_row("PRAGMA user_version", [], |r| r.get(0)).map_err(|e| e.to_string())?;
    if version > 1 { return Err("数据库版本高于当前客户端".into()); }
    c.execute_batch("SAVEPOINT cloud_schema;
      CREATE TABLE IF NOT EXISTS account_binding(singleton INTEGER PRIMARY KEY CHECK(singleton=1),user_id TEXT NOT NULL,device_id TEXT NOT NULL,library_instance_id TEXT NOT NULL);
      CREATE TABLE IF NOT EXISTS sync_game_links(local_id TEXT PRIMARY KEY,cloud_id TEXT NOT NULL UNIQUE);
      CREATE TABLE IF NOT EXISTS sync_state(singleton INTEGER PRIMARY KEY CHECK(singleton=1),cursor INTEGER NOT NULL DEFAULT 0,applying INTEGER NOT NULL DEFAULT 0,last_success TEXT NOT NULL DEFAULT '');
      INSERT OR IGNORE INTO sync_state(singleton) VALUES(1);
      CREATE TABLE IF NOT EXISTS sync_shadow(kind TEXT NOT NULL,id TEXT NOT NULL,revision INTEGER NOT NULL,resource TEXT NOT NULL,PRIMARY KEY(kind,id));
      CREATE TABLE IF NOT EXISTS sync_outbox(seq INTEGER PRIMARY KEY AUTOINCREMENT,operation_id TEXT NOT NULL UNIQUE,kind TEXT NOT NULL,id TEXT NOT NULL,payload TEXT NOT NULL,action TEXT NOT NULL DEFAULT 'set',base_revision INTEGER NOT NULL,dependency TEXT,status TEXT NOT NULL DEFAULT 'pending',result TEXT);
      CREATE INDEX IF NOT EXISTS sync_outbox_pending_resource ON sync_outbox(kind,id,seq DESC) WHERE status IN ('pending','conflict');
      CREATE TABLE IF NOT EXISTS sync_conflicts(operation_id TEXT PRIMARY KEY,result TEXT NOT NULL);
      CREATE TABLE IF NOT EXISTS cloud_archives(game_id TEXT PRIMARY KEY,archived INTEGER NOT NULL);
      CREATE TABLE IF NOT EXISTS achievement_facts(game_id TEXT NOT NULL,namespace TEXT NOT NULL,api_name TEXT NOT NULL,source TEXT NOT NULL,unlocked_at TEXT,present INTEGER NOT NULL,PRIMARY KEY(game_id,namespace,api_name,source));
      CREATE TABLE IF NOT EXISTS historical_playtime_baselines(game_id TEXT PRIMARY KEY,seconds INTEGER NOT NULL);
      CREATE TABLE IF NOT EXISTS cloud_session_owners(session_id TEXT PRIMARY KEY,device_id TEXT NOT NULL,checkpoint INTEGER NOT NULL DEFAULT 0);
      PRAGMA user_version=1;
      RELEASE cloud_schema;").map_err(|e| { let _=c.execute_batch("ROLLBACK TO cloud_schema; RELEASE cloud_schema;"); e.to_string() })?;
    super::triggers::initialize(c)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_resource_lookup_uses_index_and_keeps_latest_conflict() {
        let dir = tempfile::tempdir().unwrap();
        let c = crate::db::open(&dir.path().join("games.sqlite")).unwrap();
        c.execute_batch(
            "WITH RECURSIVE n(i) AS (VALUES(1) UNION ALL SELECT i+1 FROM n WHERE i<30000)
             INSERT INTO sync_outbox(operation_id,kind,id,payload,base_revision,status)
             SELECT 'history-'||i,'achievement_reference','game','{}',0,'done' FROM n;
             INSERT INTO sync_outbox(operation_id,kind,id,payload,base_revision,status) VALUES
             ('pending','achievement_reference','game','old',0,'pending'),
             ('conflict','achievement_reference','game','latest',0,'conflict'),
             ('done','achievement_reference','game','ignored',0,'done');"
        ).unwrap();
        let sql = "SELECT payload FROM sync_outbox WHERE kind='achievement_reference' AND id=?1
                   AND status IN ('pending','conflict') ORDER BY seq DESC LIMIT 1";
        assert_eq!(c.query_row(sql, ["game"], |r| r.get::<_, String>(0)).unwrap(), "latest");
        let plan: String = c.query_row(&format!("EXPLAIN QUERY PLAN {sql}"), ["game"], |r| r.get(3)).unwrap();
        assert!(plan.contains("SEARCH") && plan.contains("sync_outbox_pending_resource"), "{plan}");
        initialize(&c).unwrap();
    }
}
