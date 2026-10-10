use super::{cache,model::{Attempt,Identity},ManagerState};
use crate::{lock_db,AppState};
use std::{sync::atomic::Ordering,time::Duration};
use tauri::Manager;
use tauri_plugin_updater::{Update,UpdaterExt};

#[derive(Clone)]
pub(super) struct Prepared {pub id:String,pub identity:Identity,pub update:Update}
pub(super) fn support()->(bool,String) {
    if cfg!(debug_assertions)||!cfg!(target_arch="x86_64"){return (false,"应用内安装仅支持 Windows x64 NSIS 正式安装版。".into());}
    let Ok(exe)=std::env::current_exe().and_then(std::fs::canonicalize) else {return (false,"无法确认应用安装位置。".into());};
    use winreg::{RegKey,enums::{HKEY_CURRENT_USER,HKEY_LOCAL_MACHINE,KEY_READ,KEY_WOW64_64KEY}};
    for hive in [HKEY_CURRENT_USER,HKEY_LOCAL_MACHINE] {
        if let Ok(key)=RegKey::predef(hive).open_subkey_with_flags(r"Software\Microsoft\Windows\CurrentVersion\Uninstall\游迹",KEY_READ|KEY_WOW64_64KEY){
            let location=key.get_value::<String,_>("InstallLocation").unwrap_or_default();
            let binary=key.get_value::<String,_>("MainBinaryName").unwrap_or_default();
            let uninstall=key.get_value::<String,_>("UninstallString").unwrap_or_default();
            let path=std::path::Path::new(location.trim_matches('"')).join(&binary);
            if !binary.is_empty()&&uninstall.to_lowercase().ends_with("uninstall.exe\"")&&std::fs::canonicalize(path).ok().as_ref()==Some(&exe){return (true,String::new());}
        }
    }
    (false,"当前应用未匹配 NSIS 安装记录，请通过正式安装包安装后再使用应用内更新。".into())
}
fn configured(app:&tauri::AppHandle)->Result<tauri_plugin_updater::Updater,String> {
    let settings={let state=app.state::<AppState>();let conn=lock_db(&state)?;crate::network::read(&conn)?};
    let mut builder=app.updater_builder().target("windows-x86_64")
        .configure_client(|client|client.connect_timeout(Duration::from_secs(10)).read_timeout(Duration::from_secs(30)).redirect(updater_http::redirect::Policy::custom(|attempt|{
            if attempt.url().scheme()!="https"||attempt.previous().len()>=5 {attempt.error("更新连接拒绝不安全或过多重定向")}else{attempt.follow()}
        })))
        .timeout(Duration::from_secs(20)).version_comparator(|current,release|release.version.pre.is_empty()&&release.version>current);
    match settings.mode {
        crate::network::ProxyMode::System=>{},
        crate::network::ProxyMode::Direct=>builder=builder.no_proxy(),
        crate::network::ProxyMode::Custom=>builder=builder.proxy(settings.address.parse().map_err(|_|"更新代理地址无效")?),
    }
    builder.build().map_err(|e|e.to_string())
}
pub(super) async fn check(app:&tauri::AppHandle,manager:&ManagerState)->Result<(),String> {
    *manager.prepared.lock().map_err(|_|"更新准备状态不可用")?=None;
    manager.change(app,|s|{s.preparation_id.clear();s.download_ready=false;s.version.clear();s.notes.clear();s.published_at.clear();s.downloaded=0;s.total=None;})?;
    match configured(app)?.check().await.map_err(|e|e.to_string())? {
        None=>manager.change(app,|s|s.phase="current".into()),
        Some(update)=>{
            let identity=Identity{version:update.version.clone(),url:update.download_url.to_string(),signature:update.signature.clone()};
            identity.validate(&manager.snapshot()?.current_version)?;
            if update.body.as_ref().is_some_and(|s|s.len()>256*1024){return Err("更新说明过长".into());}
            let id=uuid::Uuid::new_v4().to_string();
            let cached=cache::metadata(&manager.cache_root).ok().flatten();
            let reusable=cached.as_ref().filter(|c|c.identity==identity).is_some_and(|c|cache::load(&manager.cache_root,c,&identity,&manager.public_key).is_ok());
            let publish=update.raw_json.get("pub_date").and_then(serde_json::Value::as_str).unwrap_or("").to_string();
            manager.change(app,|s|{
                s.preparation_id=id.clone();s.version=identity.version.clone();s.notes=update.body.clone().unwrap_or_default();s.published_at=publish;
                s.download_ready=reusable;
                s.phase=if reusable{"ready"}else{"available"}.into();
                if reusable {s.downloaded=cached.as_ref().unwrap().size;s.total=Some(s.downloaded);}
            })?;
            *manager.prepared.lock().map_err(|_|"更新准备状态不可用")?=Some(Prepared{id,identity,update});Ok(())
        }
    }
}
fn prepared(manager:&ManagerState,id:&str)->Result<Prepared,String> {
    manager.prepared.lock().map_err(|_|"更新准备状态不可用")?.as_ref().filter(|p|p.id==id).cloned().ok_or("更新准备已失效，请重新检查".into())
}
fn download_proxy(app:&tauri::AppHandle,update:&mut Update)->Result<(),String> {
    let settings={let state=app.state::<AppState>();let conn=lock_db(&state)?;crate::network::read(&conn)?};
    update.timeout=Some(Duration::from_secs(30*60));update.proxy=None;update.no_proxy=false;
    match settings.mode {crate::network::ProxyMode::System=>{},crate::network::ProxyMode::Direct=>update.no_proxy=true,crate::network::ProxyMode::Custom=>update.proxy=Some(settings.address.parse().map_err(|_|"更新代理地址无效")?)}Ok(())
}
pub(super) async fn download(app:&tauri::AppHandle,manager:&ManagerState,id:&str)->Result<(),String> {
    let mut prepared=prepared(manager,id)?;download_proxy(app,&mut prepared.update)?;
    manager.change(app,|s|{s.downloaded=0;s.total=None;s.download_ready=false;})?;
    let downloaded=std::sync::atomic::AtomicU64::new(0);
    let mut last_event=std::time::Instant::now()-Duration::from_millis(100);
    let future=prepared.update.download(|size,total|{
        let count=downloaded.fetch_add(size as u64,Ordering::Relaxed)+size as u64;
        let too_large=count>512*1024*1024||total.is_some_and(|n|n>512*1024*1024);
        if last_event.elapsed()>=Duration::from_millis(100)||too_large{last_event=std::time::Instant::now();let _=manager.change(app,|s|{s.downloaded=count;s.total=total;});}
        if too_large{manager.cancel.store(true,Ordering::Release);manager.notify.notify_one();}
    },||{let _=manager.change(app,|s|{s.phase="verifying".into();s.downloaded=downloaded.load(Ordering::Relaxed);});});
    let cancellation=async {loop{let notified=manager.notify.notified();if manager.cancel.load(Ordering::Acquire){break;}notified.await;}};
    let bytes=tokio::select!{result=future=>result.map_err(|e|e.to_string())?,_=cancellation=>{
        manager.change(app,|s|{s.phase="available".into();s.error=if s.downloaded>512*1024*1024||s.total.is_some_and(|n|n>512*1024*1024){"更新包超过 512 MiB 限制".into()}else{String::new()};s.downloaded=0;s.total=None;})?;return Ok(());
    }};
    if manager.cancel.load(Ordering::Acquire){manager.change(app,|s|s.phase="available".into())?;return Ok(());}
    let cached=cache::save(&manager.cache_root,prepared.identity,&bytes,&manager.public_key)?;
    manager.change(app,|s|{s.phase="ready".into();s.download_ready=true;s.cache_version=cached.identity.version;s.downloaded=cached.size;s.total=Some(cached.size);})
}
pub(super) fn install(app:&tauri::AppHandle,manager:&ManagerState,id:&str)->Result<(),String> {
    if !support().0{return Err("当前应用未匹配正式安装记录".into());}
    let prepared=prepared(manager,id)?;prepared.identity.validate(&manager.snapshot()?.current_version)?;
    let cached=cache::metadata(&manager.cache_root)?.ok_or("请先下载更新")?;
    let bytes=cache::load(&manager.cache_root,&cached,&prepared.identity,&manager.public_key)?;
    let state=app.state::<AppState>();let backup=app.state::<crate::backup::Manager>();
    let _backup=backup.prevent_operations()?;
    let _gate=state.operation_gate.lock().map_err(|_|"启动操作状态不可用")?;
    if app.path().app_data_dir().map_err(|e|e.to_string())?.join("pending-backup-restore.json").exists(){return Err("有待完成的恢复任务，请先完成恢复".into());}
    if state.maintenance.load(Ordering::Acquire){return Err("游迹正在维护，请稍后安装".into());}
    crate::backup::require_games_idle(app)?;
    crate::runtime_persistence::checkpoint(app,true)?;
    let _maintenance=crate::maintenance::Guard::enter(&state.maintenance)?;
    let result=(||{
        // 后台会在无游戏时保留等待组件。先冻结新捕获，再正常停止组件，
        // 不能将等待进程直接当作游戏仍在运行，也不能绕过解除附加。
        crate::backup::require_games_idle(app)?;
        crate::xbox_local::stop_for_update()?;
        crate::backup::require_idle(app)?;
        // 等待此前持有数据库锁的写入完成；Windows 插件成功启动安装器后直接退出进程。
        {let _conn=state.db.lock().map_err(|_|"数据库写入尚未完成")?;}
        let attempt=Attempt{from:manager.snapshot()?.current_version,to:prepared.identity.version.clone(),started_at:chrono::Utc::now().to_rfc3339()};
        cache::atomic_json(&manager.cache_root.join("install-attempt.json"),&attempt)?;
        manager.change(app,|s|s.phase="installing".into())?;
        prepared.update.install(bytes).map_err(|e|e.to_string())
    })();
    if result.is_err(){let _=std::fs::remove_file(manager.cache_root.join("install-attempt.json"));}
    result
}
