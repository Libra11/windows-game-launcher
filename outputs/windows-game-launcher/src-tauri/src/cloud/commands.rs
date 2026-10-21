use serde_json::{json,Value};
use tauri::Manager;
use rusqlite::{params,OptionalExtension};
use super::{Cloud,request,store::{self,err}};

#[tauri::command]
pub async fn cloud_status(app:tauri::AppHandle)->Result<Value,String>{
    let cloud=app.state::<Cloud>();
    let logged=cloud.session.lock().await.is_some();
    let error=cloud.last_error.lock().await.clone();
    let state=app.state::<crate::AppState>();let c=crate::lock_db(&state)?;
    let binding:Option<(String,String)>=c.query_row("SELECT user_id,device_id FROM account_binding",[],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(err)?;
    let count=|status:&str|c.query_row("SELECT COUNT(*) FROM sync_outbox WHERE status=?1",[status],|r|r.get::<_,i64>(0)).map_err(err);
    let mut q=c.prepare("SELECT o.operation_id,o.kind,o.id,o.payload,f.result,o.action FROM sync_conflicts f JOIN sync_outbox o USING(operation_id) ORDER BY o.seq").map_err(err)?;
    let conflicts=q.query_map([],|r|Ok(json!({"operationId":r.get::<_,String>(0)?,"kind":r.get::<_,String>(1)?,"resourceId":r.get::<_,String>(2)?,"local":r.get::<_,String>(3)?,"result":r.get::<_,String>(4)?,"action":r.get::<_,String>(5)?}))).map_err(err)?.collect::<Result<Vec<_>,_>>().map_err(err)?;
    let mut games_query=c.prepare("SELECT g.id,g.title,COALESCE(a.archived,0) FROM games g LEFT JOIN cloud_archives a ON a.game_id=g.id ORDER BY g.title").map_err(err)?;
    let games=games_query.query_map([],|r|Ok(json!({"id":r.get::<_,String>(0)?,"title":r.get::<_,String>(1)?,"archived":r.get::<_,bool>(2)?}))).map_err(err)?.collect::<Result<Vec<_>,_>>().map_err(err)?;
    let phase=if cloud.sync_gate.try_lock().is_err(){"syncing"}else if error.is_some(){"error"}else if !logged{"login_required"}else if binding.is_none(){"unbound"}else if !conflicts.is_empty(){"conflict"}else if count("pending")?>0{"pending"}else{"synced"};
    Ok(json!({"phase":phase,"games":games,"baseUrl":crate::db::setting(&c,"cloud_base_url")?,"boundUserId":binding.as_ref().map(|b|&b.0),"deviceId":binding.map(|b|b.1),"loggedIn":logged,"pending":count("pending")?,"conflicts":conflicts,"error":error,"lastSuccess":c.query_row("SELECT last_success FROM sync_state",[],|r|r.get::<_,String>(0)).map_err(err)?}))
}
#[tauri::command]
pub async fn cloud_challenge(app:tauri::AppHandle,base_url:String,email:String)->Result<Value,String>{
    let base=base_url.trim().trim_end_matches('/');super::http::validate_base(base)?;
    let logged=app.state::<Cloud>().session.lock().await.is_some();
    {let state=app.state::<crate::AppState>();let c=crate::lock_db(&state)?;
        let previous=crate::db::setting(&c,"cloud_base_url")?;
        let bound=c.query_row("SELECT EXISTS(SELECT 1 FROM account_binding)",[],|r|r.get::<_,bool>(0)).map_err(err)?;
        if previous!=base&&(bound||logged){return Err("已绑定或已登录资料库不能更换云服务".into());}
        crate::db::set_setting(&c,"cloud_base_url",base)?;
    }
    super::http::public(&app,"/v1/auth/challenges",json!({"email":email.trim().to_lowercase()})).await
}
#[tauri::command]
pub async fn cloud_verify(app:tauri::AppHandle,challenge_id:String,code:String,device_name:String)->Result<(),String>{
    let cloud=app.state::<Cloud>();let _gate=cloud.http_gate.lock().await;
    let device={let state=app.state::<crate::AppState>();let c=crate::lock_db(&state)?;
        c.query_row("SELECT device_id FROM account_binding",[],|r|r.get::<_,String>(0)).optional().map_err(err)?};
    let response=super::http::public(&app,"/v1/auth/verify",json!({"challengeId":challenge_id,"code":code,"deviceName":device_name,"deviceId":device})).await?;
    super::http::install(&app,serde_json::from_value(response).map_err(err)?).await
}
#[tauri::command]
pub async fn cloud_logout(app:tauri::AppHandle)->Result<(),String>{
    request(&app,"POST","/v1/auth/logout",Some(json!({}))).await?;
    let base=super::http::base(&app)?;
    let user={let state=app.state::<crate::AppState>();let c=crate::lock_db(&state)?;crate::db::setting(&c,"cloud_login_user")?};
    *app.state::<Cloud>().session.lock().await=None;
    {let state=app.state::<crate::AppState>();let c=crate::lock_db(&state)?;
    crate::db::set_setting(&c,"cloud_login_user","")?;}
    super::credentials::delete(&base,&user)
}
#[tauri::command]
pub async fn cloud_sync(app:tauri::AppHandle)->Result<(),String>{
    let result=super::worker::synchronize(&app).await;
    *app.state::<Cloud>().last_error.lock().await=result.as_ref().err().cloned();result
}
#[tauri::command]
pub async fn cloud_devices(app:tauri::AppHandle)->Result<Value,String>{request(&app,"GET","/v1/devices",None).await}
#[tauri::command]
pub async fn cloud_revoke_device(app:tauri::AppHandle,device_id:String)->Result<Value,String>{
    uuid::Uuid::parse_str(&device_id).map_err(err)?;
    request(&app,"DELETE",&format!("/v1/devices/{device_id}"),None).await
}
#[tauri::command]
pub async fn cloud_prepare_import(app:tauri::AppHandle,mappings:Option<Vec<Value>>,resolutions:Option<Vec<Value>>)->Result<Value,String>{
    let cloud=app.state::<Cloud>();let _gate=cloud.sync_gate.lock().await;
    let session=cloud.session.lock().await.as_ref().map(|s|(s.user_id.to_string(),s.device_id.to_string())).ok_or("请先登录")?;
    let resources={let state=app.state::<crate::AppState>();let c=crate::lock_db(&state)?;super::export::resources(&c,&session.1)?};
    let digest=youji_cloud_protocol::digest(&resources)?;
    for resource in &resources{if serde_json::to_vec(resource).map_err(err)?.len()>1024*1024{return Err("单条导入资源超过 1 MiB，未上传任何资料；请减少该条定义缓存后重试".into());}}
    let local_games:Vec<Value>=resources.iter().filter(|r|r["resourceType"]=="game").map(|r|json!({"id":r["resourceId"],"title":r["payload"]["displayTitle"],"source":r["payload"]["source"],"namespace":r["payload"]["achievementReference"]})).collect();
    let snap=request(&app,"POST","/v1/sync/snapshots",Some(json!({}))).await?;
    let snapshot_id=snap["snapshotId"].as_str().ok_or("云端快照缺少 ID")?;
    let mut offset=0;let mut cloud_games=Vec::new();let mut references=std::collections::HashMap::new();
    loop{
        let page:youji_cloud_protocol::SnapshotPage=serde_json::from_value(request(&app,"GET",&format!("/v1/sync/snapshots/{snapshot_id}?offset={offset}&limit=100"),None).await?).map_err(err)?;
        for r in page.resources{
            if r.resource_type==youji_cloud_protocol::ResourceType::AchievementReference&&!r.deleted{references.insert(r.resource_id.clone(),r.payload["value"].clone());}
            if r.resource_type==youji_cloud_protocol::ResourceType::Game&&!r.deleted{cloud_games.push(json!({"id":r.resource_id,"title":r.payload["displayTitle"],"source":r.payload["source"],"namespace":r.payload["achievementReference"]}));}
        }
        if !page.has_more{break;}if page.next_offset<=offset{return Err("云端快照未推进".into());}offset=page.next_offset;
    }
    for game in &mut cloud_games{if let Some(value)=game["id"].as_str().and_then(|id|references.get(id)){game["namespace"]=value.clone();}}
    let saved={let state=app.state::<crate::AppState>();let c=crate::lock_db(&state)?;
        let mut q=c.prepare("SELECT local_id,cloud_id FROM sync_game_links").map_err(err)?;
        let rows=q.query_map([],|r|Ok(json!({"localId":r.get::<_,String>(0)?,"cloudId":r.get::<_,String>(1)?}))).map_err(err)?.collect::<Result<Vec<_>,_>>().map_err(err)?;rows};
    let effective=super::library::restore_mappings(&local_games,&cloud_games,&saved,mappings.unwrap_or_default());
    let body=json!({"resources":resources,"localDigest":digest,"mappings":effective,"resolutions":resolutions.unwrap_or_default()});
    if serde_json::to_vec(&body).map_err(err)?.len()>64*1024*1024{return Err("导入资料超过 64 MiB，未上传任何资料；请精简定义缓存后重试".into());}
    let mut result=request(&app,"POST","/v1/imports/prepare",Some(body)).await?;
    result["effectiveMappings"]=json!(effective);
    result["localGames"]=json!(local_games);result["cloudGames"]=json!(cloud_games);
    *cloud.import_preview.lock().await=Some(result.clone());Ok(result)
}
#[tauri::command]
pub async fn cloud_commit_import(app:tauri::AppHandle)->Result<(),String>{
    let cloud=app.state::<Cloud>();let _gate=cloud.sync_gate.lock().await;
    let preview=cloud.import_preview.lock().await.clone().ok_or("请先预览导入")?;
    if preview["unresolvedCount"].as_u64().unwrap_or(0)>0{return Err("请先解决全部导入冲突并重新预览".into());}
    let (user,device)=cloud.session.lock().await.as_ref().map(|s|(s.user_id.to_string(),s.device_id.to_string())).ok_or("请重新登录")?;
    let digest={let state=app.state::<crate::AppState>();let runtime=state.runtime.lock().map_err(|_|"运行状态不可用")?;let c=crate::lock_db(&state)?;
        if crate::db::games(&c)?.iter().any(|game|matches!(runtime.info(&game.id).state.as_str(),"starting"|"running")){
            return Err("请先退出游戏并保存计时检查点，再提交导入".into());
        }
        youji_cloud_protocol::digest(&super::export::resources(&c,&device)?)?
    };
    if preview["localDigest"].as_str()!=Some(&digest){return Err("本地数据已变化，请重新预览".into());}
    let id=preview["importId"].as_str().ok_or("导入标识缺失")?;
    let result=request(&app,"POST",&format!("/v1/imports/{id}/commit"),Some(json!({"localDigest":digest}))).await?;
    let mappings=result["mappings"].as_array().or_else(||preview["mappings"].as_array()).ok_or("导入映射缺失")?;
    {let state=app.state::<crate::AppState>();let mut c=crate::lock_db(&state)?;
        store::transaction(&mut c,|tx|{
            // 网络期间的本地更改不能被快照覆盖。
            if youji_cloud_protocol::digest(&super::export::resources(tx,&device)?)?!=digest{return Err("云端已接收导入，但本地期间发生修改，请重新预览以合并".into());}
            super::library::remap_categories(tx,result["categoryMappings"].as_array().map(Vec::as_slice).unwrap_or(&[]))?;
            for m in mappings{tx.execute("INSERT INTO sync_game_links VALUES(?1,?2) ON CONFLICT(local_id) DO UPDATE SET cloud_id=excluded.cloud_id",params![m["localId"].as_str().ok_or("本地映射缺失")?,m["cloudId"].as_str().ok_or("云端映射缺失")?]).map_err(err)?;}
            tx.execute("INSERT INTO account_binding VALUES(1,?1,?2,?3) ON CONFLICT(singleton) DO UPDATE SET device_id=excluded.device_id",params![user,device,uuid::Uuid::new_v4().to_string()]).map_err(err)?;
            Ok(())
        })?;
    }
    super::worker::snapshot(&app).await?;
    *cloud.import_preview.lock().await=None;Ok(())
}
#[tauri::command]
pub async fn cloud_resolve(app:tauri::AppHandle,operation_id:String,keep_local:bool)->Result<(),String>{
    let state=app.state::<crate::AppState>();let mut c=crate::lock_db(&state)?;
    store::transaction(&mut c,|tx|store::resolve(tx,&operation_id,keep_local))
}
