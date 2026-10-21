// SQLite 触发器与业务写入共享原子事务；投影期间关闭入队。
use rusqlite::Connection;
pub fn initialize(c: &Connection) -> Result<(),String> {
    for (table,kind,id,payload,condition) in [
        ("game_activity","favorite","NEW.game_id","json_object('value',json(CASE WHEN NEW.favorite THEN 'true' ELSE 'false' END))",""),
        ("games","title","NEW.id","json_object('value',NEW.title)",""),
        ("games","metadata_reference","NEW.id","json_object('value',NEW.appid)",""),
        ("cloud_archives","archive","NEW.game_id","json_object('value',json(CASE WHEN NEW.archived THEN 'true' ELSE 'false' END))",""),
        ("library_tags","tag","NEW.id","json_object('name',NEW.name)",""),
        ("library_collections","collection","NEW.id","json_object('name',NEW.name)",""),
    ] {
        for event in ["INSERT","UPDATE"] {
            let mapped=if ["tag","collection"].contains(&kind) {id.to_string()} else {format!("(SELECT cloud_id FROM sync_game_links WHERE local_id={id})")};
            let changed=if event=="UPDATE" {match kind {"favorite"=>" AND NEW.favorite<>OLD.favorite","title"=>" AND NEW.title<>OLD.title","metadata_reference"=>" AND NEW.appid<>OLD.appid","collection"|"tag"=>" AND NEW.name<>OLD.name",_=>""}} else {""};
            let sql=enqueue(kind,&mapped,payload,"set");
            c.execute_batch(&format!("CREATE TRIGGER IF NOT EXISTS cloud_{kind}_{event} AFTER {event} ON {table}
                WHEN (SELECT applying=0 FROM sync_state WHERE singleton=1) AND EXISTS(SELECT 1 FROM account_binding) AND {mapped} IS NOT NULL {changed} {condition}
                BEGIN {sql} END;")).map_err(|e|e.to_string())?;
        }
    }
    for (table,kind,column) in [("game_tags","tag_member","tag_id"),("game_collections","collection_member","collection_id")] {
        for (event,row,present) in [("INSERT","NEW","true"),("DELETE","OLD","false")] {
            let game=format!("(SELECT cloud_id FROM sync_game_links WHERE local_id={row}.game_id)");
            let id=format!("{game}||':'||{row}.{column}");
            let payload=format!("json_object('gameId',{game},'categoryId',{row}.{column},'present',json('{present}'))");
            let sql=enqueue(kind,&id,&payload,"set");
            let category=if kind=="tag_member"{"library_tags"}else{"library_collections"};
            c.execute_batch(&format!("CREATE TRIGGER IF NOT EXISTS cloud_{kind}_{event} AFTER {event} ON {table}
              WHEN (SELECT applying=0 FROM sync_state) AND EXISTS(SELECT 1 FROM account_binding) AND {game} IS NOT NULL AND EXISTS(SELECT 1 FROM {category} WHERE id={row}.{column}) BEGIN {sql} END;")).map_err(|e|e.to_string())?;
        }
    }
    for (table,kind) in [("library_tags","tag"),("library_collections","collection")] {
        let sql=enqueue(kind,"OLD.id","json_object('name',OLD.name)","delete");
        c.execute_batch(&format!("CREATE TRIGGER IF NOT EXISTS cloud_{kind}_delete AFTER DELETE ON {table}
          WHEN (SELECT applying=0 FROM sync_state) AND EXISTS(SELECT 1 FROM account_binding) BEGIN {sql} END;")).map_err(|e|e.to_string())?;
    }
    Ok(())
}
fn enqueue(kind:&str,id:&str,payload:&str,action:&str)->String {
    let uuid="lower(hex(randomblob(4)))||'-'||lower(hex(randomblob(2)))||'-4'||substr(lower(hex(randomblob(2))),2)||'-a'||substr(lower(hex(randomblob(2))),2)||'-'||lower(hex(randomblob(6)))";
    format!("INSERT INTO sync_outbox(operation_id,kind,id,payload,action,base_revision,dependency)
      SELECT {uuid},'{kind}',{id},{payload},'{action}',
      COALESCE((SELECT revision FROM sync_shadow WHERE kind='{kind}' AND id={id}),0),
      (SELECT operation_id FROM sync_outbox WHERE kind='{kind}' AND id={id} AND status IN ('pending','conflict') ORDER BY seq DESC LIMIT 1);")
}
