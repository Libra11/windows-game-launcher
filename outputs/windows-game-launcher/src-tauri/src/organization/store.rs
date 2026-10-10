use super::{Changes, Collection, Item, Organization};
use rusqlite::{params, Connection};
use std::collections::HashSet;

pub fn initialize(conn: &Connection) -> Result<(), String> {
    conn.execute_batch("CREATE TABLE IF NOT EXISTS library_tags(id TEXT PRIMARY KEY,name TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS library_collections(id TEXT PRIMARY KEY,name TEXT NOT NULL,position INTEGER NOT NULL);
        CREATE TABLE IF NOT EXISTS game_tags(game_id TEXT NOT NULL,tag_id TEXT NOT NULL,PRIMARY KEY(game_id,tag_id));
        CREATE TABLE IF NOT EXISTS game_collections(game_id TEXT NOT NULL,collection_id TEXT NOT NULL,PRIMARY KEY(game_id,collection_id));
        CREATE INDEX IF NOT EXISTS game_tags_by_tag ON game_tags(tag_id,game_id);
        CREATE INDEX IF NOT EXISTS game_collections_by_collection ON game_collections(collection_id,game_id);").map_err(|e|e.to_string())
}
pub fn name(value: &str) -> Result<String,String> {
    let value=value.trim();
    if value.is_empty() || value.chars().count()>40 || value.chars().any(char::is_control) {
        return Err("名称须为 1–40 个字符，不能包含控制字符".into());
    }
    Ok(value.into())
}
fn table(collection: bool) -> &'static str { if collection {"library_collections"} else {"library_tags"} }
fn membership(collection: bool) -> (&'static str,&'static str) {
    if collection {("game_collections","collection_id")} else {("game_tags","tag_id")}
}
fn query<T>(conn:&Connection,sql:&str, map:impl FnMut(&rusqlite::Row<'_>)->rusqlite::Result<T>)->Result<Vec<T>,String> {
    let mut stmt=conn.prepare(sql).map_err(|e|e.to_string())?;
    let rows=stmt.query_map([],map).map_err(|e|e.to_string())?;
    rows.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())
}
pub fn read(conn:&Connection)->Result<Organization,String> {
    Ok(Organization {
        tags:query(conn,"SELECT id,name FROM library_tags ORDER BY name",|r|Ok(Item{id:r.get(0)?,name:r.get(1)?}))?,
        collections:query(conn,"SELECT id,name,position FROM library_collections ORDER BY position,id",|r|Ok(Collection{id:r.get(0)?,name:r.get(1)?,position:r.get(2)?}))?,
        game_tags:query(conn,"SELECT game_id,tag_id FROM game_tags ORDER BY game_id,tag_id",|r|Ok((r.get(0)?,r.get(1)?)))?,
        game_collections:query(conn,"SELECT game_id,collection_id FROM game_collections ORDER BY game_id,collection_id",|r|Ok((r.get(0)?,r.get(1)?)))?,
    })
}
fn check_name(conn:&Connection,collection:bool,id:&str,value:&str)->Result<String,String> {
    let value=name(value)?;
    let rows=query(conn,&format!("SELECT id,name FROM {}",table(collection)),|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?)))?;
    if rows.iter().any(|(other,text)|other!=id && text.to_lowercase()==value.to_lowercase()) {return Err("已存在同名分类".into());}
    Ok(value)
}
pub fn create(conn:&mut Connection,collection:bool,value:&str)->Result<String,String> {
    let tx=conn.transaction().map_err(|e|e.to_string())?;
    let value=check_name(&tx,collection,"",value)?;let id=uuid::Uuid::new_v4().to_string();
    if collection {
        let last: i64=tx.query_row("SELECT COALESCE(MAX(position),-1) FROM library_collections",[],|row|row.get(0)).map_err(|e|e.to_string())?;
        let position=last.checked_add(1).ok_or("收藏夹顺序超出范围，请重新排序")?;
        tx.execute("INSERT INTO library_collections VALUES(?1,?2,?3)",params![id,value,position]).map_err(|e|e.to_string())?;
    } else {tx.execute("INSERT INTO library_tags VALUES(?1,?2)",params![id,value]).map_err(|e|e.to_string())?;}
    tx.commit().map_err(|e|e.to_string())?;Ok(id)
}
pub fn rename(conn:&mut Connection,collection:bool,id:&str,value:&str)->Result<(),String> {
    let tx=conn.transaction().map_err(|e|e.to_string())?;
    let value=check_name(&tx,collection,id,value)?;
    if tx.execute(&format!("UPDATE {} SET name=?1 WHERE id=?2",table(collection)),params![value,id]).map_err(|e|e.to_string())?==0 {return Err("分类已不存在".into());}
    tx.commit().map_err(|e|e.to_string())
}
pub fn remove(conn:&mut Connection,collection:bool,id:&str)->Result<(),String> {
    let tx=conn.transaction().map_err(|e|e.to_string())?;let (relation,column)=membership(collection);
    if tx.execute(&format!("DELETE FROM {} WHERE id=?1",table(collection)),[id]).map_err(|e|e.to_string())?==0 {return Err("分类已不存在".into());}
    tx.execute(&format!("DELETE FROM {relation} WHERE {column}=?1"),[id]).map_err(|e|e.to_string())?;
    tx.commit().map_err(|e|e.to_string())
}
pub fn reorder(conn:&mut Connection,ids:&[String])->Result<(),String> {
    let tx=conn.transaction().map_err(|e|e.to_string())?;
    let existing=query(&tx,"SELECT id FROM library_collections",|r|r.get::<_,String>(0))?;
    let wanted:HashSet<_>=ids.iter().collect();
    if wanted.len()!=ids.len() || existing.len()!=ids.len() || existing.iter().any(|id|!wanted.contains(id)) {return Err("收藏夹已变化，请重新排序".into());}
    for (position,id) in ids.iter().enumerate() {tx.execute("UPDATE library_collections SET position=?1 WHERE id=?2",params![position as i64,id]).map_err(|e|e.to_string())?;}
    tx.commit().map_err(|e|e.to_string())
}
fn require(conn:&Connection,table:&str,ids:&[String])->Result<(),String> {
    for id in ids {
        if !conn.query_row(&format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE id=?1)"),[id],|r|r.get::<_,bool>(0)).map_err(|e|e.to_string())? {return Err("游戏或分类已不存在，请刷新后重试".into());}
    }
    Ok(())
}
fn validate_changes(conn:&Connection,changes:&Changes)->Result<(),String> {
    if changes.game_ids.is_empty(){return Err("请先选择游戏".into());}
    require(conn,"games",&changes.game_ids)?;
    for (table,add,remove) in [("library_tags",&changes.add_tag_ids,&changes.remove_tag_ids),("library_collections",&changes.add_collection_ids,&changes.remove_collection_ids)] {
        require(conn,table,add)?;require(conn,table,remove)?;
        if add.iter().any(|id|remove.contains(id)){return Err("同一分类不能同时添加和移除".into());}
    }
    Ok(())
}
pub fn batch(conn:&mut Connection,changes:&Changes)->Result<(),String> {
    let tx=conn.transaction().map_err(|e|e.to_string())?;validate_changes(&tx,changes)?;
    for game in &changes.game_ids {
        for (relation,column,add,remove) in [("game_tags","tag_id",&changes.add_tag_ids,&changes.remove_tag_ids),("game_collections","collection_id",&changes.add_collection_ids,&changes.remove_collection_ids)] {
            for id in remove {tx.execute(&format!("DELETE FROM {relation} WHERE game_id=?1 AND {column}=?2"),params![game,id]).map_err(|e|e.to_string())?;}
            for id in add {tx.execute(&format!("INSERT OR IGNORE INTO {relation} VALUES(?1,?2)"),params![game,id]).map_err(|e|e.to_string())?;}
        }
    }
    tx.commit().map_err(|e|e.to_string())
}
pub fn set(conn:&mut Connection,game:&str,tags:Vec<String>,collections:Vec<String>)->Result<(),String> {
    let tx=conn.transaction().map_err(|e|e.to_string())?;
    let changes=Changes{game_ids:vec![game.into()],add_tag_ids:tags,add_collection_ids:collections,..Default::default()};validate_changes(&tx,&changes)?;
    for table in ["game_tags","game_collections"] {tx.execute(&format!("DELETE FROM {table} WHERE game_id=?1"),[game]).map_err(|e|e.to_string())?;}
    for (relation,ids) in [("game_tags",changes.add_tag_ids),("game_collections",changes.add_collection_ids)] {
        for id in ids {tx.execute(&format!("INSERT OR IGNORE INTO {relation} VALUES(?1,?2)"),params![game,id]).map_err(|e|e.to_string())?;}
    }
    tx.commit().map_err(|e|e.to_string())
}

// 调用者的游戏删除事务同时清理关联，不单独提交。
pub fn clear_game(conn:&Connection,id:&str)->Result<(),String> {
    for relation in ["game_tags","game_collections"] {conn.execute(&format!("DELETE FROM {relation} WHERE game_id=?1"),[id]).map_err(|e|e.to_string())?;}
    Ok(())
}
