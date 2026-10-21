use rusqlite::{params,Connection,OptionalExtension};
use youji_cloud_protocol::{Resource,ResourceType as K};
use super::store::err;

pub fn local(c:&Connection,id:&str)->Result<String,String>{
    c.query_row("SELECT local_id FROM sync_game_links WHERE cloud_id=?1",[id],|r|r.get(0)).optional().map_err(err)?.ok_or("云端游戏尚未建立本机映射".into())
}
pub fn apply(c:&Connection,r:&Resource)->Result<(),String>{
    let p=&r.payload;let text=|k:&str|p[k].as_str().unwrap_or("");
    match r.resource_type {
        K::Game=>{
            let g:youji_cloud_protocol::Game=serde_json::from_value(p.clone()).map_err(err)?;
            let id=c.query_row("SELECT local_id FROM sync_game_links WHERE cloud_id=?1",[&r.resource_id],|x|x.get::<_,String>(0)).optional().map_err(err)?;
            let id=id.unwrap_or_else(||match g.source.as_str(){"steam"=>format!("steam-{}",g.source_identity),"local"=>g.source_identity.clone(),"epic"=>format!("epic-{}",g.source_identity.split_once(':').map(|(_,name)|name).unwrap_or(&g.source_identity)),_=>format!("cloud-{}",r.resource_id)});
            super::identity::reconcile(c,&id,r)?;
            // 新云端条目没有安装路径与捕获配置，现有设备配置不被更新。
            let(uri,metadata)=match g.source.as_str(){
                "steam"=>(format!("steam://rungameid/{}",g.source_identity),serde_json::json!({})),
                "epic"=>{let(ns,name)=g.source_identity.split_once(':').ok_or("Epic 平台身份无效")?;
                    let uri=format!("com.epicgames.launcher://apps/{}?action=launch&silent=true",name);
                    (uri,serde_json::json!({"epicNamespace":ns,"epicAppName":name}))},
                _=>(String::new(),serde_json::json!({}))
            };
            c.execute("INSERT OR IGNORE INTO games(id,source,appid,title,launch_uri,metadata_json) VALUES(?1,?2,?3,?4,?5,?6)",params![id,g.source,g.metadata_reference,g.display_title,uri,metadata.to_string()]).map_err(err)?;
            c.execute("INSERT OR IGNORE INTO sync_game_links VALUES(?1,?2)",params![id,r.resource_id]).map_err(err)?;
            let reference:Option<String>=c.query_row("SELECT resource FROM sync_shadow WHERE kind='achievement_reference' AND id=?1",[&r.resource_id],|x|x.get(0)).optional().map_err(err)?;
            let reference=reference.and_then(|raw|serde_json::from_str::<Resource>(&raw).ok()).and_then(|r|r.payload["value"].as_str().map(str::to_owned)).unwrap_or(g.achievement_reference);
            crate::db::set_setting(c,&format!("cloud_achievement_namespace:{id}"),&reference)?;
        }
        K::Title|K::MetadataReference|K::AchievementReference=>{
            let id=local(c,&r.resource_id)?;
            if r.resource_type==K::Title {c.execute("UPDATE games SET title=?1 WHERE id=?2",params![text("value"),id]).map_err(err)?;}
            if r.resource_type==K::MetadataReference {c.execute("UPDATE games SET appid=?1 WHERE id=?2",params![text("value"),id]).map_err(err)?;}
            if r.resource_type==K::AchievementReference{
                crate::db::set_setting(c,&format!("cloud_achievement_namespace:{id}"),text("value"))?;
                c.execute("DELETE FROM unlocks WHERE game_id=?1",[&id]).map_err(err)?;
                let mut q=c.prepare("SELECT DISTINCT api_name FROM achievement_facts WHERE game_id=?1 AND namespace=?2").map_err(err)?;
                let names=q.query_map(params![id,text("value")],|r|r.get::<_,String>(0)).map_err(err)?.collect::<Result<Vec<_>,_>>().map_err(err)?;
                for api in names{project_unlock(c,&id,text("value"),&api)?;}
            }
            // 不把共享关联变成本机捕获授权。
        }
        K::Archive=>{c.execute("INSERT INTO cloud_archives VALUES(?1,?2) ON CONFLICT(game_id) DO UPDATE SET archived=excluded.archived",params![local(c,&r.resource_id)?,p["value"].as_bool().unwrap_or(false)]).map_err(err)?;}
        K::Favorite=>{c.execute("INSERT INTO game_activity(game_id,favorite) VALUES(?1,?2) ON CONFLICT(game_id) DO UPDATE SET favorite=excluded.favorite",params![local(c,&r.resource_id)?,p["value"].as_bool().unwrap_or(false)]).map_err(err)?;}
        K::Tag|K::Collection=>{
            let (table,relation,col)=if r.resource_type==K::Tag{("library_tags","game_tags","tag_id")}else{("library_collections","game_collections","collection_id")};
            if r.deleted {
                c.execute(&format!("DELETE FROM {relation} WHERE {col}=?1"),[&r.resource_id]).map_err(err)?;
                c.execute(&format!("DELETE FROM {table} WHERE id=?1"),[&r.resource_id]).map_err(err)?;
            }else{
                let sql=if r.resource_type==K::Tag {"INSERT INTO library_tags VALUES(?1,?2) ON CONFLICT(id) DO UPDATE SET name=excluded.name"} else {"INSERT INTO library_collections VALUES(?1,?2,(SELECT COALESCE(MAX(position),-1)+1 FROM library_collections)) ON CONFLICT(id) DO UPDATE SET name=excluded.name"};
                c.execute(sql,params![r.resource_id,text("name")]).map_err(err)?;
            }
        }
        K::TagMember|K::CollectionMember=>{
            let id=local(c,text("gameId"))?;
            let(table,column)=if r.resource_type==K::TagMember{("game_tags","tag_id")}else{("game_collections","collection_id")};
            if !r.deleted && p["present"].as_bool()==Some(true){
                c.execute(&format!("INSERT OR IGNORE INTO {table} VALUES(?1,?2)"),params![id,text("categoryId")]).map_err(err)?;
            }else{c.execute(&format!("DELETE FROM {table} WHERE game_id=?1 AND {column}=?2"),params![id,text("categoryId")]).map_err(err)?;}
        }
        K::CollectionOrder=>{if let Some(ids)=p["ids"].as_array(){for(i,id)in ids.iter().enumerate(){c.execute("UPDATE library_collections SET position=?1 WHERE id=?2",params![i as i64,id.as_str().unwrap_or("")]).map_err(err)?;}}}
        K::AutomaticUnlock|K::ManualUnlock=>{
            let id=local(c,text("gameId"))?;
            c.execute("INSERT INTO achievement_facts VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(game_id,namespace,api_name,source) DO UPDATE SET unlocked_at=excluded.unlocked_at,present=excluded.present",params![id,text("namespace"),text("apiName"),text("source"),p["unlockedAt"].as_str(),p["present"].as_bool().unwrap_or(false)]).map_err(err)?;
            let g=crate::db::game(c,&id)?.ok_or("游戏不存在")?;
            if crate::achievement_platform::namespace(c,&g)?==text("namespace"){project_unlock(c,&id,text("namespace"),text("apiName"))?;}
        }
        K::OfficialPlaytime=>{
            let v:youji_cloud_protocol::OfficialPlaytime=serde_json::from_value(p.clone()).map_err(err)?;
            if v.provider=="steam"{
                let id=local(c,&v.game_id.to_string())?;
                let game=crate::db::game(c,&id)?.ok_or("游戏不存在")?;
                let key=format!("steam_playtime:{}",v.account_id);
                let mut cached:serde_json::Value=serde_json::from_str(&crate::db::setting(c,&key)?).unwrap_or(serde_json::json!({"checkedAt":"","error":"","games":{}}));
                cached["checkedAt"]=serde_json::json!(v.checked_at);
                cached["games"][&game.appid]=serde_json::json!({"seconds":v.seconds,"lastPlayed":""});
                crate::db::set_setting(c,&key,&cached.to_string())?;
            }
        }
        K::OfficialSnapshot=>{
            let snapshot:youji_cloud_protocol::OfficialSnapshot=serde_json::from_value(p.clone()).map_err(err)?;
            let id=local(c,&snapshot.game_id.to_string())?;
            let bound:Option<String>=c.query_row("SELECT resource FROM sync_shadow WHERE kind='platform_account' AND id=?1",[&snapshot.provider],|r|r.get(0)).optional().map_err(err)?;
            let Some(bound)=bound else {return Ok(());};
            let account:Resource=serde_json::from_str(&bound).map_err(err)?;
            if account.payload["accountId"].as_str()!=Some(&snapshot.account_id){return Err("官方快照与平台账号不一致".into());}
            let items=snapshot.unlocks.into_iter().map(|(api,time)|(api,time.unwrap_or_default())).collect::<Vec<_>>();
            super::official::record_facts(c,&id,&snapshot.namespace,&snapshot.provider,&items)?;
        }
        K::HistoricalBaseline=>{
            let id=local(c,text("gameId"))?;
            c.execute("INSERT INTO historical_playtime_baselines VALUES(?1,?2) ON CONFLICT(game_id) DO UPDATE SET seconds=excluded.seconds",params![id,p["seconds"].as_i64().unwrap_or(0)]).map_err(err)?;
            total(c,&id)?;
        }
        K::PlaySession=>{
            let s:youji_cloud_protocol::PlaySession=serde_json::from_value(p.clone()).map_err(err)?;
            let id=local(c,&s.game_id.to_string())?;
            c.execute("INSERT INTO play_sessions VALUES(?1,?2,?3,?4,?5) ON CONFLICT(id) DO UPDATE SET seconds=excluded.seconds,ended_at=excluded.ended_at",params![r.resource_id,id,s.started_at,s.ended_at.unwrap_or_default(),s.seconds as i64]).map_err(err)?;
            c.execute("INSERT OR IGNORE INTO cloud_session_owners VALUES(?1,?2,?3)",params![r.resource_id,s.owner_device_id.to_string(),s.checkpoint as i64]).map_err(err)?;
            crate::statistics::store::save_days(c,&r.resource_id,&id,&s.days)?;
            total(c,&id)?;
        }
        K::UnlockEvent=>{
            c.execute("INSERT OR IGNORE INTO statistics_unlock_events VALUES(?1,?2,?3,?4,?5)",params![local(c,text("gameId"))?,text("namespace"),text("apiName"),text("recordedAt"),text("date")]).map_err(err)?;
        }
        K::DefinitionCache=>{
            let d:youji_cloud_protocol::DefinitionCache=serde_json::from_value(p.clone()).map_err(err)?;
            for v in d.definitions{c.execute("INSERT INTO achievements VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(appid,api_name) DO UPDATE SET name=excluded.name,description=excluded.description,icon=excluded.icon,hidden=excluded.hidden",params![d.namespace,v.api_name,v.name,v.description,v.icon,v.hidden]).map_err(err)?;}
        }
        _=>{} // 官方平台状态必须通过匹配账号的独立采集入口应用。
    }
    Ok(())
}
pub fn project_unlock(c:&Connection,game:&str,ns:&str,api:&str)->Result<(),String>{
    let fact:Option<(String,String)>=c.query_row("SELECT source,COALESCE(unlocked_at,'') FROM achievement_facts WHERE game_id=?1 AND namespace=?2 AND api_name=?3 AND present=1 ORDER BY source='manual',unlocked_at LIMIT 1",params![game,ns,api],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(err)?;
    if let Some((source,time))=fact{c.execute("INSERT INTO unlocks VALUES(?1,?2,?3,?4,'') ON CONFLICT(game_id,api_name) DO UPDATE SET source=excluded.source,unlocked_at=excluded.unlocked_at",params![game,api,source,time]).map_err(err)?;}
    else{c.execute("DELETE FROM unlocks WHERE game_id=?1 AND api_name=?2",params![game,api]).map_err(err)?;}
    Ok(())
}
fn total(c:&Connection,id:&str)->Result<(),String>{
    c.execute("INSERT INTO game_activity(game_id,played_seconds,last_played) SELECT ?1,COALESCE((SELECT seconds FROM historical_playtime_baselines WHERE game_id=?1),0)+COALESCE(SUM(seconds),0),COALESCE(MAX(started_at),'') FROM play_sessions WHERE game_id=?1 ON CONFLICT(game_id) DO UPDATE SET played_seconds=excluded.played_seconds,last_played=excluded.last_played",[id]).map_err(err)?;Ok(())
}
