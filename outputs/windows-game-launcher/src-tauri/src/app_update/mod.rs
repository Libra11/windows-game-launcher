#![cfg_attr(not(windows),allow(dead_code))]
mod cache;
mod commands;
mod model;
#[cfg(windows)] mod windows;
#[cfg(test)] mod tests;
pub(crate) use commands::*;
use model::Status;
use std::{path::PathBuf,sync::{Mutex,atomic::{AtomicBool,Ordering}}};
use tauri::{Emitter,Manager};

pub struct ManagerState {
    status:Mutex<Status>,busy:AtomicBool,cancel:AtomicBool,notify:tokio::sync::Notify,completed:tokio::sync::Notify,
    cache_root:PathBuf,public_key:String,
    #[cfg(windows)] prepared:Mutex<Option<windows::Prepared>>,
}
struct Operation<'a>(&'a ManagerState);
impl Drop for Operation<'_>{fn drop(&mut self){self.0.busy.store(false,Ordering::Release);self.0.completed.notify_waiters();}}
impl ManagerState {
    fn begin(&self,app:&tauri::AppHandle,phase:&str)->Result<Operation<'_>,String> {
        self.busy.compare_exchange(false,true,Ordering::AcqRel,Ordering::Acquire).map_err(|_|"另一项更新操作正在进行")?;
        let operation=Operation(self);self.cancel.store(false,Ordering::Release);
        self.change(app,|s|{s.phase=phase.into();s.operation_id=uuid::Uuid::new_v4().to_string();s.error.clear();})?;
        Ok(operation)
    }
    fn change(&self,app:&tauri::AppHandle,change:impl FnOnce(&mut Status))->Result<(),String> {
        let mut status=self.status.lock().map_err(|_|"更新状态不可用")?;change(&mut status);status.revision=status.revision.saturating_add(1);let next=status.clone();drop(status);
        if let Some(window)=app.get_webview_window("main"){let _=window.emit("app-update-progress",next);}Ok(())
    }
    fn snapshot(&self)->Result<Status,String>{self.status.lock().map(|s|s.clone()).map_err(|_|"更新状态不可用".into())}
    fn fail(&self,app:&tauri::AppHandle,error:&str){let _=self.change(app,|s|{s.phase="error".into();s.error=error.into();});}
}
pub(crate) fn initialize(app:&tauri::AppHandle,conn:&rusqlite::Connection)->Result<ManagerState,String> {
    let cache_root=app.path().app_cache_dir().map_err(|e|e.to_string())?.join("app-updates");
    let public_key=app.config().plugins.0.get("updater").and_then(|v|v.get("pubkey")).and_then(serde_json::Value::as_str).unwrap_or("").to_string();
    let (supported,reason)=support();
    let mut status=Status{supported,reason,current_version:app.package_info().version.to_string(),current_notes:include_str!(concat!("../../../release-notes/",env!("CARGO_PKG_VERSION"),".md")).into(),auto_check:crate::db::setting(conn,"check_updates_on_startup")?!="false",phase:"idle".into(),..Default::default()};
    match cache::metadata(&cache_root){Ok(Some(cached))=>{let newer=semver::Version::parse(&cached.identity.version).ok().zip(semver::Version::parse(&status.current_version).ok()).is_some_and(|(cached,current)|cached>current);
        if newer{status.cache_version=cached.identity.version;if supported{status.phase="cached".into();}}},Err(e)=>status.error=e,Ok(None)=>{}}
    match cache::attempt(&cache_root){Ok(Some(attempt))=>{
        status.install_message=model::attempt_message(&attempt,&status.current_version);
        if status.current_version==attempt.to {let _=std::fs::remove_file(cache_root.join("install-attempt.json"));}
    },Err(e)=>status.install_message=e,Ok(None)=>{}}
    Ok(ManagerState{status:Mutex::new(status),busy:AtomicBool::new(false),cancel:AtomicBool::new(false),notify:tokio::sync::Notify::new(),completed:tokio::sync::Notify::new(),cache_root,public_key,#[cfg(windows)]prepared:Mutex::new(None)})
}
#[cfg(windows)] fn support()->(bool,String){windows::support()}
#[cfg(not(windows))] fn support()->(bool,String){(false,"应用内安装仅支持 Windows x64 NSIS 正式安装版；当前为开发或其他平台版本。".into())}

// 每次原生进程启动仅安排一次，不随 WebView 热刷新或设置页切换重复检查。
pub(crate) fn start(app:tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(8)).await;
        if commands::get_app_update_status(app.clone()).is_ok_and(|s|s.supported&&s.auto_check){let _=commands::check_app_update(app).await;}
    });
}
