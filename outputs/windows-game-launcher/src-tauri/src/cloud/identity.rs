use rusqlite::{Connection,params,OptionalExtension};
use serde_json::{json,Value};
use youji_cloud_protocol::{Resource,ResourceType as K};
use super::store::{self,err};

// 两台设备离线新建同一平台条目时，以服务端已接受的身份为准。
// 重建尚未完成的操作，旧 operationId 永不以不同内容重发。
pub fn reconcile(c:&Connection,local:&str,resource:&Resource)->Result<(),String>{
    let prior:Option<String>=c.query_row("SELECT cloud_id FROM sync_game_links WHERE local_id=?1",[local],|r|r.get(0)).optional().map_err(err)?;
    let Some(prior)=prior.filter(|id|id!=&resource.resource_id) else{return Ok(());};
    let mut q=c.prepare("SELECT operation_id,kind,id,payload,action FROM sync_outbox WHERE status IN ('pending','conflict') ORDER BY seq").map_err(err)?;
    let rows=q.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,String>(4)?))).map_err(err)?.collect::<Result<Vec<_>,_>>().map_err(err)?;
    c.execute("UPDATE sync_game_links SET cloud_id=?1 WHERE local_id=?2",params![resource.resource_id,local]).map_err(err)?;
    for(op,kind,id,raw,action)in rows{
        let mut payload:Value=serde_json::from_str(&raw).map_err(err)?;
        if id!=prior&&payload["gameId"].as_str()!=Some(&prior){continue;}
        let kind:K=serde_json::from_value(Value::String(kind)).map_err(err)?;
        c.execute("UPDATE sync_outbox SET status='superseded' WHERE operation_id=?1",[&op]).map_err(err)?;
        c.execute("DELETE FROM sync_conflicts WHERE operation_id=?1",[&op]).map_err(err)?;
        if kind==K::Game{
            for(k,field)in [(K::Title,"displayTitle"),(K::MetadataReference,"metadataReference"),(K::AchievementReference,"achievementReference")]{
                if payload[field]!=resource.payload[field]{store::enqueue(c,k,&resource.resource_id,json!({"value":payload[field]}))?;}
            }
            continue;
        }
        if payload.get("gameId").is_some(){payload["gameId"]=json!(resource.resource_id);}
        let new_id=match kind{
            K::TagMember|K::CollectionMember=>format!("{}:{}",resource.resource_id,payload["categoryId"].as_str().ok_or("分类身份无效")?),
            K::AutomaticUnlock|K::ManualUnlock|K::UnlockEvent=>youji_cloud_protocol::unlock_id(resource.resource_id.parse().map_err(err)?,payload["namespace"].as_str().unwrap_or(""),payload["apiName"].as_str().unwrap_or(""),if kind==K::UnlockEvent{"event"}else{payload["source"].as_str().unwrap_or("")}),
            K::PlaySession=>id,
            _=>if id==prior{resource.resource_id.clone()}else{id},
        };
        store::enqueue(c,kind,&new_id,payload)?;
        c.execute("UPDATE sync_outbox SET action=?1 WHERE seq=(SELECT MAX(seq) FROM sync_outbox)",[action]).map_err(err)?;
    }
    Ok(())
}
