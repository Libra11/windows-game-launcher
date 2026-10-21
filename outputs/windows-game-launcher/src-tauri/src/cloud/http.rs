use serde_json::{json,Value};
use tauri::Manager;
use youji_cloud_protocol::SessionTokens;
use super::{Cloud,store::err};

pub fn base(app:&tauri::AppHandle)->Result<String,String>{
    let state=app.state::<crate::AppState>();let c=crate::lock_db(&state)?;
    let base=crate::db::setting(&c,"cloud_base_url")?;
    validate_base(&base)?;Ok(base)
}
pub fn validate_base(value:&str)->Result<(),String>{
    let u=reqwest::Url::parse(value).map_err(|_|"请填写有效的 HTTPS 服务地址")?;
    if u.scheme()!="https" || u.host_str().is_none() || !u.username().is_empty() || u.password().is_some() || u.query().is_some() || u.fragment().is_some(){return Err("云服务地址必须是无凭据的 HTTPS 地址".into());}
    Ok(())
}
pub async fn public(app:&tauri::AppHandle,path:&str,body:Value)->Result<Value,String>{
    let response=crate::network::client(crate::network::Service::Public)?.post(format!("{}{}",base(app)?,path)).json(&body).send().await.map_err(|_|"无法连接云服务")?;
    decode(response).await
}
async fn decode(mut response:reqwest::Response)->Result<Value,String>{
    let status=response.status();
    let mut data=Vec::new();
    while let Some(chunk)=response.chunk().await.map_err(|_|"读取云端响应失败")?{
        if data.len()+chunk.len()>16*1024*1024{return Err("云端响应超过限制".into());}
        data.extend_from_slice(&chunk);
    }
    let value:Value=serde_json::from_slice(&data).map_err(|_|"云端响应格式错误")?;
    if !status.is_success(){
        return Err(value["error"].as_str().filter(|s|s.len()<=1000&&!s.chars().any(char::is_control)).map(str::to_owned).unwrap_or_else(||format!("云请求失败（HTTP {}）",status.as_u16())));
    }
    Ok(value)
}
pub async fn install(app:&tauri::AppHandle,tokens:SessionTokens)->Result<(),String>{
    let base=base(app)?;
    {
        let state=app.state::<crate::AppState>();let c=crate::lock_db(&state)?;
        let bound:Option<String>=rusqlite::OptionalExtension::optional(c.query_row("SELECT user_id FROM account_binding",[],|r|r.get(0))).map_err(err)?;
        if bound.is_some_and(|id|id!=tokens.user_id.to_string()){return Err("此资料库已绑定其他游迹账号，不能换绑".into());}
    }
    super::credentials::write(&base,&tokens.user_id.to_string(),&tokens.refresh_token)?;
    {let state=app.state::<crate::AppState>();let c=crate::lock_db(&state)?;
    crate::db::set_setting(&c,"cloud_login_user",&tokens.user_id.to_string())?;
    c.execute("UPDATE account_binding SET device_id=?1 WHERE user_id=?2",rusqlite::params![tokens.device_id.to_string(),tokens.user_id.to_string()]).map_err(err)?;}
    *app.state::<Cloud>().session.lock().await=Some(tokens);
    *app.state::<Cloud>().last_error.lock().await=None;Ok(())
}
pub async fn request(app:&tauri::AppHandle,method:&str,path:&str,body:Option<Value>)->Result<Value,String>{
    if !path.starts_with("/v1/") || path.contains("://"){return Err("无效的云接口路径".into());}
    let cloud=app.state::<Cloud>();let _guard=cloud.http_gate.lock().await;
    let base=base(app)?;
    for attempt in 0..2 {
        if cloud.session.lock().await.is_none() {
            let user={let state=app.state::<crate::AppState>();let c=crate::lock_db(&state)?;crate::db::setting(&c,"cloud_login_user")?};
            if user.is_empty(){return Err("请先登录云账号".into());}
            let refresh=super::credentials::read(&base,&user)?;
            let response=match public(app,"/v1/auth/refresh",json!({"refreshToken":refresh})).await{
                Ok(value)=>value,
                Err(_)=>{
                    // 响应丢失时不能重放可能已经轮换的 refresh token。
                    let state=app.state::<crate::AppState>();let c=crate::lock_db(&state)?;
                    crate::db::set_setting(&c,"cloud_login_user","")?;
                    return Err("刷新登录未完成，请重新验证邮箱以恢复会话".into());
                }
            };
            install(app,serde_json::from_value(response).map_err(err)?).await?;
        }
        let token=cloud.session.lock().await.as_ref().ok_or("请重新登录")?.access_token.clone();
        let mut req=crate::network::client(crate::network::Service::Public)?.request(method.parse::<reqwest::Method>().map_err(err)?,format!("{base}{path}")).bearer_auth(token);
        if let Some(b)=&body {req=req.json(b);}
        let response=req.send().await.map_err(|_|"无法连接云服务，变更已保留在本机")?;
        if response.status()==reqwest::StatusCode::UNAUTHORIZED && attempt==0{*cloud.session.lock().await=None;continue;}
        return decode(response).await;
    } Err("登录已失效，请重新登录".into())
}
