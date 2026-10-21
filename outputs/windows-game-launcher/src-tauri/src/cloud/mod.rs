mod commands;
mod library;
pub use library::*;
mod identity;
pub(crate) mod secrets;
pub(crate) mod official;
pub(crate) mod hooks;
mod credentials;
pub(crate) mod export;
mod http;
mod projection;
pub(crate) mod schema;
mod store;
mod triggers;
mod worker;
pub use commands::*;
pub use http::request;

pub struct Cloud {
    session:tokio::sync::Mutex<Option<youji_cloud_protocol::SessionTokens>>,
    http_gate:tokio::sync::Mutex<()>,
    sync_gate:tokio::sync::Mutex<()>,
    import_preview:tokio::sync::Mutex<Option<serde_json::Value>>,
    last_error:tokio::sync::Mutex<Option<String>>,
}
impl Cloud {
    pub fn new()->Result<Self,String>{
        Ok(Self{session:Default::default(),http_gate:Default::default(),sync_gate:Default::default(),import_preview:Default::default(),last_error:Default::default()})
    }
}
pub use worker::start;

// 恢复只在现有恢复事务中清理同步运行状态，不复制设备或旧 outbox。
pub(crate) fn reset_for_restore(c:&rusqlite::Connection)->Result<(),String>{
    if !c.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='sync_state')",[],|r|r.get::<_,bool>(0)).map_err(store::err)?{return Ok(());}
    c.execute_batch("UPDATE sync_state SET applying=1;
      DELETE FROM account_binding; DELETE FROM sync_outbox; DELETE FROM sync_conflicts;
      DELETE FROM sync_shadow; DELETE FROM sync_game_links; DELETE FROM cloud_session_owners;
      DELETE FROM achievement_facts; DELETE FROM historical_playtime_baselines; DELETE FROM cloud_archives;
      UPDATE sync_state SET cursor=0,last_success='',applying=0;").map_err(store::err)
}
#[cfg(test)]
mod tests;
