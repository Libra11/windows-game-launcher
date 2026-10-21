use rusqlite::{Connection,params};
use super::store::err;
use tauri::{Manager,Emitter};

pub fn set_archived(c:&Connection,game_id:&str,archived:bool,running:bool)->Result<(),String>{
    if running{return Err("游戏正在启动或运行，请退出游戏后再归档或恢复".into());}
    if crate::db::game(c,game_id)?.is_none(){return Err("游戏不存在".into());}
    c.execute("INSERT INTO cloud_archives VALUES(?1,?2) ON CONFLICT(game_id) DO UPDATE SET archived=excluded.archived",params![game_id,archived]).map_err(err)?;
    Ok(())
}
#[tauri::command]
pub fn cloud_set_archived(app:tauri::AppHandle,game_id:String,archived:bool)->Result<(),String>{
    let state=app.state::<crate::AppState>();
    // 与现有扫描/启动锁顺序一致；保留运行守卫所需的完整 games 查询。
    let _initialized=state.initialized.lock().map_err(|_|"扫描状态不可用")?;
    let runtime=state.runtime.lock().map_err(|_|"运行状态不可用")?;
    let running=matches!(runtime.info(&game_id).state.as_str(),"starting"|"running");
    let mut c=crate::lock_db(&state)?;
    let tx=c.transaction().map_err(err)?;set_archived(&tx,&game_id,archived,running)?;tx.commit().map_err(err)?;
    let _=app.emit("library-changed",&game_id);Ok(())
}
pub fn remap_categories(c:&Connection,mappings:&[serde_json::Value])->Result<(),String>{
    let applying:bool=c.query_row("SELECT applying FROM sync_state",[],|r|r.get(0)).map_err(err)?;
    c.execute("UPDATE sync_state SET applying=1",[]).map_err(err)?;
    for m in mappings{
        let local=m["localId"].as_str().ok_or("分类映射缺少本机 ID")?;
        let cloud=m["cloudId"].as_str().ok_or("分类映射缺少云端 ID")?;
        if local==cloud{continue;}
        let (table,relation,column)=match m["kind"].as_str(){
            Some("tag")=>("library_tags","game_tags","tag_id"),
            Some("collection")=>("library_collections","game_collections","collection_id"),
            _=>return Err("分类映射类型无效".into()),
        };
        // 删除旧分类后由完整云快照投影新 ID，避免名称唯一规则与旧 ID 冲突。
        c.execute(&format!("INSERT OR IGNORE INTO {relation}(game_id,{column}) SELECT game_id,?1 FROM {relation} WHERE {column}=?2"),params![cloud,local]).map_err(err)?;
        c.execute(&format!("DELETE FROM {relation} WHERE {column}=?1"),[local]).map_err(err)?;
        c.execute(&format!("DELETE FROM {table} WHERE id=?1"),[local]).map_err(err)?;
    }
    c.execute("UPDATE sync_state SET applying=?1",[applying]).map_err(err)?;Ok(())
}

// 仅接受当前认证用户的云端快照可见且命名空间一致的备份映射。
pub fn restore_mappings(local:&[serde_json::Value],cloud:&[serde_json::Value],saved:&[serde_json::Value],mut explicit:Vec<serde_json::Value>)->Vec<serde_json::Value>{
    for mapping in saved{
        if explicit.iter().any(|m|m["localId"]==mapping["localId"]){continue;}
        let left=local.iter().find(|g|g["id"]==mapping["localId"]&&g["source"]=="local");
        let right=cloud.iter().find(|g|g["id"]==mapping["cloudId"]&&g["source"]=="local");
        if let(Some(left),Some(right))=(left,right){if left["namespace"]==right["namespace"]{explicit.push(mapping.clone());}}
    }explicit
}
