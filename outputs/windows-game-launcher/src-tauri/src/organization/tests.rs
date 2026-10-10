use super::{store, Changes};
use crate::{db, model::Game};
use rusqlite::Connection;
fn fixture()->Connection {
    let conn=db::open(std::path::Path::new(":memory:")).unwrap();
    for id in ["one","two"] {db::upsert_game(&conn,&Game{id:id.into(),source:"local".into(),title:id.into(),..Default::default()}).unwrap();}
    conn
}
#[test]
fn names_are_trimmed_unicode_counted_and_case_insensitively_unique() {
    let mut conn=fixture();let id=store::create(&mut conn,false,"  Action  ").unwrap();
    assert_eq!(store::read(&conn).unwrap().tags[0].name,"Action");
    assert!(store::create(&mut conn,false,"aCTION").is_err());
    assert!(store::create(&mut conn,false,"").is_err());
    assert!(store::create(&mut conn,false,"a\nb").is_err());
    assert!(store::create(&mut conn,false,&"中".repeat(41)).is_err());
    store::rename(&mut conn,false,&id,&"中".repeat(40)).unwrap();
    store::create(&mut conn,true,&"中".repeat(40)).unwrap();
}
#[test]
fn memberships_are_multiple_idempotent_and_survive_platform_refresh() {
    let mut conn=fixture();let tag=store::create(&mut conn,false,"联机").unwrap();let other=store::create(&mut conn,false,"待玩").unwrap();
    let first=store::create(&mut conn,true,"周末").unwrap();let second=store::create(&mut conn,true,"朋友").unwrap();
    store::set(&mut conn,"one",vec![tag.clone(),other.clone()],vec![first.clone(),second.clone()]).unwrap();
    let change=Changes{game_ids:vec!["one".into(),"two".into()],add_tag_ids:vec![tag.clone()],remove_collection_ids:vec![first.clone()],..Default::default()};
    store::batch(&mut conn,&change).unwrap();store::batch(&mut conn,&change).unwrap();
    db::upsert_game(&conn,&Game{id:"one".into(),source:"local".into(),title:"新的资料".into(),metadata_json:r#"{"familySharing":{"available":false}}"#.into(),..Default::default()}).unwrap();
    let read=store::read(&conn).unwrap();assert_eq!(read.game_tags.len(),3);
    assert!(read.game_tags.contains(&("one".into(),other)));
    assert_eq!(read.game_collections,vec![("one".into(),second.clone())]);
    store::remove(&mut conn,true,&second).unwrap();
    assert!(store::read(&conn).unwrap().game_collections.is_empty());assert_eq!(db::games(&conn).unwrap().len(),2);
    assert!(store::remove(&mut conn,true,&second).is_err());
}
#[test]
fn invalid_targets_and_mid_transaction_failures_preserve_every_membership() {
    let mut conn=fixture();let tag=store::create(&mut conn,false,"标签").unwrap();store::set(&mut conn,"one",vec![tag.clone()],vec![]).unwrap();
    let original=store::read(&conn).unwrap().game_tags;
    let invalid=Changes{game_ids:vec!["one".into(),"missing".into()],remove_tag_ids:vec![tag.clone()],..Default::default()};
    assert!(store::batch(&mut conn,&invalid).is_err());
    assert!(store::set(&mut conn,"one",vec!["missing".into()],vec![]).is_err());
    assert_eq!(store::read(&conn).unwrap().game_tags,original);
    conn.execute_batch("CREATE TRIGGER reject_second BEFORE INSERT ON game_tags WHEN NEW.game_id='two' BEGIN SELECT RAISE(ABORT,'fixture'); END;").unwrap();
    let new=store::create(&mut conn,false,"新增").unwrap();
    assert!(store::batch(&mut conn,&Changes{game_ids:vec!["one".into(),"two".into()],add_tag_ids:vec![new],..Default::default()}).is_err());
    assert_eq!(store::read(&conn).unwrap().game_tags,original);
}
#[test]
fn reorder_requires_all_current_collections_and_delete_only_removes_relationships() {
    let mut conn=fixture();let a=store::create(&mut conn,true,"A").unwrap();let b=store::create(&mut conn,true,"B").unwrap();
    assert!(store::reorder(&mut conn,&[a.clone()]).is_err());assert!(store::reorder(&mut conn,&[a.clone(),a.clone()]).is_err());
    store::reorder(&mut conn,&[b.clone(),a.clone()]).unwrap();let read=store::read(&conn).unwrap();
    assert_eq!(read.collections.iter().map(|item|item.id.clone()).collect::<Vec<_>>(),vec![b,a.clone()]);
    let tag=store::create(&mut conn,false,"待玩").unwrap();store::set(&mut conn,"one",vec![tag.clone()],vec![a.clone()]).unwrap();
    store::remove(&mut conn,false,&tag).unwrap();assert!(store::read(&conn).unwrap().game_tags.is_empty());
    assert_eq!(store::read(&conn).unwrap().game_collections.len(),1);
    let tx=conn.transaction().unwrap();store::clear_game(&tx,"one").unwrap();tx.execute("DELETE FROM games WHERE id='one'",[]).unwrap();tx.commit().unwrap();
    assert!(store::read(&conn).unwrap().game_collections.is_empty());assert_eq!(store::read(&conn).unwrap().collections.len(),2);
}
