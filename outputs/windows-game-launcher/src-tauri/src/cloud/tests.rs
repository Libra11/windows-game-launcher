use super::*;
use rusqlite::{params,Connection};
use serde_json::json;
use youji_cloud_protocol::{Resource,ResourceType as K,OperationResult,Outcome};
fn setup()->(tempfile::TempDir,Connection,String,String){
    let dir=tempfile::tempdir().unwrap();
    let c=crate::db::open(&dir.path().join("test.sqlite")).unwrap();
    let local=format!("local-{}",uuid::Uuid::new_v4());
    let cloud=uuid::Uuid::new_v4().to_string();
    let g=crate::model::Game{id:local.clone(),source:"local".into(),appid:"480".into(),title:"测试".into(),exe_path:"D:/Private/game.exe".into(),metadata_json:"{}".into(),..Default::default()};
    crate::db::upsert_game(&c,&g).unwrap();
    c.execute("INSERT INTO sync_game_links VALUES(?1,?2)",params![local,cloud]).unwrap();
    c.execute("INSERT INTO account_binding VALUES(1,?1,?2,?3)",params![uuid::Uuid::new_v4().to_string(),uuid::Uuid::new_v4().to_string(),uuid::Uuid::new_v4().to_string()]).unwrap();
    (dir,c,local,cloud)
}
fn resource(kind:K,id:&str,payload:serde_json::Value,revision:i64)->Resource{
    Resource{resource_type:kind,resource_id:id.into(),payload,revision,deleted:false,device_id:uuid::Uuid::new_v4(),updated_at:"2026-10-21T00:00:00Z".into()}
}
#[test]
fn repeated_scans_preserve_import_digest_and_do_not_enqueue_again(){
    for bound in [false,true] {
        let(_dir,c,local,_cloud)=setup();
        if !bound {c.execute("DELETE FROM account_binding",[]).unwrap();}
        let first="2026-10-21T10:00:00Z";
        let later="2026-10-21T10:00:30Z";
        for source in ["local","xbox-local"] {
            assert!(crate::db::add_unlock(&c,&local,source,source,first,"scan").unwrap());
        }
        let before=export::import_digest(&export::resources(&c,"device").unwrap()).unwrap();
        let pending=store::pending(&c).unwrap();
        for source in ["local","xbox-local"] {
            assert!(!crate::db::add_unlock(&c,&local,source,source,later,"scan").unwrap());
            let time:String=c.query_row("SELECT unlocked_at FROM achievement_facts WHERE game_id=?1 AND api_name=?2",params![local,source],|r|r.get(0)).unwrap();
            assert_eq!(time,first);
        }
        assert_eq!(before,export::import_digest(&export::resources(&c,"device").unwrap()).unwrap());
        assert_eq!(serde_json::to_value(pending).unwrap(),serde_json::to_value(store::pending(&c).unwrap()).unwrap());
        assert!(crate::db::add_unlock(&c,&local,"new","local",later,"scan").unwrap());
        assert_ne!(before,export::import_digest(&export::resources(&c,"device").unwrap()).unwrap());
    }
}
#[test]
fn automatic_promotion_keeps_its_own_first_time(){
    let(_dir,c,local,_cloud)=setup();
    crate::db::add_unlock(&c,&local,"a","manual","2026-10-21T09:00:00Z","").unwrap();
    crate::db::add_unlock(&c,&local,"a","local","2026-10-21T10:00:00Z","scan").unwrap();
    crate::db::add_unlock(&c,&local,"a","local","2026-10-21T11:00:00Z","scan").unwrap();
    let time:String=c.query_row("SELECT unlocked_at FROM achievement_facts WHERE game_id=?1 AND api_name='a' AND source='local'",[local],|r|r.get(0)).unwrap();
    assert_eq!(time,"2026-10-21T10:00:00Z");
}
#[test]
fn atomic_outbox_failure_rolls_back_favorite_and_game(){
    let(_dir,c,local,_cloud)=setup();
    c.execute_batch("CREATE TRIGGER fail_outbox BEFORE INSERT ON sync_outbox BEGIN SELECT RAISE(ABORT,'test failure'); END;").unwrap();
    assert!(crate::activity::set_favorite(&c,&local,true).is_err());
    assert_eq!(c.query_row("SELECT COUNT(*) FROM game_activity",[],|r|r.get::<_,i64>(0)).unwrap(),0);
    let g=crate::model::Game{id:format!("local-{}",uuid::Uuid::new_v4()),source:"local".into(),title:"不能保存".into(),metadata_json:"{}".into(),..Default::default()};
    assert!(crate::db::upsert_game(&c,&g).is_err());
    assert!(crate::db::game(&c,&g.id).unwrap().is_none());
    assert_eq!(c.query_row("SELECT COUNT(*) FROM sync_game_links",[],|r|r.get::<_,i64>(0)).unwrap(),1);
}
#[test]
fn pull_keeps_pending_and_dependency_uses_ack_revision(){
    let(_dir,mut c,local,cloud)=setup();
    crate::activity::set_favorite(&c,&local,true).unwrap();
    let first=store::pending(&c).unwrap().remove(0);
    crate::activity::set_favorite(&c,&local,false).unwrap();
    assert_eq!(store::pending(&c).unwrap().len(),1);
    store::transaction(&mut c,|tx|store::apply(tx,&resource(K::Favorite,&cloud,json!({"value":true}),2))).unwrap();
    assert!(!crate::activity::all(&c).unwrap()[&local].favorite);
    let ack=OperationResult{operation_id:first.operation_id,outcome:Outcome::Applied,revision:3,sequence:3,current:Some(resource(K::Favorite,&cloud,json!({"value":true}),3)),reason:String::new()};
    store::transaction(&mut c,|tx|store::result(tx,&ack)).unwrap();
    let second=store::pending(&c).unwrap().remove(0);
    assert_eq!(second.base_revision,3);
    assert_eq!(second.depends_on_operation_id,Some(first.operation_id));
    assert!(!crate::activity::all(&c).unwrap()[&local].favorite);
}
#[test]
fn projection_never_overwrites_paths_or_queues_itself(){
    let(_dir,mut c,local,cloud)=setup();
    store::transaction(&mut c,|tx|{
        store::apply(tx,&resource(K::Game,&cloud,json!({"source":"local","sourceIdentity":local,"displayTitle":"云名称","metadataReference":"480","achievementReference":"480"}),1))?;
        store::apply(tx,&resource(K::Title,&cloud,json!({"value":"新名称"}),2))
    }).unwrap();
    let game=crate::db::game(&c,&local).unwrap().unwrap();
    assert_eq!(game.exe_path,"D:/Private/game.exe");assert_eq!(game.title,"新名称");
    assert!(store::pending(&c).unwrap().is_empty());
}
#[test]
fn archive_keeps_underlying_game_and_history(){
    let(_dir,c,local,_)=setup();
    c.execute("INSERT INTO cloud_archives VALUES(?1,1)",[&local]).unwrap();
    assert_eq!(crate::db::games(&c).unwrap().len(),1);
    assert_eq!(store::pending(&c).unwrap()[0].resource_type,K::Archive);
}
#[test]
fn manual_cancel_does_not_remove_automatic_fact(){
    let(_dir,mut c,local,cloud)=setup();
    let ns=crate::achievement_platform::namespace(&c,&crate::db::game(&c,&local).unwrap().unwrap()).unwrap();
    store::transaction(&mut c,|tx|{
        for(source,present,kind)in [("local",true,K::AutomaticUnlock),("manual",false,K::ManualUnlock)]{
            let id=youji_cloud_protocol::unlock_id(cloud.parse().unwrap(),&ns,"A",source);
            store::apply(tx,&resource(kind,&id,json!({"gameId":cloud,"namespace":ns,"apiName":"A","source":source,"unlockedAt":null,"present":present}),1))?;
        }Ok(())
    }).unwrap();
    assert_eq!(c.query_row("SELECT source FROM unlocks WHERE game_id=?1",[local],|r|r.get::<_,String>(0)).unwrap(),"local");
}
#[test]
fn session_facts_sum_and_repeat_does_not_double_count(){
    let(_dir,mut c,local,cloud)=setup();
    let device=uuid::Uuid::new_v4();
    let mut all=vec![resource(K::HistoricalBaseline,&cloud,json!({"gameId":cloud,"seconds":360000}),1)];
    for seconds in [7200,10800]{
        all.push(resource(K::PlaySession,&uuid::Uuid::new_v4().to_string(),json!({"gameId":cloud,"ownerDeviceId":device,"checkpoint":1,"startedAt":"2026-10-21T00:00:00Z","endedAt":"2026-10-21T03:00:00Z","seconds":seconds,"days":{"2026-10-21":seconds},"utcOffsetSeconds":0,"historical":false}),1));
    }
    for _ in 0..2{store::transaction(&mut c,|tx|{for r in &all{store::apply(tx,r)?;}Ok(())}).unwrap();}
    assert_eq!(crate::activity::all(&c).unwrap()[&local].played_seconds,105*3600);
}
#[test]
fn category_delete_has_no_rejected_member_removal(){
    let(_dir,mut c,local,_)=setup();
    let tag=crate::organization::store::create(&mut c,false,"标签").unwrap();
    crate::organization::store::set(&mut c,&local,vec![tag.clone()],vec![]).unwrap();
    c.execute("UPDATE sync_outbox SET status='done'",[]).unwrap();
    crate::organization::store::remove(&mut c,false,&tag).unwrap();
    let pending=store::pending(&c).unwrap();
    assert_eq!(pending.len(),1);assert_eq!(pending[0].action,youji_cloud_protocol::Action::Delete);
}
#[test]
fn restore_discards_device_and_outbox_but_not_credentials_in_export(){
    let(_dir,c,local,_)=setup();
    crate::activity::set_favorite(&c,&local,true).unwrap();
    let device=c.query_row("SELECT device_id FROM account_binding",[],|r|r.get::<_,String>(0)).unwrap();
    let export=serde_json::to_string(&export::resources(&c,&device).unwrap()).unwrap();
    assert!(!export.contains("D:/Private"));assert!(!export.contains("exePath"));
    reset_for_restore(&c).unwrap();
    assert_eq!(c.query_row("SELECT COUNT(*) FROM account_binding",[],|r|r.get::<_,i64>(0)).unwrap(),0);
    assert!(store::pending(&c).unwrap().is_empty());
}
#[test]
fn resolving_conflict_keeps_latest_intent_and_delete_action(){
    let(_dir,mut c,local,cloud)=setup();
    crate::activity::set_favorite(&c,&local,true).unwrap();
    let first=store::pending(&c).unwrap()[0].clone();
    crate::activity::set_favorite(&c,&local,false).unwrap();
    let conflict=OperationResult{operation_id:first.operation_id,outcome:Outcome::Conflict,revision:4,sequence:4,current:Some(resource(K::Favorite,&cloud,json!({"value":true}),4)),reason:"其他设备修改".into()};
    store::transaction(&mut c,|tx|store::result(tx,&conflict)).unwrap();
    store::transaction(&mut c,|tx|store::resolve(tx,&first.operation_id.to_string(),true)).unwrap();
    let op=store::pending(&c).unwrap().remove(0);
    assert_eq!(op.payload,json!({"value":false}));assert_eq!(op.base_revision,4);
    assert!(op.depends_on_operation_id.is_none());
    let tag=crate::organization::store::create(&mut c,false,"待删").unwrap();
    c.execute("UPDATE sync_outbox SET status='done'",[]).unwrap();
    crate::organization::store::remove(&mut c,false,&tag).unwrap();
    let deletion=store::pending(&c).unwrap().remove(0);
    c.execute("UPDATE sync_outbox SET status='conflict' WHERE operation_id=?1",[deletion.operation_id.to_string()]).unwrap();
    store::transaction(&mut c,|tx|store::resolve(tx,&deletion.operation_id.to_string(),true)).unwrap();
    assert_eq!(store::pending(&c).unwrap()[0].action,youji_cloud_protocol::Action::Delete);
}
#[test]
fn concurrent_platform_creation_rekeys_pending_without_changing_path(){
    let(_dir,mut c,local,old)=setup();
    crate::activity::set_favorite(&c,&local,true).unwrap();
    let new=uuid::Uuid::new_v4().to_string();
    let r=resource(K::Game,&new,json!({"source":"local","sourceIdentity":local,"displayTitle":"云端","metadataReference":"480","achievementReference":"480"}),1);
    store::transaction(&mut c,|tx|store::apply(tx,&r)).unwrap();
    assert_eq!(c.query_row("SELECT cloud_id FROM sync_game_links WHERE local_id=?1",[&local],|r|r.get::<_,String>(0)).unwrap(),new);
    let pending=store::pending(&c).unwrap();
    assert!(pending.iter().all(|o|o.resource_id!=old));
    assert!(pending.iter().any(|o|o.resource_id==new&&o.resource_type==K::Favorite));
    assert_eq!(crate::db::game(&c,&local).unwrap().unwrap().exe_path,"D:/Private/game.exe");
}
#[test]
fn completed_capture_enqueues_session_and_days_atomically(){
    let(_dir,mut c,local,_)=setup();
    let id=uuid::Uuid::new_v4().to_string();
    crate::activity::start(&mut c,&id,&local,"2026-10-21T00:00:00Z").unwrap();
    let days=[("2026-10-21".into(),123)].into();
    crate::activity::checkpoint(&mut c,&id,123,true,&days).unwrap();
    let pending=store::pending(&c).unwrap();
    let session=pending.iter().find(|o|o.resource_type==K::PlaySession).unwrap();
    assert_eq!(session.payload["seconds"],123);assert_eq!(session.payload["days"]["2026-10-21"],123);
    assert_eq!(session.payload["historical"],false);
}
#[test]
fn late_ack_replays_newer_shadow_after_pending_clears(){
    let(_dir,mut c,local,cloud)=setup();
    crate::activity::set_favorite(&c,&local,true).unwrap();
    let op=store::pending(&c).unwrap().remove(0);
    store::transaction(&mut c,|tx|store::apply(tx,&resource(K::Favorite,&cloud,json!({"value":false}),2))).unwrap();
    assert!(crate::activity::all(&c).unwrap()[&local].favorite);
    store::transaction(&mut c,|tx|store::result(tx,&OperationResult{
        operation_id:op.operation_id,outcome:Outcome::Applied,revision:1,sequence:1,
        current:Some(resource(K::Favorite,&cloud,json!({"value":true}),1)),reason:String::new()
    })).unwrap();
    assert!(!crate::activity::all(&c).unwrap()[&local].favorite);
}
#[test]
fn snapshot_orders_account_before_official_and_keeps_source_facts(){
    let(_dir,mut c,local,cloud)=setup();
    let ns="480";
    let manual=resource(K::ManualUnlock,&youji_cloud_protocol::unlock_id(cloud.parse().unwrap(),ns,"A","manual"),json!({"gameId":cloud,"namespace":ns,"apiName":"A","source":"manual","unlockedAt":null,"present":true}),1);
    let snapshot=resource(K::OfficialSnapshot,&cloud,json!({"gameId":cloud,"provider":"steam","accountId":"123","namespace":ns,"unlocks":{"A":null}}),1);
    let game=resource(K::Game,&cloud,json!({"source":"local","sourceIdentity":local,"displayTitle":"游戏","metadataReference":ns,"achievementReference":ns}),1);
    let account=resource(K::PlatformAccount,"steam",json!({"provider":"steam","accountId":"123"}),1);
    store::transaction(&mut c,|tx|store::snapshot(tx,vec![snapshot,manual,game,account])).unwrap();
    assert_eq!(c.query_row("SELECT source FROM unlocks WHERE game_id=?1",[&local],|r|r.get::<_,String>(0)).unwrap(),"steam");
    store::transaction(&mut c,|tx|store::apply(tx,&resource(K::OfficialSnapshot,&cloud,json!({"gameId":cloud,"provider":"steam","accountId":"123","namespace":ns,"unlocks":{}}),2))).unwrap();
    assert_eq!(c.query_row("SELECT source FROM unlocks WHERE game_id=?1",[&local],|r|r.get::<_,String>(0)).unwrap(),"manual");
}
#[test]
fn remote_xbox_namespace_is_display_only_and_epic_identity_is_stable(){
    let(_dir,mut c,_,_)=setup();
    for(source,identity,namespace)in [("local",format!("local-{}",uuid::Uuid::new_v4()),"xbox:123".to_string()),("epic","namespace:appname".into(),"epic:epic-appname".into())]{
        let id=uuid::Uuid::new_v4().to_string();
        let game=resource(K::Game,&id,json!({"source":source,"sourceIdentity":identity,"displayTitle":"云游戏","metadataReference":"","achievementReference":namespace}),1);
        store::transaction(&mut c,|tx|store::snapshot(tx,vec![game])).unwrap();
        let local=projection::local(&c,&id).unwrap();
        let g=crate::db::game(&c,&local).unwrap().unwrap();
        assert_eq!(crate::achievement_platform::namespace(&c,&g).unwrap(),namespace);
        assert!(g.exe_path.is_empty());
        assert_eq!(crate::db::setting(&c,&format!("achievement_platform:{local}")).unwrap(),"");
        if source=="epic"{assert_eq!(local,"epic-appname");}
    }
}

#[test]
fn archive_all_sources_is_atomic_and_preserves_history(){
    let(_dir,mut c,local,_cloud)=setup();
    for source in ["local","steam","epic"]{
        c.execute("UPDATE games SET source=?1 WHERE id=?2",params![source,local]).unwrap();
        let tx=c.transaction().unwrap();
        assert!(library::set_archived(&tx,&local,true,true).is_err());
        library::set_archived(&tx,&local,true,false).unwrap();tx.commit().unwrap();
        assert!(crate::db::game(&c,&local).unwrap().is_some());
        let tx=c.transaction().unwrap();library::set_archived(&tx,&local,false,false).unwrap();tx.commit().unwrap();
    }
    assert_eq!(c.query_row("SELECT COUNT(*) FROM sync_outbox WHERE kind='archive'",[],|r|r.get::<_,i64>(0)).unwrap(),6);
    c.execute_batch("CREATE TRIGGER fail_archive BEFORE INSERT ON sync_outbox BEGIN SELECT RAISE(ABORT,'failure'); END;").unwrap();
    {let tx=c.transaction().unwrap();assert!(library::set_archived(&tx,&local,true,false).is_err());}
    assert_eq!(c.query_row("SELECT archived FROM cloud_archives WHERE game_id=?1",[&local],|r|r.get::<_,i64>(0)).unwrap(),0);
}
#[test]
fn old_official_namespace_keeps_facts_without_polluting_projection(){
    let(_dir,mut c,local,cloud)=setup();
    store::transaction(&mut c,|tx|store::snapshot(tx,vec![
        resource(K::AchievementReference,&cloud,json!({"value":"xbox:123"}),2),
        resource(K::PlatformAccount,"steam",json!({"provider":"steam","accountId":"123"}),1),
        resource(K::OfficialSnapshot,&cloud,json!({"gameId":cloud,"provider":"steam","accountId":"123","namespace":"480","unlocks":{"OLD":null}}),1),
    ])).unwrap();
    assert_eq!(c.query_row("SELECT COUNT(*) FROM achievement_facts WHERE game_id=?1 AND namespace='480' AND present=1",[&local],|r|r.get::<_,i64>(0)).unwrap(),1);
    assert_eq!(c.query_row("SELECT COUNT(*) FROM unlocks WHERE game_id=?1",[&local],|r|r.get::<_,i64>(0)).unwrap(),0);
}
#[test]
fn imported_open_session_is_never_runtime_recovery(){
    let(_dir,mut c,_local,cloud)=setup();let id=uuid::Uuid::new_v4().to_string();
    let device:String=c.query_row("SELECT device_id FROM account_binding",[],|r|r.get(0)).unwrap();
    store::transaction(&mut c,|tx|store::apply(tx,&resource(K::PlaySession,&id,json!({
        "gameId":cloud,"ownerDeviceId":device,"checkpoint":1,"startedAt":"2026-10-20T00:00:00Z",
        "endedAt":null,"seconds":30,"days":{},"utcOffsetSeconds":0,"historical":true
    }),1))).unwrap();
    assert!(!crate::runtime_persistence::recoverable(&c,&id).unwrap());
    assert_eq!(c.query_row("SELECT ended_at FROM play_sessions WHERE id=?1",[&id],|r|r.get::<_,String>(0)).unwrap(),"");
    assert!(crate::runtime_persistence::recoverable(&c,&uuid::Uuid::new_v4().to_string()).unwrap());
}
#[test]
fn category_mapping_before_snapshot_avoids_case_insensitive_duplicates(){
    let(_dir,mut c,local,cloud)=setup();
    let old=uuid::Uuid::new_v4().to_string();let new=uuid::Uuid::new_v4().to_string();
    c.execute("INSERT INTO library_collections VALUES(?1,'Action',0)",[&old]).unwrap();
    c.execute("INSERT INTO game_collections VALUES(?1,?2)",params![local,old]).unwrap();
    c.execute_batch("CREATE UNIQUE INDEX test_names ON library_collections(lower(name)); DELETE FROM sync_outbox;").unwrap();
    store::transaction(&mut c,|tx|{
        library::remap_categories(tx,&[json!({"kind":"collection","localId":old,"cloudId":new})])?;
        store::snapshot(tx,vec![
            resource(K::CollectionOrder,"all",json!({"ids":[new]}),1),
            resource(K::Collection,&new,json!({"name":"action"}),1),
            resource(K::CollectionMember,&format!("{cloud}:{new}"),json!({"gameId":cloud,"categoryId":new,"present":true}),1),
        ])
    }).unwrap();
    assert_eq!(c.query_row("SELECT COUNT(*) FROM library_collections",[],|r|r.get::<_,i64>(0)).unwrap(),1);
    assert_eq!(c.query_row("SELECT collection_id FROM game_collections WHERE game_id=?1",[&local],|r|r.get::<_,String>(0)).unwrap(),new);
    assert_eq!(c.query_row("SELECT COUNT(*) FROM sync_outbox",[],|r|r.get::<_,i64>(0)).unwrap(),0);
}

#[test]
fn official_account_guard_rejects_mismatch_even_when_offline(){
    let(_dir,mut c,_,_)=setup();
    store::transaction(&mut c,|tx|store::apply(tx,&resource(K::PlatformAccount,"steam",json!({"provider":"steam","accountId":"first"}),1))).unwrap();
    assert!(official::account_guard(&c,"steam","first").is_ok());
    assert!(official::account_guard(&c,"steam","other").is_err());
    assert!(official::account_guard(&c,"epic","other").is_ok());
}
#[test]
fn restored_identity_candidates_require_current_user_visibility_and_namespace(){
    let local=vec![json!({"id":"local-A","source":"local","namespace":"480"})];
    let saved=vec![json!({"localId":"local-A","cloudId":"B"})];
    assert!(library::restore_mappings(&local,&[],&saved,vec![]).is_empty());
    assert!(library::restore_mappings(&local,&[json!({"id":"B","source":"local","namespace":"999"})],&saved,vec![]).is_empty());
    assert_eq!(library::restore_mappings(&local,&[json!({"id":"B","source":"local","namespace":"480"})],&saved,vec![]),saved);
}
#[test]
fn export_includes_independently_revised_game_fields(){
    let(_dir,c,local,_)=setup();let data=export::resources(&c,&uuid::Uuid::new_v4().to_string()).unwrap();
    for kind in ["title","metadata_reference","achievement_reference"]{
        assert!(data.iter().any(|r|r["resourceType"]==kind&&r["resourceId"]==local&&r["payload"]["value"].is_string()));
    }
}
