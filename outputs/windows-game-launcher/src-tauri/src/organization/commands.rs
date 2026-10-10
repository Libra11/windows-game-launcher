use super::{store, Changes, Organization};
use crate::{lock_db, AppState};
use tauri::Emitter;

fn changed(app:&tauri::AppHandle)->Result<(),String> {
    let _=app.emit("library-organization-changed",());Ok(())
}
#[tauri::command]
pub(crate) fn get_library_organization(state:tauri::State<'_,AppState>)->Result<Organization,String> {
    { let conn=lock_db(&state)?;store::read(&conn) }
}
#[tauri::command]
pub(crate) fn create_library_tag(app:tauri::AppHandle,state:tauri::State<'_,AppState>,name:String)->Result<String,String> {
    let mut conn=lock_db(&state)?;let id=store::create(&mut conn,false,&name)?;changed(&app)?;Ok(id)
}
#[tauri::command]
pub(crate) fn rename_library_tag(app:tauri::AppHandle,state:tauri::State<'_,AppState>,id:String,name:String)->Result<(),String> {
    let mut conn=lock_db(&state)?;store::rename(&mut conn,false,&id,&name)?;changed(&app)
}
#[tauri::command]
pub(crate) fn delete_library_tag(app:tauri::AppHandle,state:tauri::State<'_,AppState>,id:String)->Result<(),String> {
    let mut conn=lock_db(&state)?;store::remove(&mut conn,false,&id)?;changed(&app)
}
#[tauri::command]
pub(crate) fn create_library_collection(app:tauri::AppHandle,state:tauri::State<'_,AppState>,name:String)->Result<String,String> {
    let mut conn=lock_db(&state)?;let id=store::create(&mut conn,true,&name)?;changed(&app)?;Ok(id)
}
#[tauri::command]
pub(crate) fn rename_library_collection(app:tauri::AppHandle,state:tauri::State<'_,AppState>,id:String,name:String)->Result<(),String> {
    let mut conn=lock_db(&state)?;store::rename(&mut conn,true,&id,&name)?;changed(&app)
}
#[tauri::command]
pub(crate) fn delete_library_collection(app:tauri::AppHandle,state:tauri::State<'_,AppState>,id:String)->Result<(),String> {
    let mut conn=lock_db(&state)?;store::remove(&mut conn,true,&id)?;changed(&app)
}
#[tauri::command]
pub(crate) fn reorder_library_collections(app:tauri::AppHandle,state:tauri::State<'_,AppState>,ids:Vec<String>)->Result<(),String> {
    let mut conn=lock_db(&state)?;store::reorder(&mut conn,&ids)?;changed(&app)
}
#[tauri::command]
pub(crate) fn set_game_organization(app:tauri::AppHandle,state:tauri::State<'_,AppState>,game_id:String,tag_ids:Vec<String>,collection_ids:Vec<String>)->Result<(),String> {
    let mut conn=lock_db(&state)?;store::set(&mut conn,&game_id,tag_ids,collection_ids)?;changed(&app)
}
#[tauri::command]
pub(crate) fn batch_update_game_organization(app:tauri::AppHandle,state:tauri::State<'_,AppState>,changes:Changes)->Result<(),String> {
    let mut conn=lock_db(&state)?;store::batch(&mut conn,&changes)?;changed(&app)
}
