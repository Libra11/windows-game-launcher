mod cloud;
mod commands;
mod remote;
mod restore;
mod store;
pub use commands::*;
pub use remote::*;
use std::path::{Path, PathBuf};
use tauri::Manager;
#[derive(Default)]
pub struct ManagerState { gate: tokio::sync::Mutex<()> }
pub fn startup(data: &Path) -> Result<(),String> {
    let dir = data.join("saves");
    let conn = store::open(&dir)?;
    restore::recover(&conn,&dir)
}
fn directory(app: &tauri::AppHandle) -> Result<PathBuf,String> {
    Ok(app.path().app_data_dir().map_err(|e|e.to_string())?.join("saves"))
}
fn cloud_game(app: &tauri::AppHandle, local: &str) -> Result<String,String> {
    let state = app.state::<crate::AppState>();
    let conn = crate::lock_db(&state)?;
    conn.query_row("SELECT cloud_id FROM sync_game_links WHERE local_id=?1",[local],|r|r.get(0))
        .map_err(|_|"请先完成云账号资料库导入，再关联云存档".into())
}
