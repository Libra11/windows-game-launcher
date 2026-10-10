use super::{cache,model::{Identity,Attempt,attempt_message},ManagerState};
use base64::Engine;
fn fixture()->(String,String,Vec<u8>){
    let value:serde_json::Value=serde_json::from_str(include_str!("fixtures/signed.json")).unwrap();
    (value["publicKey"].as_str().unwrap().into(),value["signature"].as_str().unwrap().into(),base64::engine::general_purpose::STANDARD.decode(value["payload"].as_str().unwrap()).unwrap())
}
fn identity(signature:String)->Identity{Identity{version:"1.1.0".into(),url:"https://github.com/Libra11/windows-game-launcher/releases/download/v1.1.0/youji_1.1.0_x64-setup.exe".into(),signature}}
#[test]fn stable_versions_and_owned_release_urls_are_required(){
    let (_,signature,_)=fixture();let valid=identity(signature);valid.validate("1.0.0").unwrap();assert!(valid.validate("1.1.0").is_err());
    for version in ["1.2.0-beta.1","garbage","0.9.0"]{let mut changed=valid.clone();changed.version=version.into();assert!(changed.validate("1.0.0").is_err());}
    for url in ["http://github.com/Libra11/windows-game-launcher/releases/download/v1.1.0/youji_1.1.0_x64-setup.exe","https://other.example/update.exe","https://github.com/Other/repo/releases/download/v1.1.0/youji_1.1.0_x64-setup.exe"]{let mut changed=valid.clone();changed.url=url.into();assert!(changed.validate("1.0.0").is_err());}
}
#[test]fn cache_reuse_verifies_signature_size_digest_version_and_identity(){
    let temp=tempfile::tempdir().unwrap();let (public,signature,bytes)=fixture();let expected=identity(signature);
    let cached=cache::save(temp.path(),expected.clone(),&bytes,&public).unwrap();
    assert_eq!(cache::load(temp.path(),&cached,&expected,&public).unwrap(),bytes);
    assert_eq!(cache::metadata(temp.path()).unwrap().unwrap().identity,expected);
    let mut changed=expected.clone();changed.version="1.2.0".into();assert!(cache::load(temp.path(),&cached,&changed,&public).is_err());
    let mut tampered=bytes.clone();tampered[2]^=1;std::fs::write(temp.path().join(&cached.file),&tampered).unwrap();
    let mut forged=cached.clone();forged.sha256=cache::hash(&tampered);
    assert!(cache::load(temp.path(),&forged,&expected,&public).is_err());
    assert!(cache::verify(&bytes,&expected.signature,&public,"1.2.0").is_err());
}
#[test]fn cache_cannot_escape_and_failed_validation_does_not_replace_previous_download(){
    let temp=tempfile::tempdir().unwrap();let (public,signature,bytes)=fixture();let valid=identity(signature);
    let original=cache::save(temp.path(),valid.clone(),&bytes,&public).unwrap();
    assert!(!cache::safe_file("../outside.exe"));assert!(!cache::safe_file("C:/outside.exe"));assert!(!cache::safe_file("arbitrary.exe"));
    assert!(cache::save(temp.path(),valid.clone(),b"tampered",&public).is_err());
    assert_eq!(cache::metadata(temp.path()).unwrap().unwrap().file,original.file);
    let mut link=original.clone();link.file="../outside.exe".into();assert!(cache::load(temp.path(),&link,&valid,&public).is_err());
}
#[test]fn cache_write_failure_does_not_publish_a_download_index(){
    let temp=tempfile::tempdir().unwrap();let (public,signature,bytes)=fixture();let blocked=temp.path().join("not-a-directory");std::fs::write(&blocked,b"occupied").unwrap();
    assert!(cache::save(&blocked,identity(signature),&bytes,&public).is_err());assert!(!blocked.join("download.json").exists());
}
#[test]fn install_success_is_confirmed_from_actual_running_version(){
    let attempt=Attempt{from:"1.0.0".into(),to:"1.1.0".into(),started_at:"2026-10-10T08:00:00Z".into()};
    assert_eq!(attempt_message(&attempt,"1.1.0"),"已更新至 1.1.0");assert!(attempt_message(&attempt,"1.0.0").contains("尚未确认完成"));
}
#[test]fn dropping_an_operation_releases_single_operation_ownership(){
    use std::sync::{Mutex,atomic::{AtomicBool,Ordering}};
    let manager=ManagerState{status:Mutex::new(Default::default()),busy:AtomicBool::new(true),cancel:AtomicBool::new(false),notify:Default::default(),completed:Default::default(),cache_root:Default::default(),public_key:String::new(),#[cfg(windows)]prepared:Mutex::new(None)};
    {let _operation=super::Operation(&manager);assert!(manager.busy.compare_exchange(false,true,Ordering::AcqRel,Ordering::Acquire).is_err());}
    assert!(!manager.busy.load(Ordering::Acquire));
}
#[cfg(unix)]
#[test]fn linked_download_is_rejected_without_reading_the_target(){
    let temp=tempfile::tempdir().unwrap();let (public,signature,bytes)=fixture();let expected=identity(signature);
    let mut cached=cache::save(temp.path(),expected.clone(),&bytes,&public).unwrap();
    let link=format!("{}.exe",uuid::Uuid::new_v4());std::os::unix::fs::symlink(temp.path().join(&cached.file),temp.path().join(&link)).unwrap();cached.file=link;
    assert!(cache::load(temp.path(),&cached,&expected,&public).is_err());
}
