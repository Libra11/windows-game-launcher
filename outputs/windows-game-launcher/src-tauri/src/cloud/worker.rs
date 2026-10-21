use serde_json::json;
use tauri::{Manager,Emitter};
use youji_cloud_protocol::{ChangesPage,SnapshotPage,PushRequest,OperationResult};
use super::{request,store::{self,err},Cloud};

pub async fn synchronize(app:&tauri::AppHandle)->Result<(),String>{
    let state=app.state::<Cloud>();let _guard=state.sync_gate.lock().await;
    {let state=app.state::<crate::AppState>();let c=crate::lock_db(&state)?;
        if !c.query_row("SELECT EXISTS(SELECT 1 FROM account_binding)",[],|r|r.get::<_,bool>(0)).map_err(err)?{return Ok(());}}
    let status=request(app,"GET","/v1/sync/status",None).await?;
    if status["protocolVersion"].as_u64()!=Some(youji_cloud_protocol::VERSION as u64){return Err("云同步协议版本不匹配，请更新客户端".into());}
    pull(app).await?;
    for _ in 0..50{
        let mut operations={let state=app.state::<crate::AppState>();let c=crate::lock_db(&state)?;store::pending(&c)?};
        if operations.is_empty(){break;}
        while operations.len()>1 && serde_json::to_vec(&operations).map_err(err)?.len()>youji_cloud_protocol::MAX_REQUEST_BYTES-100 {operations.pop();}
        let request_body=serde_json::to_value(PushRequest{protocol_version:youji_cloud_protocol::VERSION,operations}).map_err(err)?;
        if serde_json::to_vec(&request_body).map_err(err)?.len()>youji_cloud_protocol::MAX_REQUEST_BYTES{return Err("待同步批次过大，请减少单批数据".into());}
        let results:Vec<OperationResult>=serde_json::from_value(request(app,"POST","/v1/sync/push",Some(request_body)).await?).map_err(err)?;
        if results.is_empty(){return Err("云端未返回操作结果".into());}
        {let state=app.state::<crate::AppState>();let mut c=crate::lock_db(&state)?;
            store::transaction(&mut c,|tx|{for r in &results{store::result(tx,r)?;}Ok(())})?;}
        if results.iter().all(|r|r.outcome==youji_cloud_protocol::Outcome::DependencyPending){break;}
    }
    pull(app).await?;
    {let state=app.state::<crate::AppState>();let c=crate::lock_db(&state)?;
        c.execute("UPDATE sync_state SET last_success=?1",[chrono::Utc::now().to_rfc3339()]).map_err(err)?;}
    let _=app.emit("library-changed",());let _=app.emit("library-organization-changed",());
    Ok(())
}
async fn pull(app:&tauri::AppHandle)->Result<(),String>{
    let mut high=None;
    loop{
        let cursor={let state=app.state::<crate::AppState>();let c=crate::lock_db(&state)?;c.query_row("SELECT cursor FROM sync_state",[],|r|r.get::<_,i64>(0)).map_err(err)?};
        let url=format!("/v1/sync/changes?cursor={cursor}&limit=100{}",high.map(|h|format!("&highWatermark={h}")).unwrap_or_default());
        let page:ChangesPage=serde_json::from_value(request(app,"GET",&url,None).await?).map_err(err)?;
        if page.next_cursor<cursor || page.next_cursor>page.high_watermark || high.is_some_and(|h|h!=page.high_watermark){return Err("云端同步游标无效".into());}
        if page.has_more && page.next_cursor==cursor{return Err("云端同步分页未推进".into());}
        {let state=app.state::<crate::AppState>();let mut c=crate::lock_db(&state)?;
            store::transaction(&mut c,|tx|{for change in &page.changes{store::apply(tx,&change.resource)?;}tx.execute("UPDATE sync_state SET cursor=?1",[page.next_cursor]).map_err(err)?;Ok(())})?;}
        high=Some(page.high_watermark);if !page.has_more{break;}
    }Ok(())
}
pub async fn snapshot(app:&tauri::AppHandle)->Result<(),String>{
    let init=request(app,"POST","/v1/sync/snapshots",Some(json!({}))).await?;
    let id=init["snapshotId"].as_str().ok_or("快照标识缺失")?;
    let mut offset=0;let mut resources=Vec::new();let high=init["highWatermark"].as_i64().ok_or("快照序号缺失")?;
    loop{
        let p:SnapshotPage=serde_json::from_value(request(app,"GET",&format!("/v1/sync/snapshots/{id}?offset={offset}&limit=100"),None).await?).map_err(err)?;
        if p.high_watermark!=high || p.snapshot_id.to_string()!=id || (p.has_more&&p.next_offset<=offset){return Err("快照分页身份不匹配".into());}
        resources.extend(p.resources);offset=p.next_offset;if !p.has_more{break;}
    }
    let state=app.state::<crate::AppState>();let mut c=crate::lock_db(&state)?;
    store::transaction(&mut c,|tx|{
        // 游戏映射必须先于关系与会话投影。
        store::snapshot(tx,resources)?;
        tx.execute("UPDATE sync_state SET cursor=?1",[high]).map_err(err)?;Ok(())
    })
}
pub fn start(app:tauri::AppHandle){
    tauri::async_runtime::spawn(async move{
        let saved_login={let state=app.state::<crate::AppState>();let result=crate::lock_db(&state).and_then(|c|crate::db::setting(&c,"cloud_login_user"));result.unwrap_or_default()};
        if !saved_login.is_empty(){let _=request(&app,"GET","/v1/sync/status",None).await;}
        let mut elapsed=60u64;let mut failures=0u32;
        loop{
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;elapsed+=2;
            let should={let state=app.state::<crate::AppState>();let value=match crate::lock_db(&state){Ok(c)=>c.query_row("SELECT EXISTS(SELECT 1 FROM account_binding) AND (EXISTS(SELECT 1 FROM sync_outbox WHERE status='pending') OR ?1>=60)",[elapsed as i64],|r|r.get::<_,bool>(0)).unwrap_or(false),Err(_)=>false};value};
            if !should{continue;}
            let result=synchronize(&app).await;elapsed=0;
            let cloud=app.state::<Cloud>();*cloud.last_error.lock().await=result.err();
            if cloud.last_error.lock().await.is_some(){
                failures=(failures+1).min(8);
                let base=(2u64.pow(failures)).min(240);
                let jitter=u64::from(uuid::Uuid::new_v4().as_bytes()[0])%((base/4).max(1));
                tokio::time::sleep(std::time::Duration::from_secs((base+jitter).min(300))).await;
            }else{failures=0;}
        }
    });
}
