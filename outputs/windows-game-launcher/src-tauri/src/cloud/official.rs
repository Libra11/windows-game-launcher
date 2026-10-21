use rusqlite::{params,OptionalExtension,Connection};
use serde_json::json;
use tauri::Manager;
use super::store::{self,err};
use youji_cloud_protocol::ResourceType as K;

pub struct Capture {game:String,provider:String,account:String,namespace:String,revision:i64}
pub async fn begin(app:&tauri::AppHandle,game:&crate::model::Game,provider:&str,account:&str)->Result<Option<Capture>,String>{
    let cloud_id={
        let state=app.state::<crate::AppState>();let c=crate::lock_db(&state)?;
        if !c.query_row("SELECT EXISTS(SELECT 1 FROM account_binding)",[],|r|r.get::<_,bool>(0)).map_err(err)?{return Ok(None);}
        c.query_row("SELECT cloud_id FROM sync_game_links WHERE local_id=?1",[&game.id],|r|r.get::<_,String>(0)).optional().map_err(err)?
    };
    let Some(cloud_id)=cloud_id else{return Ok(None);};
    // 在平台网络请求之前取得云端版本，不能给旧官方响应改版本重传。
    super::worker::synchronize(app).await?;
    {
        let state=app.state::<crate::AppState>();let c=crate::lock_db(&state)?;
        let existing:Option<String>=c.query_row("SELECT resource FROM sync_shadow WHERE kind='platform_account' AND id=?1",[provider],|r|r.get(0)).optional().map_err(err)?;
        if let Some(raw)=existing {
            let r:youji_cloud_protocol::Resource=serde_json::from_str(&raw).map_err(err)?;
            if r.payload["accountId"].as_str()!=Some(account){return Err("当前平台账号与游迹云绑定的身份不一致".into());}
        }else{store::enqueue(&c,K::PlatformAccount,provider,json!({"provider":provider,"accountId":account}))?;}
    }
    super::worker::synchronize(app).await?;
    let state=app.state::<crate::AppState>();let c=crate::lock_db(&state)?;
    let revision=c.query_row("SELECT revision FROM sync_shadow WHERE kind='official_snapshot' AND id=?1",[&cloud_id],|r|r.get(0)).optional().map_err(err)?.unwrap_or(0);
    let namespace=crate::achievement_platform::namespace(&c,game)?;
    Ok(Some(Capture{game:cloud_id,provider:provider.into(),account:account.into(),namespace,revision}))
}
pub fn save(c:&Connection,capture:Option<&Capture>,items:&[(String,String)])->Result<(),String>{
    let Some(cap)=capture else{return Ok(());};
    let unlocks:std::collections::BTreeMap<_,_>=items.iter().map(|(api,time)|(api.clone(),if chrono::DateTime::parse_from_rfc3339(time).is_ok(){Some(time.clone())}else{None})).collect();
    let payload=json!({"gameId":cap.game,"provider":cap.provider,"accountId":cap.account,"namespace":cap.namespace,"unlocks":unlocks});
    // 官方快照不能依赖旧待发送快照自动抬高 revision。
    c.execute("UPDATE sync_outbox SET status='superseded' WHERE kind='official_snapshot' AND id=?1 AND status IN ('pending','conflict')",[&cap.game]).map_err(err)?;
    c.execute("DELETE FROM sync_conflicts WHERE operation_id IN (SELECT operation_id FROM sync_outbox WHERE kind='official_snapshot' AND id=?1)",[&cap.game]).map_err(err)?;
    c.execute("INSERT INTO sync_outbox(operation_id,kind,id,payload,base_revision) VALUES(?1,'official_snapshot',?2,?3,?4)",params![uuid::Uuid::new_v4().to_string(),cap.game,payload.to_string(),cap.revision]).map_err(err)?;
    Ok(())
}
pub fn record_facts(c:&Connection,game:&str,namespace:&str,provider:&str,items:&[(String,String)])->Result<(),String>{
    c.execute("UPDATE achievement_facts SET present=0 WHERE game_id=?1 AND namespace=?2 AND source=?3",params![game,namespace,provider]).map_err(err)?;
    for(api,time)in items{
        let time=if chrono::DateTime::parse_from_rfc3339(time).is_ok(){Some(time)}else{None};
        c.execute("INSERT INTO achievement_facts VALUES(?1,?2,?3,?4,?5,1) ON CONFLICT(game_id,namespace,api_name,source) DO UPDATE SET unlocked_at=excluded.unlocked_at,present=1",params![game,namespace,api,provider,time]).map_err(err)?;
    }
    let current=crate::db::game(c,game)?.ok_or("成就游戏不存在")?;
    if crate::achievement_platform::namespace(c,&current)?!=namespace{return Ok(());}
    let mut q=c.prepare("SELECT DISTINCT api_name FROM achievement_facts WHERE game_id=?1 AND namespace=?2 UNION SELECT api_name FROM unlocks WHERE game_id=?1").map_err(err)?;
    let names=q.query_map(params![game,namespace],|r|r.get::<_,String>(0)).map_err(err)?.collect::<Result<Vec<_>,_>>().map_err(err)?;
    for api in names{super::projection::project_unlock(c,game,namespace,&api)?;}Ok(())
}
pub fn seed_facts(c:&Connection)->Result<(),String>{
    for game in crate::db::games(c)?{
        let namespace=crate::achievement_platform::namespace(c,&game)?;
        c.execute("INSERT OR IGNORE INTO achievement_facts(game_id,namespace,api_name,source,unlocked_at,present) SELECT game_id,?1,api_name,source,NULLIF(unlocked_at,''),1 FROM unlocks WHERE game_id=?2",params![namespace,game.id]).map_err(err)?;
    }Ok(())
}
pub fn account_guard(c:&Connection,provider:&str,account:&str)->Result<(),String>{
    let raw:Option<String>=c.query_row("SELECT resource FROM sync_shadow WHERE kind='platform_account' AND id=?1",[provider],|r|r.get(0)).optional().map_err(err)?;
    if let Some(raw)=raw{let resource:youji_cloud_protocol::Resource=serde_json::from_str(&raw).map_err(err)?;
        if resource.payload["accountId"].as_str()!=Some(account){return Err("当前平台账号与游迹云绑定身份不同，已停止替换官方成就和时长".into());}}
    Ok(())
}
fn guard_app(app:&tauri::AppHandle,provider:&str,account:&str)->Result<(),String>{
    let state=app.state::<crate::AppState>();let c=crate::lock_db(&state)?;account_guard(&c,provider,account)
}
pub async fn capture(app:&tauri::AppHandle,game:&crate::model::Game,provider:&str,account:&str)->Result<Option<Capture>,String>{
    guard_app(app,provider,account)?;
    let result=begin(app,game,provider,account).await;
    guard_app(app,provider,account)?;
    match result{
        Ok(value)=>Ok(value),
        Err(error)=>{*app.state::<super::Cloud>().last_error.lock().await=Some(format!("官方云同步已暂停：{error}"));Ok(None)}
    }
}
pub struct PlaytimeCapture { account:String,revisions:std::collections::HashMap<String,i64> }
pub async fn playtime_begin(app:&tauri::AppHandle,account:&str)->Result<Option<PlaytimeCapture>,String>{
    guard_app(app,"steam",account)?;
    let result=async{
        {let state=app.state::<crate::AppState>();let c=crate::lock_db(&state)?;
         if !c.query_row("SELECT EXISTS(SELECT 1 FROM account_binding)",[],|r|r.get::<_,bool>(0)).map_err(err)?{return Ok(None);}}
        super::worker::synchronize(app).await?;
        {let state=app.state::<crate::AppState>();let c=crate::lock_db(&state)?;
            let existing:Option<String>=c.query_row("SELECT resource FROM sync_shadow WHERE kind='platform_account' AND id='steam'",[],|r|r.get(0)).optional().map_err(err)?;
            if let Some(raw)=existing{
                let r:youji_cloud_protocol::Resource=serde_json::from_str(&raw).map_err(err)?;
                if r.payload["accountId"].as_str()!=Some(account){return Err("Steam 时长账号与云账号绑定不一致".into());}
            }else{store::enqueue(&c,K::PlatformAccount,"steam",json!({"provider":"steam","accountId":account}))?;}
        }
        super::worker::synchronize(app).await?;
        let state=app.state::<crate::AppState>();let c=crate::lock_db(&state)?;
        let mut q=c.prepare("SELECT id,revision FROM sync_shadow WHERE kind='official_playtime'").map_err(err)?;
        let revisions=q.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,i64>(1)?))).map_err(err)?.collect::<Result<_,_>>().map_err(err)?;
        Ok::<_,String>(Some(PlaytimeCapture{account:account.into(),revisions}))
    }.await;
    guard_app(app,"steam",account)?;
    match result{Ok(v)=>Ok(v),Err(e)=>{*app.state::<super::Cloud>().last_error.lock().await=Some(e);Ok(None)}}
}
pub fn playtime_save(c:&Connection,capture:Option<&PlaytimeCapture>,games:&[crate::model::Game])->Result<(),String>{
    let Some(cap)=capture else{return Ok(());};
    for game in games{
        let meta:serde_json::Value=serde_json::from_str(&game.metadata_json).map_err(err)?;
        let Some(seconds)=meta["steamPlayedSeconds"].as_u64() else{continue;};
        let id:Option<String>=c.query_row("SELECT cloud_id FROM sync_game_links WHERE local_id=?1",[&game.id],|r|r.get(0)).optional().map_err(err)?;
        let Some(id)=id else{continue;};
        c.execute("UPDATE sync_outbox SET status='superseded' WHERE kind='official_playtime' AND id=?1 AND status IN ('pending','conflict')",[&id]).map_err(err)?;
        c.execute("DELETE FROM sync_conflicts WHERE operation_id IN (SELECT operation_id FROM sync_outbox WHERE kind='official_playtime' AND id=?1)",[&id]).map_err(err)?;
        c.execute("INSERT INTO sync_outbox(operation_id,kind,id,payload,base_revision) VALUES(?1,'official_playtime',?2,?3,?4)",params![uuid::Uuid::new_v4().to_string(),id,json!({"gameId":id,"provider":"steam","accountId":cap.account,"seconds":seconds,"checkedAt":chrono::Utc::now().to_rfc3339()}).to_string(),cap.revisions.get(&id).copied().unwrap_or(0)]).map_err(err)?;
    }Ok(())
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn official_results_keep_request_start_revision(){
        let dir=tempfile::tempdir().unwrap();
        let c=crate::db::open(&dir.path().join("official.sqlite")).unwrap();
        let local="steam-480";let game=uuid::Uuid::new_v4().to_string();
        c.execute("INSERT INTO games(id,source,appid,title) VALUES(?1,'steam','480','游戏')",[local]).unwrap();
        c.execute("INSERT INTO sync_game_links VALUES(?1,?2)",params![local,game]).unwrap();
        c.execute("INSERT INTO account_binding VALUES(1,?1,?2,?3)",params![uuid::Uuid::new_v4().to_string(),uuid::Uuid::new_v4().to_string(),uuid::Uuid::new_v4().to_string()]).unwrap();
        let cap=Capture{game:game.clone(),provider:"steam".into(),account:"100".into(),namespace:"480".into(),revision:3};
        let time=PlaytimeCapture{account:"100".into(),revisions:[(game.clone(),4)].into()};
        for kind in [K::OfficialSnapshot,K::OfficialPlaytime]{
            c.execute("INSERT INTO sync_shadow VALUES(?1,?2,9,'{}')",params![kind.key(),game]).unwrap();
        }
        save(&c,Some(&cap),&[("A".into(),"2026-10-21T00:00:00Z".into())]).unwrap();
        let g=crate::model::Game{id:local.into(),metadata_json:json!({"steamPlayedSeconds":100}).to_string(),..Default::default()};
        playtime_save(&c,Some(&time),&[g]).unwrap();
        let ops=store::pending(&c).unwrap();
        assert_eq!(ops.iter().find(|o|o.resource_type==K::OfficialSnapshot).unwrap().base_revision,3);
        assert_eq!(ops.iter().find(|o|o.resource_type==K::OfficialPlaytime).unwrap().base_revision,4);
    }
}
