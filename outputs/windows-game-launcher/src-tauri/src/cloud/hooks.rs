use rusqlite::{params,Connection,OptionalExtension};
use serde_json::json;
use youji_cloud_protocol::ResourceType as K;
use super::store::{enqueue,err};

pub fn atomic<T>(c:&Connection,f:impl FnOnce()->Result<T,String>)->Result<T,String>{
    c.execute_batch("SAVEPOINT cloud_write").map_err(err)?;
    match f(){Ok(v)=>{c.execute_batch("RELEASE cloud_write").map_err(err)?;Ok(v)},Err(e)=>{let _=c.execute_batch("ROLLBACK TO cloud_write; RELEASE cloud_write");Err(e)}}
}
fn ready(c:&Connection)->Result<bool,String>{
    let exists:bool=c.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='account_binding')",[],|r|r.get(0)).map_err(err)?;
    if !exists{return Ok(false);}
    c.query_row("SELECT EXISTS(SELECT 1 FROM account_binding) AND (SELECT applying=0 FROM sync_state)",[],|r|r.get(0)).map_err(err)
}
pub fn game(c:&Connection,g:&crate::model::Game)->Result<(),String>{
    if !ready(c)?{return Ok(());}
    if c.query_row("SELECT EXISTS(SELECT 1 FROM sync_game_links WHERE local_id=?1)",[&g.id],|r|r.get::<_,bool>(0)).map_err(err)?{return Ok(());}
    let meta:serde_json::Value=serde_json::from_str(&g.metadata_json).unwrap_or_default();
    let identity=match g.source.as_str(){"steam"=>g.appid.clone(),"local"=>g.id.clone(),"epic"=>format!("{}:{}",meta["epicNamespace"].as_str().ok_or("Epic 身份缺失")?,meta["epicAppName"].as_str().ok_or("Epic 身份缺失")?),_=>return Err("不支持的同步来源".into())};
    let id=uuid::Uuid::new_v4().to_string();
    let payload=json!({"source":g.source,"sourceIdentity":identity,"displayTitle":g.title,"metadataReference":g.appid,"achievementReference":crate::achievement_platform::namespace(c,g)?});
    youji_cloud_protocol::validate(K::Game,&id,&payload)?;
    c.execute("INSERT INTO sync_game_links VALUES(?1,?2)",params![g.id,id]).map_err(err)?;
    enqueue(c,K::Game,&id,payload)
}
pub fn unlock(c:&Connection,game:&str,api:&str,source:&str,time:&str,present:bool)->Result<(),String>{
    if !["manual","local","xbox-local"].contains(&source){return Ok(());}
    let exists:bool=c.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='achievement_facts')",[],|r|r.get(0)).map_err(err)?;
    if !exists{return Ok(());}
    let g=crate::db::game(c,game)?.ok_or("游戏不存在")?;
    let ns=crate::achievement_platform::namespace(c,&g)?;
    let time=if chrono::DateTime::parse_from_rfc3339(time).is_ok(){Some(time)}else{None};
    let previous:Option<(Option<String>,bool)>=c.query_row("SELECT unlocked_at,present FROM achievement_facts WHERE game_id=?1 AND namespace=?2 AND api_name=?3 AND source=?4",params![game,ns,api,source],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(err)?;
    if previous.as_ref().is_some_and(|(old,seen)|old.as_deref()==time && *seen==present){return Ok(());}
    c.execute("INSERT INTO achievement_facts VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(game_id,namespace,api_name,source) DO UPDATE SET unlocked_at=excluded.unlocked_at,present=excluded.present",params![game,ns,api,source,time,present]).map_err(err)?;
    if !ready(c)?{return Ok(());}
    let cloud:Option<String>=c.query_row("SELECT cloud_id FROM sync_game_links WHERE local_id=?1",[game],|r|r.get(0)).optional().map_err(err)?;
    if let Some(cloud)=cloud{
        let uuid=cloud.parse().map_err(err)?;let id=youji_cloud_protocol::unlock_id(uuid,&ns,api,source);
        enqueue(c,if source=="manual"{K::ManualUnlock}else{K::AutomaticUnlock},&id,json!({"gameId":cloud,"namespace":ns,"apiName":api,"source":source,"unlockedAt":time,"present":present}))?;
    } Ok(())
}
pub fn order(c:&Connection,ids:&[String])->Result<(),String>{
    if ready(c)? {enqueue(c,K::CollectionOrder,"order",json!({"ids":ids}))?;} Ok(())
}

pub fn namespace(c:&Connection,game:&str)->Result<Option<String>,String>{
    if !c.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='sync_game_links')",[],|r|r.get::<_,bool>(0)).map_err(err)?{return Ok(None);}
    let id:Option<String>=c.query_row("SELECT cloud_id FROM sync_game_links WHERE local_id=?1",[game],|r|r.get(0)).optional().map_err(err)?;
    let saved=crate::db::setting(c,&format!("cloud_achievement_namespace:{game}"))?;
    let Some(id)=id else{return Ok((!saved.is_empty()).then_some(saved));};
    let pending:Option<String>=c.query_row("SELECT payload FROM sync_outbox WHERE kind='achievement_reference' AND id=?1 AND status IN ('pending','conflict') ORDER BY seq DESC LIMIT 1",[&id],|r|r.get(0)).optional().map_err(err)?;
    if let Some(raw)=pending {let value:serde_json::Value=serde_json::from_str(&raw).map_err(err)?;return Ok(value["value"].as_str().map(str::to_owned));}
    let shadow:Option<String>=c.query_row("SELECT resource FROM sync_shadow WHERE id=?1 AND kind IN ('achievement_reference','game') ORDER BY kind='achievement_reference' DESC LIMIT 1",[&id],|r|r.get(0)).optional().map_err(err)?;
    Ok(shadow.and_then(|raw|serde_json::from_str::<youji_cloud_protocol::Resource>(&raw).ok()).and_then(|r|r.payload["value"].as_str().or_else(||r.payload["achievementReference"].as_str()).map(str::to_owned)).or_else(||(!saved.is_empty()).then_some(saved)))
}
pub fn reference(c:&Connection,g:&crate::model::Game)->Result<(),String>{
    if !ready(c)?{return Ok(());}
    let id:Option<String>=c.query_row("SELECT cloud_id FROM sync_game_links WHERE local_id=?1",[&g.id],|r|r.get(0)).optional().map_err(err)?;
    if let Some(id)=id {enqueue(c,K::AchievementReference,&id,json!({"value":crate::achievement_platform::namespace_local(c,g)?}))?;}
    Ok(())
}
pub fn event(c:&Connection,game:&str,ns:&str,api:&str,time:&str,date:&str)->Result<(),String>{
    if !ready(c)?{return Ok(());}
    let id:Option<String>=c.query_row("SELECT cloud_id FROM sync_game_links WHERE local_id=?1",[game],|r|r.get(0)).optional().map_err(err)?;
    if let Some(id)=id{let uuid=id.parse().map_err(err)?;enqueue(c,K::UnlockEvent,&youji_cloud_protocol::unlock_id(uuid,ns,api,"event"),json!({"gameId":id,"namespace":ns,"apiName":api,"recordedAt":time,"date":date}))?;}
    Ok(())
}
pub fn definitions(c:&Connection,game:&str)->Result<(),String>{
    if !ready(c)?{return Ok(());}
    let Some(g)=crate::db::game(c,game)? else{return Ok(());};
    let id:Option<String>=c.query_row("SELECT cloud_id FROM sync_game_links WHERE local_id=?1",[game],|r|r.get(0)).optional().map_err(err)?;
    let Some(id)=id else{return Ok(());};
    let ns=crate::achievement_platform::namespace(c,&g)?;
    let mut q=c.prepare("SELECT api_name,name,description,icon,hidden FROM achievements WHERE appid=?1 ORDER BY api_name").map_err(err)?;
    let rows=q.query_map([&ns],|r|{let icon:String=r.get(3)?;Ok(json!({"apiName":r.get::<_,String>(0)?,"name":r.get::<_,String>(1)?,"description":r.get::<_,String>(2)?,"icon":if icon.starts_with("https://"){icon}else{String::new()},"hidden":r.get::<_,bool>(4)?}))}).map_err(err)?.collect::<Result<Vec<_>,_>>().map_err(err)?;
    enqueue(c,K::DefinitionCache,&id,json!({"gameId":id,"namespace":ns,"language":"zh-CN","source":g.schema_source,"complete":g.schema_source=="Steam Web API"||g.schema_source=="Epic 官方成就","definitions":rows}))
}
pub fn collection_order(c:&Connection)->Result<(),String>{
    if !ready(c)?{return Ok(());}
    let mut q=c.prepare("SELECT id FROM library_collections ORDER BY position,id").map_err(err)?;
    let ids=q.query_map([],|r|r.get::<_,String>(0)).map_err(err)?.collect::<Result<Vec<_>,_>>().map_err(err)?;
    order(c,&ids)
}
