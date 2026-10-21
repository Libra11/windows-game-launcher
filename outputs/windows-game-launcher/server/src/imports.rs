use crate::{App,ApiError,Result,auth::Identity,sync};
use axum::{extract::{State,Path},Json};
use serde::{Deserialize,Serialize};
use serde_json::{Value,json};
use sqlx::Row;
use std::collections::{HashMap,HashSet};
use uuid::Uuid;
use youji_cloud_protocol::*;
#[path="import_resolution.rs"] mod resolution;
use resolution::{Decisions,Resolution,CategoryMapping};

#[derive(Clone,Serialize,Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct Seed {pub resource_type:ResourceType,pub resource_id:String,pub payload:Value}
#[derive(Clone,Serialize,Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct Mapping {pub local_id:String,pub cloud_id:Uuid}
#[derive(Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct Prepare {pub resources:Vec<Seed>,pub local_digest:String,#[serde(default)]pub mappings:Vec<Mapping>,#[serde(default)]pub resolutions:Vec<Resolution>}
#[derive(Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
struct Plan {operations:Vec<Operation>,mappings:Vec<Mapping>,warnings:Vec<String>,unresolved:usize,category_mappings:Vec<CategoryMapping>}

pub async fn prepare(State(app):State<App>,identity:Identity,Json(input):Json<Prepare>)->Result<Json<Value>> {
    if !saves::valid_hash(&input.local_digest) || input.resources.len()>10000 {return Err(ApiError::bad("导入摘要或记录数量无效"));}
    for seed in &input.resources{if serde_json::to_vec(seed).map_err(|_|ApiError::bad("资源编码失败"))?.len()>MAX_REQUEST_BYTES{return Err(ApiError::bad("单条导入资源超过 1 MiB，请减少该条定义缓存后重试"));}}
    let mut tx=app.pool.begin().await?;let sequence=sync::lock(&mut tx,identity.user_id).await?;
    let initialized:bool=sqlx::query_scalar("SELECT initialized FROM user_sync_state WHERE user_id=$1").bind(identity.user_id).fetch_one(&mut *tx).await?;
    let mut mappings=HashMap::new();let mut warnings=Vec::new();let mut operations=Vec::new();let mut created=HashSet::new();
    let explicit:HashMap<_,_>=input.mappings.into_iter().map(|m|(m.local_id,m.cloud_id)).collect();
    let mut source_ids=HashMap::new();
    let mut decisions=Decisions::new(input.resolutions)?;
    let rows=sqlx::query("SELECT kind,id,payload FROM sync_resources WHERE user_id=$1 AND kind IN ('tag','collection') AND NOT deleted").bind(identity.user_id).fetch_all(&mut *tx).await?;
    let mut categories:Vec<_>=rows.iter().map(|r|(if r.get::<String,_>("kind")=="tag"{ResourceType::Tag}else{ResourceType::Collection},r.get::<String,_>("id"),r.get::<Value,_>("payload"))).collect();
    for seed in input.resources.iter().filter(|s|s.resource_type==ResourceType::Game) {
        if mappings.contains_key(&seed.resource_id) {return Err(ApiError::bad("重复的本地游戏标识"));}
        let game:Game=decode(&seed.payload).map_err(ApiError::bad)?;
        let existing:Option<String>=sqlx::query_scalar("SELECT id FROM sync_resources WHERE user_id=$1 AND kind='game' AND payload->>'source'=$2 AND payload->>'sourceIdentity'=$3").bind(identity.user_id).bind(&game.source).bind(&game.source_identity).fetch_optional(&mut *tx).await?;
        let key=(game.source.clone(),game.source_identity.clone());
        let id=if let Some(id)=explicit.get(&seed.resource_id) {
            let target=sync::resource(&mut tx,identity.user_id,ResourceType::Game,&id.to_string()).await?.ok_or_else(||ApiError::bad("选择的云端游戏不存在"))?;
            let reference=sync::resource(&mut tx,identity.user_id,ResourceType::AchievementReference,&id.to_string()).await?.map(|r|r.payload["value"].clone()).unwrap_or_else(||target.payload["achievementReference"].clone());
            if game.source!="local"||target.payload["source"]!="local"||reference!=game.achievement_reference {return Err(ApiError::bad("只能关联成就命名空间一致的本地条目"));}
            if existing.as_ref().is_some_and(|e|e!=&id.to_string()) {return Err(ApiError::bad("本地身份已绑定到另一条目"));}
            *id
        } else if let Some(id)=existing {Uuid::parse_str(&id).map_err(|_|ApiError::bad("云端身份无效"))?}
        else if let Some(id)=source_ids.get(&key) {*id}
        else {Uuid::new_v4()};
        source_ids.insert(key,id);mappings.insert(seed.resource_id.clone(),id);
        validate(ResourceType::Game,&id.to_string(),&seed.payload).map_err(ApiError::bad)?;
        if sync::resource(&mut tx,identity.user_id,ResourceType::Game,&id.to_string()).await?.is_none() && created.insert(id) {
            operations.push(operation(ResourceType::Game,id.to_string(),seed.payload.clone(),0));
        }
    }
    let mut seeds:Vec<_>=input.resources.into_iter().filter(|s|s.resource_type!=ResourceType::Game).collect();
    seeds.sort_by_key(|s|match s.resource_type {ResourceType::PlatformAccount=>0,ResourceType::Tag|ResourceType::Collection=>1,ResourceType::AchievementReference=>2,ResourceType::CollectionOrder=>4,_=>3});
    let mut seen=HashMap::new();
    for mut seed in seeds {
        let decision_key=Decisions::key(&seed);
        if matches!(seed.resource_type,ResourceType::Tag|ResourceType::Collection)&&!decisions.category(&mut seed,&mut categories)?{continue;}
        decisions.remap_category(&mut seed);
        if seed.resource_type==ResourceType::CollectionOrder {
            let ids=seed.payload["ids"].as_array_mut().ok_or_else(||ApiError::bad("收藏夹顺序无效"))?;
            for (_,id,_) in categories.iter().filter(|(k,_,_)|*k==ResourceType::Collection){if !ids.iter().any(|v|v.as_str()==Some(id)){ids.push(json!(id));}}
        }
        remap(&mut seed,&mappings,identity)?;
        validate(seed.resource_type,&seed.resource_id,&seed.payload).map_err(ApiError::bad)?;
        if let Some(previous)=seen.insert((seed.resource_type,seed.resource_id.clone()),seed.payload.clone()) {
            if previous==seed.payload{continue;}
            return Err(ApiError::bad("多个本地条目映射到同一资源但意图不同，请先整理关联"));
        }
        if initialized && seed.resource_type==ResourceType::HistoricalBaseline {warnings.push("其他设备的无会话历史差额不重复导入，仅补入唯一会话".into());continue;}
        let mut current=sync::resource(&mut tx,identity.user_id,seed.resource_type,&seed.resource_id).await?;
        let mut synthetic=false;
        if current.is_none()&&matches!(seed.resource_type,ResourceType::Title|ResourceType::MetadataReference|ResourceType::AchievementReference){
            if let Some(mut game)=sync::resource(&mut tx,identity.user_id,ResourceType::Game,&seed.resource_id).await?{
                let field=match seed.resource_type{ResourceType::Title=>"displayTitle",ResourceType::MetadataReference=>"metadataReference",_=>"achievementReference"};
                game.payload=json!({"value":game.payload[field]});game.resource_type=seed.resource_type;game.revision=0;current=Some(game);synthetic=true;
            }
        }
        // 备份导入不是实时会话的接管入口。相同/较旧副本复用已有云端事实。
        if seed.resource_type==ResourceType::PlaySession {
            if let Some(c)=&current {
                let old:PlaySession=decode(&c.payload).map_err(ApiError::bad)?;
                let incoming:PlaySession=decode(&seed.payload).map_err(ApiError::bad)?;
                if old.game_id!=incoming.game_id || old.started_at!=incoming.started_at {
                    return Err(ApiError::conflict("历史会话身份冲突，请检查游戏关联或开始时间"));
                }
                if !old.historical {
                    if incoming.seconds>old.seconds || incoming.days.iter().any(|(day,seconds)|old.days.get(day).copied().unwrap_or(0)<*seconds) {
                        return Err(ApiError::conflict("备份会话包含云端实时会话尚未确认的数据，请在原设备同步"));
                    }
                    continue;
                }
            }
        }
        if let Some(c)=&current {
            if !matches!(seed.resource_type,ResourceType::PlaySession|ResourceType::AutomaticUnlock|ResourceType::UnlockEvent) {
                let conflicts_before=decisions.conflicts.len();
                if !decisions.keep_local(&decision_key,&seed,c){
                    // 初始字段尚无独立资源；采用云端或同值也必须生成投影与 revision。
                    if synthetic && decisions.conflicts.len()==conflicts_before{
                        operations.push(operation(seed.resource_type,seed.resource_id.clone(),c.payload.clone(),0));
                    }
                    // 保留云端顺序仍需容纳本次新建的收藏夹。
                    if seed.resource_type==ResourceType::CollectionOrder && decisions.conflicts.is_empty(){
                        let mut payload=c.payload.clone();
                        let ids=payload["ids"].as_array_mut().ok_or_else(||ApiError::bad("云端顺序无效"))?;
                        for (_,id,_) in categories.iter().filter(|(k,_,_)|*k==ResourceType::Collection){
                            if !ids.iter().any(|v|v.as_str()==Some(id)){ids.push(json!(id));}
                        }
                        if payload!=c.payload{operations.push(operation(seed.resource_type,seed.resource_id.clone(),payload,c.revision));}
                    }
                    continue;
                }
            }
        }
        let op=operation(seed.resource_type,seed.resource_id,seed.payload,current.as_ref().map_or(0,|c|c.revision));
        let (outcome,_)=merge::decide(&op,current.as_ref(),identity.device_id).map_err(ApiError::bad)?;
        if outcome==Outcome::Conflict {return Err(ApiError::conflict("历史记录身份冲突，请检查游戏关联或开始时间"));}
        if outcome!=Outcome::Noop {operations.push(op);}
    }
    if decisions.conflicts.is_empty(){
        sqlx::query("SAVEPOINT import_preview").execute(&mut *tx).await?;
        let mut skipped=HashSet::new();
        for op in &operations{
            let mut stable_id=op.resource_id.clone();
            for(local,cloud)in &mappings{stable_id=stable_id.replace(&cloud.to_string(),local);}
            let key=format!("validation:{}:{}",op.resource_type.key(),stable_id);
            if decisions.skip(&key){skipped.insert(op.operation_id);continue;}
            let result=sync::apply(&mut tx,identity,op,true).await?;
            if !matches!(result.outcome,Outcome::Applied|Outcome::Noop){
                decisions.conflicts.push(json!({"key":key,"kind":op.resource_type,"local":op.payload,"reason":result.reason,"choices":["cloud"]}));
                break;
            }
        }
        sqlx::query("ROLLBACK TO SAVEPOINT import_preview").execute(&mut *tx).await?;
        sqlx::query("RELEASE SAVEPOINT import_preview").execute(&mut *tx).await?;
        operations.retain(|op|!skipped.contains(&op.operation_id));
    }
    warnings.sort();warnings.dedup();
    let mappings:Vec<_>=mappings.into_iter().map(|(local_id,cloud_id)|Mapping{local_id,cloud_id}).collect();
    let preview=json!({"importId":Uuid::new_v4(),"sequence":sequence,"localDigest":input.local_digest,"mappings":mappings,"warnings":warnings,"resourceCount":operations.len(),"conflicts":decisions.conflicts,"unresolvedCount":decisions.conflicts.len(),"categoryMappings":decisions.mappings});
    let plan=Plan{operations,mappings,warnings,unresolved:decisions.conflicts.len(),category_mappings:decisions.mappings};
    sqlx::query("INSERT INTO import_jobs(user_id,id,device_id,sequence,digest,plan,expires_at) VALUES($1,$2,$3,$4,$5,$6,now()+interval '30 minutes')")
        .bind(identity.user_id).bind(preview["importId"].as_str().and_then(|s|Uuid::parse_str(s).ok()).unwrap()).bind(identity.device_id).bind(sequence).bind(input.local_digest).bind(serde_json::to_value(plan).unwrap()).execute(&mut *tx).await?;
    tx.commit().await?;Ok(Json(preview))
}
fn operation(resource_type:ResourceType,resource_id:String,payload:Value,base_revision:i64)->Operation {
    Operation{operation_id:Uuid::new_v4(),resource_type,resource_id,action:Action::Set,base_revision,depends_on_operation_id:None,payload}
}
fn remap(seed:&mut Seed,map:&HashMap<String,Uuid>,identity:Identity)->Result<()> {
    let resolve=|id:&str|map.get(id).copied().or_else(||Uuid::parse_str(id).ok()).ok_or_else(||ApiError::bad("记录缺少游戏身份映射"));
    if let Some(id)=seed.payload.get("gameId").and_then(Value::as_str).map(str::to_owned) {seed.payload["gameId"]=json!(resolve(&id)?);}
    match seed.resource_type {
        ResourceType::Title|ResourceType::MetadataReference|ResourceType::AchievementReference|ResourceType::Archive|ResourceType::Favorite =>seed.resource_id=resolve(&seed.resource_id)?.to_string(),
        ResourceType::HistoricalBaseline|ResourceType::OfficialSnapshot|ResourceType::DefinitionCache|ResourceType::OfficialPlaytime=>seed.resource_id=seed.payload["gameId"].as_str().ok_or_else(||ApiError::bad("游戏身份缺失"))?.into(),
        ResourceType::TagMember|ResourceType::CollectionMember=>seed.resource_id=format!("{}:{}",seed.payload["gameId"].as_str().unwrap_or(""),seed.payload["categoryId"].as_str().unwrap_or("")),
        ResourceType::AutomaticUnlock|ResourceType::ManualUnlock|ResourceType::UnlockEvent=>{
            let game=Uuid::parse_str(seed.payload["gameId"].as_str().unwrap_or("")).map_err(|_|ApiError::bad("成就游戏身份无效"))?;
            let source=if seed.resource_type==ResourceType::UnlockEvent {"event"} else {seed.payload["source"].as_str().unwrap_or("")};
            seed.resource_id=unlock_id(game,seed.payload["namespace"].as_str().unwrap_or(""),seed.payload["apiName"].as_str().unwrap_or(""),source);
        }
        ResourceType::PlaySession=>{seed.payload["historical"]=json!(true);seed.payload["ownerDeviceId"]=json!(identity.device_id);}
        _=>{}
    }
    Ok(())
}
#[derive(Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct Commit {pub local_digest:String}
pub async fn commit(State(app):State<App>,identity:Identity,Path(id):Path<Uuid>,Json(input):Json<Commit>)->Result<Json<Value>> {
    let mut tx=app.pool.begin().await?;let sequence=sync::lock(&mut tx,identity.user_id).await?;
    let row=sqlx::query("SELECT * FROM import_jobs WHERE user_id=$1 AND id=$2 AND device_id=$3 FOR UPDATE").bind(identity.user_id).bind(id).bind(identity.device_id).fetch_optional(&mut *tx).await?.ok_or_else(||ApiError::bad("导入预览不存在"))?;
    if row.get::<String,_>("digest")!=input.local_digest {return Err(ApiError::conflict("本地数据已变化，请重新预览"));}
    if let Some(result)=row.get::<Option<Value>,_>("result") {return Ok(Json(result));}
    if row.get::<i64,_>("sequence")!=sequence || row.get::<chrono::DateTime<chrono::Utc>,_>("expires_at")<=chrono::Utc::now() {return Err(ApiError::conflict("预览已过期或云端发生变化，请重新预览"));}
    let plan:Plan=serde_json::from_value(row.get("plan")).map_err(|_|ApiError::bad("导入计划无效"))?;
    if plan.unresolved>0{return Err(ApiError::conflict("请先解决全部导入冲突并重新预览"));}
    for op in plan.operations {
        let result=sync::apply(&mut tx,identity,&op,true).await?;
        if !matches!(result.outcome,Outcome::Applied|Outcome::Noop) {return Err(ApiError::conflict(format!("导入未提交：{}",result.reason)));}
    }
    let sequence:i64=sqlx::query_scalar("UPDATE user_sync_state SET initialized=true WHERE user_id=$1 RETURNING sequence").bind(identity.user_id).fetch_one(&mut *tx).await?;
    let result=json!({"sequence":sequence,"mappings":plan.mappings,"categoryMappings":plan.category_mappings});
    sqlx::query("UPDATE import_jobs SET result=$3 WHERE user_id=$1 AND id=$2").bind(identity.user_id).bind(id).bind(&result).execute(&mut *tx).await?;
    tx.commit().await?;Ok(Json(result))
}
