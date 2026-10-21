use crate::{App,ApiError,Result,auth::Identity};
use axum::{extract::{State,Query,Path},Json};
use serde::Deserialize;
use serde_json::{Value,json};
use sqlx::{PgConnection,Row};
use uuid::Uuid;
use youji_cloud_protocol::*;

// 定义缓存可能很大；分页同时受条目数和字节数约束，确保客户端能够接收。
fn page_len<T:serde::Serialize>(items:&[T])->Result<usize>{
    let mut bytes=0usize;
    for (index,item) in items.iter().enumerate(){
        let size=serde_json::to_vec(item).map_err(|_|ApiError::bad("分页资源无效"))?.len();
        if index>0 && bytes+size>4*1024*1024{return Ok(index);}
        bytes+=size;
    }
    Ok(items.len())
}

pub async fn lock(conn:&mut PgConnection,user:Uuid)->Result<i64> {
    Ok(sqlx::query_scalar("SELECT sequence FROM user_sync_state WHERE user_id=$1 FOR UPDATE").bind(user).fetch_one(conn).await?)
}
pub async fn resource(conn:&mut PgConnection,user:Uuid,kind:ResourceType,id:&str)->Result<Option<Resource>> {
    let row=sqlx::query("SELECT revision,deleted,payload,device_id,updated_at FROM sync_resources WHERE user_id=$1 AND kind=$2 AND id=$3").bind(user).bind(kind.key()).bind(id).fetch_optional(conn).await?;
    Ok(row.map(|r|Resource{resource_type:kind,resource_id:id.into(),revision:r.get("revision"),deleted:r.get("deleted"),payload:r.get("payload"),device_id:r.get("device_id"),updated_at:r.get::<chrono::DateTime<chrono::Utc>,_>("updated_at").to_rfc3339()}))
}
pub async fn save(conn:&mut PgConnection,user:Uuid,resource:&Resource)->Result<i64> {
    let seq:i64=sqlx::query_scalar("UPDATE user_sync_state SET sequence=sequence+1 WHERE user_id=$1 RETURNING sequence").bind(user).fetch_one(&mut *conn).await?;
    sqlx::query("INSERT INTO sync_resources(user_id,kind,id,revision,deleted,payload,device_id,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(user_id,kind,id) DO UPDATE SET revision=excluded.revision,deleted=excluded.deleted,payload=excluded.payload,device_id=excluded.device_id,updated_at=excluded.updated_at")
        .bind(user).bind(resource.resource_type.key()).bind(&resource.resource_id).bind(resource.revision).bind(resource.deleted).bind(&resource.payload).bind(resource.device_id).bind(chrono::DateTime::parse_from_rfc3339(&resource.updated_at).map_err(|_|ApiError::bad("时间无效"))?).execute(&mut *conn).await?;
    sqlx::query("INSERT INTO sync_changes(user_id,sequence,resource) VALUES($1,$2,$3)").bind(user).bind(seq).bind(serde_json::to_value(resource).map_err(|_|ApiError::bad("资源无效"))?).execute(conn).await?;
    Ok(seq)
}

pub async fn apply(conn:&mut PgConnection,identity:Identity,op:&Operation,importing:bool)->Result<OperationResult> {
    let hash=digest(op).map_err(ApiError::bad)?;
    if let Some(row)=sqlx::query("SELECT digest,result FROM sync_operations WHERE user_id=$1 AND id=$2").bind(identity.user_id).bind(op.operation_id).fetch_optional(&mut *conn).await? {
        if row.get::<String,_>("digest")!=hash {return Err(ApiError::bad("操作 ID 已用于不同内容"));}
        return serde_json::from_value(row.get("result")).map_err(|_|ApiError::bad("操作结果无效"));
    }
    let mut effective=op.clone();
    let current=resource(conn,identity.user_id,op.resource_type,&op.resource_id).await?;
    let mut result=OperationResult{operation_id:op.operation_id,outcome:Outcome::Rejected,revision:current.as_ref().map_or(0,|c|c.revision),sequence:0,current:current.clone(),reason:String::new()};
    if let Some(dep)=op.depends_on_operation_id {
        let previous:Option<Value>=sqlx::query_scalar("SELECT result FROM sync_operations WHERE user_id=$1 AND id=$2").bind(identity.user_id).bind(dep).fetch_optional(&mut *conn).await?;
        let previous=previous.and_then(|v|serde_json::from_value::<OperationResult>(v).ok());
        match previous {
            Some(p) if matches!(p.outcome,Outcome::Applied|Outcome::Noop) && p.current.as_ref().is_some_and(|r|r.resource_type==op.resource_type&&r.resource_id==op.resource_id) => effective.base_revision=p.revision,
            _ => {result.outcome=Outcome::DependencyPending;result.reason="前置操作尚未成功".into();return Ok(result);}
        }
    }
    let decision=merge::decide(&effective,current.as_ref(),identity.device_id);
    match decision {
        Err(reason)=>result.reason=reason,
        Ok((outcome,payload))=>{
            result.outcome=outcome;
            if outcome==Outcome::Applied {
                if let Err(reason)=crate::constraints::check(conn,identity,&effective,&payload,importing).await? {
                    result.outcome=Outcome::Rejected;result.reason=reason;
                } else {
                    let record=Resource{resource_type:op.resource_type,resource_id:op.resource_id.clone(),revision:result.revision+1,deleted:op.action==Action::Delete,payload,device_id:identity.device_id,updated_at:chrono::Utc::now().to_rfc3339()};
                    result.sequence=save(conn,identity.user_id,&record).await?;
                    result.revision=record.revision;result.current=Some(record);
                    if op.action==Action::Delete {result.sequence=delete_members(conn,identity,op).await?.max(result.sequence);}
                }
            } else if outcome==Outcome::Conflict {result.reason="云端记录已变化，请选择需要保留的版本".into();}
        }
    }
    sqlx::query("INSERT INTO sync_operations(user_id,id,digest,result) VALUES($1,$2,$3,$4)").bind(identity.user_id).bind(op.operation_id).bind(hash).bind(serde_json::to_value(&result).map_err(|_|ApiError::bad("操作结果无效"))?).execute(conn).await?;
    Ok(result)
}

async fn delete_members(conn:&mut PgConnection,identity:Identity,op:&Operation)->Result<i64> {
    let kind=if op.resource_type==ResourceType::Tag {ResourceType::TagMember}else{ResourceType::CollectionMember};
    let ids:Vec<String>=sqlx::query_scalar("SELECT id FROM sync_resources WHERE user_id=$1 AND kind=$2 AND payload->>'categoryId'=$3 AND NOT deleted").bind(identity.user_id).bind(kind.key()).bind(&op.resource_id).fetch_all(&mut *conn).await?;
    let mut seq=0;
    for id in ids {
        if let Some(mut r)=resource(conn,identity.user_id,kind,&id).await? {
            r.deleted=true;r.revision+=1;r.device_id=identity.device_id;r.updated_at=chrono::Utc::now().to_rfc3339();seq=save(conn,identity.user_id,&r).await?;
        }
    }
    if op.resource_type==ResourceType::Collection {
        if let Some(mut r)=resource(conn,identity.user_id,ResourceType::CollectionOrder,"order").await? {
            let mut order:CollectionOrder=decode(&r.payload).map_err(ApiError::bad)?;
            order.ids.retain(|id|id.to_string()!=op.resource_id);r.payload=serde_json::to_value(order).unwrap();r.revision+=1;r.device_id=identity.device_id;r.updated_at=chrono::Utc::now().to_rfc3339();seq=save(conn,identity.user_id,&r).await?;
        }
    }
    Ok(seq)
}
pub async fn status(State(app):State<App>,identity:Identity)->Result<Json<Value>> {
    let row=sqlx::query("SELECT sequence,initialized FROM user_sync_state WHERE user_id=$1").bind(identity.user_id).fetch_one(&app.pool).await?;
    Ok(Json(json!({"protocolVersion":VERSION,"sequence":row.get::<i64,_>("sequence"),"initialized":row.get::<bool,_>("initialized")})))
}
pub async fn push(State(app):State<App>,identity:Identity,Json(input):Json<PushRequest>)->Result<Json<Vec<OperationResult>>> {
    if input.protocol_version!=VERSION || input.operations.len()>MAX_OPERATIONS {return Err(ApiError::bad("协议版本或批量大小不支持"));}
    let mut results=Vec::new();
    for op in input.operations {
        let mut tx=app.pool.begin().await?;lock(&mut tx,identity.user_id).await?;
        let initialized:bool=sqlx::query_scalar("SELECT initialized FROM user_sync_state WHERE user_id=$1").bind(identity.user_id).fetch_one(&mut *tx).await?;
        if !initialized {return Err(ApiError::conflict("请先完成导入预览与绑定"));}
        results.push(apply(&mut tx,identity,&op,false).await?);tx.commit().await?;
    }
    Ok(Json(results))
}
#[derive(Deserialize)]
#[serde(rename_all="camelCase")]
pub struct PageQuery {pub cursor:Option<i64>,pub high_watermark:Option<i64>,pub limit:Option<i64>,pub offset:Option<usize>}
pub async fn changes(State(app):State<App>,identity:Identity,Query(q):Query<PageQuery>)->Result<Json<ChangesPage>> {
    let current:i64=sqlx::query_scalar("SELECT sequence FROM user_sync_state WHERE user_id=$1").bind(identity.user_id).fetch_one(&app.pool).await?;
    let high=q.high_watermark.unwrap_or(current);let cursor=q.cursor.unwrap_or(0);let limit=q.limit.unwrap_or(100).clamp(1,500);
    if cursor<0 || high<cursor || high>current {return Err(ApiError::bad("同步游标无效"));}
    let rows=sqlx::query("SELECT sequence,resource FROM sync_changes WHERE user_id=$1 AND sequence>$2 AND sequence<=$3 ORDER BY sequence LIMIT $4").bind(identity.user_id).bind(cursor).bind(high).bind(limit).fetch_all(&app.pool).await?;
    let mut changes:Vec<Change>=rows.into_iter().map(|r|Ok(Change{sequence:r.get("sequence"),resource:serde_json::from_value(r.get("resource")).map_err(|_|ApiError::bad("变更记录无效"))?})).collect::<Result<_>>()?;
    changes.truncate(page_len(&changes)?);
    let next=changes.last().map_or(cursor,|r|r.sequence);
    Ok(Json(ChangesPage{changes,next_cursor:next,high_watermark:high,has_more:next<high}))
}
pub async fn snapshot_create(State(app):State<App>,identity:Identity)->Result<Json<Value>> {
    let mut tx=app.pool.begin().await?;let seq=lock(&mut tx,identity.user_id).await?;
    let rows=sqlx::query("SELECT kind,id FROM sync_resources WHERE user_id=$1 ORDER BY kind,id").bind(identity.user_id).fetch_all(&mut *tx).await?;
    let mut resources=Vec::new();
    for r in rows {let kind=serde_json::from_value(Value::String(r.get("kind"))).map_err(|_|ApiError::bad("资源类型无效"))?;if let Some(v)=resource(&mut tx,identity.user_id,kind,&r.get::<String,_>("id")).await? {resources.push(v);}}
    let id=Uuid::new_v4();
    sqlx::query("INSERT INTO sync_snapshots(user_id,id,sequence,resources,expires_at) VALUES($1,$2,$3,$4,now()+interval '30 minutes')").bind(identity.user_id).bind(id).bind(seq).bind(serde_json::to_value(resources).unwrap()).execute(&mut *tx).await?;
    tx.commit().await?;Ok(Json(json!({"snapshotId":id,"highWatermark":seq})))
}
pub async fn snapshot_page(State(app):State<App>,identity:Identity,Path(id):Path<Uuid>,Query(q):Query<PageQuery>)->Result<Json<SnapshotPage>> {
    let row=sqlx::query("SELECT sequence,resources FROM sync_snapshots WHERE user_id=$1 AND id=$2 AND expires_at>now()").bind(identity.user_id).bind(id).fetch_optional(&app.pool).await?.ok_or_else(||ApiError::bad("快照不存在或已过期"))?;
    let resources:Vec<Resource>=serde_json::from_value(row.get("resources")).map_err(|_|ApiError::bad("快照数据无效"))?;
    let offset=q.offset.unwrap_or(0);if offset>resources.len(){return Err(ApiError::bad("快照位置无效"));}
    let candidate_end=(offset+q.limit.unwrap_or(100).clamp(1,500) as usize).min(resources.len());
    let end=offset+page_len(&resources[offset..candidate_end])?;
    Ok(Json(SnapshotPage{snapshot_id:id,high_watermark:row.get("sequence"),resources:resources[offset..end].to_vec(),next_offset:end,has_more:end<resources.len()}))
}

#[cfg(test)]
mod paging_tests {
    #[test]
    fn large_resources_stop_at_byte_budget_without_skipping_first() {
        let items=vec!["x".repeat(2*1024*1024);3];
        assert_eq!(super::page_len(&items).unwrap(),1);
        assert_eq!(super::page_len::<String>(&[]).unwrap(),0);
        assert_eq!(super::page_len(&["x".repeat(5*1024*1024)]).unwrap(),1);
    }
}
