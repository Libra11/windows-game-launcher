use super::{ManagerState,model::Status};
use tauri::Manager;
use std::sync::atomic::Ordering;
fn supported(manager:&ManagerState)->Result<(),String>{let status=manager.snapshot()?;if status.supported{Ok(())}else{Err(status.reason)}}
#[tauri::command]
pub(crate) fn get_app_update_status(app:tauri::AppHandle)->Result<Status,String> {
    let manager=app.state::<ManagerState>();let mut status=manager.snapshot()?;
    let state=app.state::<crate::AppState>();let conn=crate::lock_db(&state)?;status.auto_check=crate::db::setting(&conn,"check_updates_on_startup")?!="false";Ok(status)
}
#[tauri::command]
pub(crate) async fn check_app_update(app:tauri::AppHandle)->Result<Status,String> {
    let manager=app.state::<ManagerState>();supported(&manager)?;
    let completed=manager.completed.notified();tokio::pin!(completed);completed.as_mut().enable();
    if manager.busy.load(Ordering::Acquire)&&manager.snapshot()?.phase=="checking"{completed.await;return manager.snapshot();}
    let _operation=manager.begin(&app,"checking")?;
    #[cfg(windows)] let result=super::windows::check(&app,&manager).await;
    #[cfg(not(windows))] let result:Result<(),String>=Err("此平台不支持应用内安装".into());
    if let Err(error)=result{manager.fail(&app,&error);}manager.snapshot()
}
#[tauri::command]
pub(crate) async fn download_app_update(app:tauri::AppHandle,preparation_id:String)->Result<Status,String> {
    let manager=app.state::<ManagerState>();supported(&manager)?;
    let _operation=manager.begin(&app,"downloading")?;
    #[cfg(windows)] let result=super::windows::download(&app,&manager,&preparation_id).await;
    #[cfg(not(windows))] let result:Result<(),String>={let _=preparation_id;Err("此平台不支持应用内安装".into())};
    if let Err(error)=result{manager.fail(&app,&error);}manager.snapshot()
}
#[tauri::command]
pub(crate) fn cancel_app_update_download(app:tauri::AppHandle,operation_id:String)->Result<bool,String> {
    let manager=app.state::<ManagerState>();let status=manager.status.lock().map_err(|_|"更新状态不可用")?;
    if !manager.busy.load(Ordering::Acquire)||status.operation_id!=operation_id||status.phase!="downloading" {return Ok(false);}
    manager.cancel.store(true,Ordering::Release);manager.notify.notify_one();Ok(true)
}
#[tauri::command]
pub(crate) async fn install_app_update(app:tauri::AppHandle,preparation_id:String)->Result<Status,String> {
    tauri::async_runtime::spawn_blocking(move||{
        let manager=app.state::<ManagerState>();supported(&manager)?;let _operation=manager.begin(&app,"preparing")?;
        #[cfg(windows)] let result=super::windows::install(&app,&manager,&preparation_id);
        #[cfg(not(windows))] let result:Result<(),String>={let _=preparation_id;Err("此平台不支持应用内安装".into())};
        if let Err(error)=result{manager.fail(&app,&error);}manager.snapshot()
    }).await.map_err(|e|e.to_string())?
}
