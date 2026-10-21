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
      CREATE TABLE IF NOT EXISTS sync_conflicts(operation_id TEXT PRIMARY KEY,result TEXT NOT NULL);
      CREATE TABLE IF NOT EXISTS cloud_archives(game_id TEXT PRIMARY KEY,archived INTEGER NOT NULL);
      CREATE TABLE IF NOT EXISTS achievement_facts(game_id TEXT NOT NULL,namespace TEXT NOT NULL,api_name TEXT NOT NULL,source TEXT NOT NULL,unlocked_at TEXT,present INTEGER NOT NULL,PRIMARY KEY(game_id,namespace,api_name,source));
      CREATE TABLE IF NOT EXISTS historical_playtime_baselines(game_id TEXT PRIMARY KEY,seconds INTEGER NOT NULL);
      CREATE TABLE IF NOT EXISTS cloud_session_owners(session_id TEXT PRIMARY KEY,device_id TEXT NOT NULL,checkpoint INTEGER NOT NULL DEFAULT 0);
      PRAGMA user_version=1;
      RELEASE cloud_schema;").map_err(|e| { let _=c.execute_batch("ROLLBACK TO cloud_schema; RELEASE cloud_schema;"); e.to_string() })?;
    super::triggers::initialize(c)
}
