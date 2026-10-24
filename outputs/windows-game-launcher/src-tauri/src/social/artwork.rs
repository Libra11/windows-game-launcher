//! Independent, resumable artwork publication. Only cache-resolved image bytes leave this device.
use super::*;
use base64::{engine::general_purpose::STANDARD, Engine};
use sha2::{Digest, Sha256};
pub(super) fn start(app:tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(15)).await;
            if app.state::<Social>().stopping.load(Ordering::Acquire){break;}
            let Ok(account)=identity(&app) else {continue};
            let _=publish(&app,&account).await;
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        }
    });
}
async fn publish(app:&tauri::AppHandle,account:&str)->Result<(),String> {
    let manifest=crate::cloud::request(app,"GET","/v1/social/covers",None).await?;
    let candidates={let state=app.state::<AppState>();let c=lock_db(&state)?;
        db::games(&c)?.into_iter().filter(|g|c.query_row("SELECT EXISTS(SELECT 1 FROM sync_game_links WHERE local_id=?1)",[&g.id],|r|r.get::<_,bool>(0)).unwrap_or(false)).map(|g|(game_key(&c,&g),g.id)).collect::<Vec<_>>()};
    for (key,id) in candidates {
        if !manifest["gameKeys"].as_array().is_some_and(|keys|keys.iter().any(|v|v==&key)){continue;}
        if app.state::<Social>().stopping.load(Ordering::Acquire)||identity(app).ok().as_deref()!=Some(account){return Ok(());}
        let Ok(Some(path))=crate::cover_cache::resolve(app,&id,false,false).await else {continue};
        let Ok(meta)=tokio::fs::metadata(&path).await else {continue};
        if meta.len()>8*1024*1024 {continue;}
        let Ok(bytes)=tokio::fs::read(path).await else {continue};
        let hash=format!("{:x}",Sha256::digest(&bytes));
        if manifest["covers"].as_array().is_some_and(|a|a.iter().any(|v|v["key"]==key&&v["hash"]==hash)){continue;}
        if identity(app).ok().as_deref()!=Some(account){return Ok(());}
        crate::cloud::request(app,"POST","/v1/social/covers",Some(json!({"key":key,"data":STANDARD.encode(bytes)}))).await?;
        tokio::time::sleep(std::time::Duration::from_millis(600)).await;
    }
    Ok(())
}
