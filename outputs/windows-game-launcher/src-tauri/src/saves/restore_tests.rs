use super::*;
fn fixture() -> (tempfile::TempDir, Connection, PathBuf, store::Binding, store::Snapshot) {
    let temp=tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap();
    let data=temp.path().join("data");let root=temp.path().join("game");let new=temp.path().join("new");
    fs::create_dir(&root).unwrap();fs::create_dir(&new).unwrap();
    fs::write(root.join("old.dat"),b"old").unwrap();fs::write(new.join("new.dat"),b"new").unwrap();
    let c=store::open(&data).unwrap();let archive=data.join("snapshot.zip");
    let m=youji_save_archive::create(&new,&archive).unwrap();
    let b=store::Binding{id:"profile".into(),game_id:"game".into(),name:"主存档".into(),root:root.to_str().unwrap().into(),cloud_id:None,parent_id:None,revision:0};
    store::put_binding(&c,&b).unwrap();
    let s=store::Snapshot{id:"snapshot".into(),profile_id:b.id.clone(),archive:archive.to_str().unwrap().into(),
        manifest_hash:youji_save_archive::manifest_hash(&m).unwrap(),archive_hash:youji_save_archive::file_hash(&archive).unwrap(),
        bytes:0,parent_ids:vec![],base_revision:0,created_at:"".into(),uploaded:false,upload_started:false,error:None};
    (temp,c,data,b,s)
}
#[test] fn restore_exact_directory_retains_original() {
    let (_temp,c,data,b,s)=fixture();let job=prepare(&c,&data,&b,&s).unwrap();let old=job.rollback.clone();let safety=job.safety.clone();
    apply(&c,&data,job).unwrap();
    assert!(Path::new(&b.root).join("new.dat").exists());assert!(!Path::new(&b.root).join("old.dat").exists());
    assert!(old.join("old.dat").exists());assert!(youji_save_archive::verify(&safety).is_ok());
    assert_eq!(store::binding(&c,&b.id).unwrap().parent_id,Some(s.id));
    recover(&c,&data).unwrap();
}
#[test] fn crash_after_original_rename_rolls_back() {
    let (_temp,c,data,b,s)=fixture();let job=prepare(&c,&data,&b,&s).unwrap();
    fs::rename(&job.root,&job.rollback).unwrap();recover(&c,&data).unwrap();
    assert_eq!(fs::read(Path::new(&b.root).join("old.dat")).unwrap(),b"old");
}
#[test] fn crash_after_install_finishes_and_updates_parent() {
    let (_temp,c,data,b,s)=fixture();let job=prepare(&c,&data,&b,&s).unwrap();
    fs::rename(&job.root,&job.rollback).unwrap();fs::rename(&job.staged,&job.root).unwrap();
    recover(&c,&data).unwrap();assert_eq!(store::binding(&c,&b.id).unwrap().parent_id,Some(s.id));
}
#[test] fn corrupted_installed_directory_restores_original() {
    let (_temp,c,data,b,s)=fixture();let job=prepare(&c,&data,&b,&s).unwrap();
    fs::rename(&job.root,&job.rollback).unwrap();fs::rename(&job.staged,&job.root).unwrap();
    fs::write(job.root.join("new.dat"),b"broken").unwrap();
    recover(&c,&data).unwrap();assert!(job.root.join("old.dat").exists());
}
#[test] fn safety_failure_does_not_touch_source() {
    let (_temp,c,data,b,s)=fixture();
    let unavailable=data.join("missing");
    assert!(prepare(&c,&unavailable,&b,&s).is_err());
    assert!(Path::new(&b.root).join("old.dat").exists());
}
#[test] fn crash_before_first_rename_keeps_original() {
    let (_temp,c,data,b,s)=fixture();let _job=prepare(&c,&data,&b,&s).unwrap();
    recover(&c,&data).unwrap();
    assert_eq!(fs::read(Path::new(&b.root).join("old.dat")).unwrap(),b"old");
    assert_eq!(store::binding(&c,&b.id).unwrap().parent_id,None);
}
#[test] fn unknown_directory_state_blocks_recovery() {
    let (_temp,c,data,b,s)=fixture();let job=prepare(&c,&data,&b,&s).unwrap();
    fs::rename(&job.root,data.join("externally-moved")).unwrap();
    assert!(recover(&c,&data).is_err());
}
