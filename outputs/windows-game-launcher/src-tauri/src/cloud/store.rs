use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;
use youji_cloud_protocol::{Action, Operation, OperationResult, Outcome, Resource, ResourceType};

pub fn enqueue(c:&Connection,kind:ResourceType,id:&str,payload:Value)->Result<(),String>{
    if !c.query_row("SELECT EXISTS(SELECT 1 FROM account_binding)",[],|r|r.get::<_,bool>(0)).map_err(err)? {return Ok(());}
    let key=kind.key();
    let revision=c.query_row("SELECT revision FROM sync_shadow WHERE kind=?1 AND id=?2",params![key,id],|r|r.get::<_,i64>(0)).optional().map_err(err)?.unwrap_or(0);
    let dependency:Option<String>=c.query_row("SELECT operation_id FROM sync_outbox WHERE kind=?1 AND id=?2 AND status IN ('pending','conflict') ORDER BY seq DESC LIMIT 1",params![key,id],|r|r.get(0)).optional().map_err(err)?;
    c.execute("INSERT INTO sync_outbox(operation_id,kind,id,payload,base_revision,dependency) VALUES(?1,?2,?3,?4,?5,?6)",params![uuid::Uuid::new_v4().to_string(),key,id,payload.to_string(),revision,dependency]).map_err(err)?;
    Ok(())
}
pub fn pending(c:&Connection)->Result<Vec<Operation>,String>{
    let mut q=c.prepare("SELECT operation_id,kind,id,payload,action,base_revision,dependency FROM sync_outbox o WHERE status='pending' AND (dependency IS NULL OR EXISTS(SELECT 1 FROM sync_outbox p WHERE p.operation_id=o.dependency AND p.status='done')) ORDER BY seq LIMIT 100").map_err(err)?;
    let rows=q.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,String>(4)?,r.get::<_,i64>(5)?,r.get::<_,Option<String>>(6)?))).map_err(err)?;
    rows.map(|row|{let(a,b,id,p,d,mut rev,dep)=row.map_err(err)?;
        if let Some(ref prior)=dep {
            let result:String=c.query_row("SELECT result FROM sync_outbox WHERE operation_id=?1",[prior],|r|r.get(0)).map_err(err)?;
            rev=serde_json::from_str::<OperationResult>(&result).map_err(err)?.revision;
        }
        Ok(Operation{operation_id:a.parse().map_err(err)?,resource_type:serde_json::from_value(Value::String(b)).map_err(err)?,resource_id:id,payload:serde_json::from_str(&p).map_err(err)?,action:if d=="delete"{Action::Delete}else{Action::Set},base_revision:rev,depends_on_operation_id:dep.map(|s|s.parse()).transpose().map_err(err)?})
    }).collect()
}
pub fn apply(c:&Connection,r:&Resource)->Result<(),String>{
    let old=c.query_row("SELECT revision FROM sync_shadow WHERE kind=?1 AND id=?2",params![r.resource_type.key(),r.resource_id],|x|x.get::<_,i64>(0)).optional().map_err(err)?.unwrap_or(-1);
    if r.revision<old {
        let raw:String=c.query_row("SELECT resource FROM sync_shadow WHERE kind=?1 AND id=?2",params![r.resource_type.key(),r.resource_id],|x|x.get(0)).map_err(err)?;
        let latest:Resource=serde_json::from_str(&raw).map_err(err)?;
        return apply(c,&latest);
    }
    c.execute("INSERT INTO sync_shadow VALUES(?1,?2,?3,?4) ON CONFLICT(kind,id) DO UPDATE SET revision=excluded.revision,resource=excluded.resource",params![r.resource_type.key(),r.resource_id,r.revision,serde_json::to_string(r).map_err(err)?]).map_err(err)?;
    let pending:bool=c.query_row("SELECT EXISTS(SELECT 1 FROM sync_outbox WHERE kind=?1 AND id=?2 AND status IN ('pending','conflict'))",params![r.resource_type.key(),r.resource_id],|x|x.get(0)).map_err(err)?;
    if !pending {super::projection::apply(c,r)?;}
    Ok(())
}
pub fn result(c:&Connection,r:&OperationResult)->Result<(),String>{
    let status=match r.outcome {Outcome::Applied|Outcome::Noop=>"done",Outcome::DependencyPending=>return Ok(()),_=>"conflict"};
    c.execute("UPDATE sync_outbox SET status=?1,result=?2 WHERE operation_id=?3",params![status,serde_json::to_string(r).map_err(err)?,r.operation_id.to_string()]).map_err(err)?;
    if status=="conflict"{c.execute("INSERT OR REPLACE INTO sync_conflicts VALUES(?1,?2)",params![r.operation_id.to_string(),serde_json::to_string(r).map_err(err)?]).map_err(err)?;}
    if let Some(current)=&r.current{apply(c,current)?;} Ok(())
}
pub fn transaction<T>(c:&mut Connection,f:impl FnOnce(&Connection)->Result<T,String>)->Result<T,String>{
    let tx=c.transaction().map_err(err)?;
    tx.execute("UPDATE sync_state SET applying=1",[]).map_err(err)?;
    let result=f(&tx)?;
    tx.execute("UPDATE sync_state SET applying=0",[]).map_err(err)?;
    tx.commit().map_err(err)?;Ok(result)
}
pub fn err(e:impl std::fmt::Display)->String{e.to_string()}

pub fn resolve(tx:&Connection,operation_id:&str,keep_local:bool)->Result<(),String>{

        let(kind,id,_payload):(String,String,String)=tx.query_row("SELECT kind,id,payload FROM sync_outbox WHERE operation_id=?1 AND status='conflict'",[&operation_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).map_err(err)?;
        let(payload,action):(String,String)=tx.query_row("SELECT payload,action FROM sync_outbox WHERE kind=?1 AND id=?2 AND status IN ('pending','conflict') ORDER BY seq DESC LIMIT 1",params![kind,id],|r|Ok((r.get(0)?,r.get(1)?))).map_err(err)?;
        if keep_local && (kind=="official_snapshot"||kind=="official_playtime") {return Err("官方快照必须重新获取，请在游戏详情更新资料".into());}
        tx.execute("UPDATE sync_outbox SET status='superseded' WHERE kind=?1 AND id=?2 AND status IN ('pending','conflict')",params![kind,id]).map_err(err)?;
        tx.execute("DELETE FROM sync_conflicts WHERE operation_id IN (SELECT operation_id FROM sync_outbox WHERE kind=?1 AND id=?2)",params![kind,id]).map_err(err)?;
        let resource:Option<String>=tx.query_row("SELECT resource FROM sync_shadow WHERE kind=?1 AND id=?2",params![kind,id],|r|r.get(0)).optional().map_err(err)?;
        if keep_local{enqueue(tx,serde_json::from_value(Value::String(kind.clone())).map_err(err)?,&id,serde_json::from_str(&payload).map_err(err)?)?;
            tx.execute("UPDATE sync_outbox SET action=?1 WHERE seq=(SELECT MAX(seq) FROM sync_outbox WHERE kind=?2 AND id=?3)",params![action,kind,id]).map_err(err)?;
        }
        else if let Some(resource)=resource {super::projection::apply(tx,&serde_json::from_str(&resource).map_err(err)?)?;}
        Ok(())
}

pub fn snapshot(c:&Connection,mut resources:Vec<Resource>)->Result<(),String>{
    // 先建立完整事实集合，关系投影不能依赖服务器的字典分页顺序。
    for r in &resources{
        c.execute("INSERT INTO sync_shadow VALUES(?1,?2,?3,?4) ON CONFLICT(kind,id) DO UPDATE SET revision=excluded.revision,resource=excluded.resource WHERE excluded.revision>=sync_shadow.revision",params![r.resource_type.key(),r.resource_id,r.revision,serde_json::to_string(r).map_err(err)?]).map_err(err)?;
    }
    resources.sort_by_key(|r|match r.resource_type{ResourceType::Game=>0,ResourceType::PlatformAccount=>1,ResourceType::AchievementReference=>2,ResourceType::DefinitionCache=>3,ResourceType::CollectionOrder=>5,_=>4});
    for r in &resources{apply(c,r)?;}Ok(())
}
