// 仅导出显式白名单，永不序列化 Game 或 settings 整体。
use rusqlite::{Connection,params,OptionalExtension};
use serde_json::{json,Value};
use super::store::err;

pub fn resources(c:&Connection,device:&str)->Result<Vec<Value>,String>{
    let mut out=Vec::new();
    let mut push=|kind:&str,id:String,payload:Value|out.push(json!({"resourceType":kind,"resourceId":id,"payload":payload}));
    for game in crate::db::games(c)? {
        let meta:Value=serde_json::from_str(&game.metadata_json).unwrap_or_default();
        let identity=match game.source.as_str() {
            "steam"=>game.appid.clone(),
            "epic"=>{let ns=meta["epicNamespace"].as_str().unwrap_or("");let name=meta["epicAppName"].as_str().unwrap_or("");if ns.is_empty()||name.is_empty(){return Err(format!("{} 缺少 Epic 身份，暂不能导入",game.title));}format!("{ns}:{name}")},
            "local"=>game.id.clone(),
            _=>return Err("存在不支持的游戏来源".into()),
        };
        let ns=crate::achievement_platform::namespace(c,&game)?;
        push("game",game.id.clone(),json!({"source":game.source,"sourceIdentity":identity,"displayTitle":game.title,"metadataReference":game.appid,"achievementReference":ns}));
        push("title",game.id.clone(),json!({"value":game.title}));
        push("metadata_reference",game.id.clone(),json!({"value":game.appid}));
        push("achievement_reference",game.id.clone(),json!({"value":ns}));
        let favorite:bool=c.query_row("SELECT favorite FROM game_activity WHERE game_id=?1",[&game.id],|r|r.get(0)).optional().map_err(err)?.unwrap_or(false);
        push("favorite",game.id.clone(),json!({"value":favorite}));
        let archived:bool=c.query_row("SELECT archived FROM cloud_archives WHERE game_id=?1",[&game.id],|r|r.get(0)).optional().map_err(err)?.unwrap_or(false);
        push("archive",game.id.clone(),json!({"value":archived}));
        let baseline:i64=c.query_row("SELECT MAX(0,COALESCE((SELECT played_seconds FROM game_activity WHERE game_id=?1),0)-COALESCE((SELECT SUM(seconds) FROM play_sessions WHERE game_id=?1),0))",[&game.id],|r|r.get(0)).map_err(err)?;
        push("historical_baseline",game.id.clone(),json!({"gameId":game.id,"seconds":baseline}));
        let mut q=c.prepare("SELECT id,started_at,ended_at,seconds FROM play_sessions WHERE game_id=?1").map_err(err)?;
        let rows=q.query_map([&game.id],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,i64>(3)?))).map_err(err)?;
        for row in rows{let(id,start,end,seconds)=row.map_err(err)?;
            push("play_session",id.clone(),json!({"gameId":game.id,"ownerDeviceId":device,"checkpoint":seconds+1,"startedAt":start,"endedAt":if end.is_empty(){None}else{Some(end)},"seconds":seconds,"days":crate::statistics::store::session_days(c,&id)?,"utcOffsetSeconds":0,"historical":true}));
        }
        let mut q=c.prepare("SELECT api_name,source,COALESCE(unlocked_at,''),present FROM achievement_facts WHERE game_id=?1 AND namespace=?2 AND source IN ('manual','local','xbox-local') UNION ALL SELECT u.api_name,u.source,u.unlocked_at,1 FROM unlocks u WHERE u.game_id=?1 AND u.source IN ('manual','local','xbox-local') AND NOT EXISTS(SELECT 1 FROM achievement_facts f WHERE f.game_id=u.game_id AND f.api_name=u.api_name AND f.source=u.source)").map_err(err)?;
        let rows=q.query_map(params![game.id,ns],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,bool>(3)?))).map_err(err)?;
        for row in rows {let(api,source,time,present)=row.map_err(err)?;
            let id=youji_cloud_protocol::digest(&(&game.id,&ns,&api,&source))?;
            push(if source=="manual"{"manual_unlock"}else{"automatic_unlock"},id,json!({"gameId":game.id,"namespace":ns,"apiName":api,"source":source,"unlockedAt":if chrono::DateTime::parse_from_rfc3339(&time).is_ok(){Some(time)}else{None},"present":present}));
        }
        let mut q=c.prepare("SELECT api_name,name,description,icon,hidden FROM achievements WHERE appid=?1").map_err(err)?;
        let definitions=q.query_map([&ns],|r|{let icon:String=r.get(3)?;Ok(json!({"apiName":r.get::<_,String>(0)?,"name":r.get::<_,String>(1)?,"description":r.get::<_,String>(2)?,"icon":if icon.starts_with("https://"){icon}else{String::new()},"hidden":r.get::<_,bool>(4)?}))}).map_err(err)?.collect::<Result<Vec<_>,_>>().map_err(err)?;
        if !ns.is_empty()&&!definitions.is_empty(){push("definition_cache",game.id.clone(),json!({"gameId":game.id,"namespace":ns,"language":"zh-CN","source":game.schema_source,"complete":game.schema_source=="Steam Web API"||game.schema_source=="Epic 官方成就","definitions":definitions}));}
    }
    let steam=crate::db::setting(c,"steam_id")?;
    if !steam.is_empty(){
        push("platform_account","steam".into(),json!({"provider":"steam","accountId":steam}));
        let cache=crate::db::setting(c,&format!("steam_playtime:{steam}"))?;
        if let Ok(cache)=serde_json::from_str::<Value>(&cache){
            for game in crate::db::games(c)?.into_iter().filter(|g|g.source=="steam"){
                if let Some(seconds)=cache["games"][&game.appid]["seconds"].as_u64(){
                    push("official_playtime",game.id.clone(),json!({"gameId":game.id,"provider":"steam","accountId":steam,"seconds":seconds,"checkedAt":cache["checkedAt"]}));
                }
            }
        }
    }
    let epic=crate::db::setting(c,"epic_oauth_session")?;
    if let Ok(epic)=serde_json::from_str::<Value>(&epic){if let Some(account)=epic["account_id"].as_str().or_else(||epic["accountId"].as_str()){push("platform_account","epic".into(),json!({"provider":"epic","accountId":account}));}}
    let mut events=c.prepare("SELECT game_id,namespace,api_name,recorded_at,date FROM statistics_unlock_events").map_err(err)?;
    let rows=events.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,String>(4)?))).map_err(err)?;
    for row in rows{let(game,namespace,api,time,date)=row.map_err(err)?;push("unlock_event",youji_cloud_protocol::digest(&(&game,&namespace,&api,"event"))?,json!({"gameId":game,"namespace":namespace,"apiName":api,"recordedAt":time,"date":date}));}
    let org=crate::organization::store::read(c)?;
    for t in org.tags{push("tag",t.id,json!({"name":t.name}));}
    let order:Vec<_>=org.collections.iter().map(|x|x.id.clone()).collect();
    for t in org.collections{push("collection",t.id,json!({"name":t.name}));}
    push("collection_order","order".into(),json!({"ids":order}));
    for(kind,items)in [("tag_member",org.game_tags),("collection_member",org.game_collections)]{
        for(game,category)in items{push(kind,format!("{game}:{category}"),json!({"gameId":game,"categoryId":category,"present":true}));}
    }
    Ok(out)
}

pub fn checkpoint(c:&Connection,id:&str)->Result<(),String>{
    if !c.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='account_binding')",[],|r|r.get::<_,bool>(0)).map_err(err)? {return Ok(());}
    if !crate::runtime_persistence::recoverable(c,id)?{return Err("历史或其他设备会话不能继续计时".into());}
    let binding:Option<String>=c.query_row("SELECT device_id FROM account_binding",[],|r|r.get(0)).optional().map_err(err)?;
    let Some(device)=binding else{return Ok(());};
    let(game,start,end,seconds):(String,String,String,i64)=c.query_row("SELECT game_id,started_at,ended_at,seconds FROM play_sessions WHERE id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).map_err(err)?;
    let cloud:Option<String>=c.query_row("SELECT cloud_id FROM sync_game_links WHERE local_id=?1",[game],|r|r.get(0)).optional().map_err(err)?;
    let Some(game)=cloud else{return Ok(());};
    c.execute("INSERT OR IGNORE INTO cloud_session_owners VALUES(?1,?2,0)",params![id,device]).map_err(err)?;
    let owner:String=c.query_row("SELECT device_id FROM cloud_session_owners WHERE session_id=?1",[id],|r|r.get(0)).map_err(err)?;
    if owner!=device{return Err("其他设备的会话不能在本机继续计时".into());}
    c.execute("UPDATE cloud_session_owners SET checkpoint=checkpoint+1 WHERE session_id=?1",[id]).map_err(err)?;
    let checkpoint:i64=c.query_row("SELECT checkpoint FROM cloud_session_owners WHERE session_id=?1",[id],|r|r.get(0)).map_err(err)?;
    super::store::enqueue(c,youji_cloud_protocol::ResourceType::PlaySession,id,json!({"gameId":game,"ownerDeviceId":device,"checkpoint":checkpoint,"startedAt":start,"endedAt":if end.is_empty(){None}else{Some(end)},"seconds":seconds,"days":crate::statistics::store::session_days(c,id)?,"utcOffsetSeconds":chrono::Local::now().offset().local_minus_utc(),"historical":false}))
}
