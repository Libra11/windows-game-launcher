use crate::{App,ApiError,Result};
use axum::{extract::{FromRequestParts,State,Path},http::request::Parts,Json};
use chrono::{Duration,Utc};
use hmac::{Hmac,Mac};
use rand::Rng;
use serde::Deserialize;
use serde_json::{Value,json};
use sha2::Sha256;
use sqlx::Row;
use uuid::Uuid;
use youji_cloud_protocol::SessionTokens;

#[derive(Clone,Copy,Debug)]
pub struct Identity {pub user_id:Uuid,pub device_id:Uuid,pub session_id:Uuid}
impl FromRequestParts<App> for Identity {
    type Rejection=ApiError;
    async fn from_request_parts(parts:&mut Parts,app:&App)->Result<Self> {
        let token=parts.headers.get("authorization").and_then(|h|h.to_str().ok()).and_then(|h|h.strip_prefix("Bearer ")).filter(|v|v.len()==64).ok_or_else(ApiError::unauthorized)?;
        let row=sqlx::query("SELECT s.id,s.user_id,s.device_id FROM auth_sessions s JOIN devices d ON d.user_id=s.user_id AND d.id=s.device_id WHERE s.access_digest=$1 AND s.access_expires>now() AND NOT s.revoked AND NOT d.revoked")
            .bind(digest(app,token)).fetch_optional(&app.pool).await?.ok_or_else(ApiError::unauthorized)?;
        Ok(Self{user_id:row.get("user_id"),device_id:row.get("device_id"),session_id:row.get("id")})
    }
}
pub fn email(value:&str)->Result<String> {
    let value=value.trim().to_lowercase();
    if value.len()>254 || value.chars().any(|c|c.is_control()||c.is_whitespace()) || value.matches('@').count()!=1 || value.starts_with('@') || !value.split('@').nth(1).is_some_and(|x|x.contains('.')&&!x.ends_with('.')) {return Err(ApiError::bad("邮箱格式无效"));}
    Ok(value)
}
pub(crate) fn digest(app:&App,value:&str)->String {
    let mut mac=Hmac::<Sha256>::new_from_slice(app.config.token_key.as_bytes()).expect("HMAC 支持任意密钥长度");
    mac.update(value.as_bytes());hex::encode(mac.finalize().into_bytes())
}
fn token()->String {let bytes:[u8;32]=rand::random();hex::encode(bytes)}
#[derive(Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct ChallengeRequest {email:String}
pub async fn challenge(State(app):State<App>,Json(input):Json<ChallengeRequest>)->Result<axum::response::Response> {
    use axum::response::IntoResponse;
    let email=email(&input.email)?;let id=Uuid::new_v4();
    let code=format!("{:08}",rand::thread_rng().gen_range(0..100_000_000u32));
    let mut tx=app.pool.begin().await?;
    // 同一邮箱串行限流，不能通过并发请求绕过发送间隔。
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,17))").bind(&email).execute(&mut *tx).await?;
    let recent:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM login_challenges WHERE email=$1 AND created_at>now()-interval '60 seconds')").bind(&email).fetch_one(&mut *tx).await?;
    if recent {
        tx.commit().await?;
        return Ok((axum::http::StatusCode::TOO_MANY_REQUESTS,
            [(axum::http::header::RETRY_AFTER,"60")],
            Json(json!({"error":"请求过于频繁，请稍后重试"}))).into_response());
    }
    let invited:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM invites WHERE email=$1)").bind(&email).fetch_one(&mut *tx).await?;
    sqlx::query("INSERT INTO login_challenges(id,email,digest,expires_at) VALUES($1,$2,$3,$4)")
        .bind(id).bind(&email).bind(digest(&app,&format!("{id}:{code}"))).bind(Utc::now()+Duration::minutes(10)).execute(&mut *tx).await?;
    tx.commit().await?;
    if invited { tokio::spawn(async move {
        // 统一发送入口不把验证码、凭据或底层服务响应写入日志。
        let delivered=crate::mail::send(&app.config.mail,&app.http,&email,&code).await.is_ok();
        if !delivered {let _=sqlx::query("UPDATE login_challenges SET consumed=true WHERE id=$1").bind(id).execute(&app.pool).await;}
    }); }
    Ok(Json(json!({"challengeId":id})).into_response())
}
#[derive(Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct VerifyRequest {challenge_id:Uuid,code:String,device_name:String,device_id:Option<Uuid>}
pub async fn verify(State(app):State<App>,Json(input):Json<VerifyRequest>)->Result<Json<SessionTokens>> {
    if input.device_name.trim().is_empty() || input.device_name.len()>120 || input.device_name.chars().any(char::is_control) {return Err(ApiError::bad("设备名称无效"));}
    let mut tx=app.pool.begin().await?;
    let row=sqlx::query("SELECT email,digest,attempts,consumed,expires_at FROM login_challenges WHERE id=$1 FOR UPDATE").bind(input.challenge_id).fetch_optional(&mut *tx).await?.ok_or_else(ApiError::unauthorized)?;
    let email:String=row.get("email");
    let invited:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM invites WHERE email=$1)").bind(&email).fetch_one(&mut *tx).await?;
    let valid=!row.get::<bool,_>("consumed") && row.get::<i32,_>("attempts")<5 && row.get::<chrono::DateTime<Utc>,_>("expires_at")>Utc::now() && invited;
    sqlx::query("UPDATE login_challenges SET attempts=LEAST(attempts+1,6) WHERE id=$1").bind(input.challenge_id).execute(&mut *tx).await?;
    let expected: String=row.get("digest");
    let actual=digest(&app,&format!("{}:{}",input.challenge_id,input.code));
    let equal=expected.bytes().zip(actual.bytes()).fold(0u8,|diff,(a,b)|diff|(a^b))==0 && expected.len()==actual.len();
    if !valid || !equal || input.code.len()!=8 || !input.code.bytes().all(|c|c.is_ascii_digit()) {tx.commit().await?;return Err(ApiError::unauthorized());}
    sqlx::query("UPDATE login_challenges SET consumed=true WHERE id=$1").bind(input.challenge_id).execute(&mut *tx).await?;
    let user_id:Uuid=sqlx::query_scalar("INSERT INTO users(id,email) VALUES($1,$2) ON CONFLICT(email) DO UPDATE SET email=excluded.email RETURNING id").bind(Uuid::new_v4()).bind(email).fetch_one(&mut *tx).await?;
    let device_id=if let Some(id)=input.device_id {
        let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM devices WHERE user_id=$1 AND id=$2)").bind(user_id).bind(id).fetch_one(&mut *tx).await?;
        if !exists {return Err(ApiError::conflict("此资料库已绑定其他账号，不能换绑"));}
        sqlx::query("UPDATE devices SET name=$3,revoked=false WHERE user_id=$1 AND id=$2").bind(user_id).bind(id).bind(input.device_name.trim()).execute(&mut *tx).await?;
        id
    } else {
        let id=Uuid::new_v4();
        sqlx::query("INSERT INTO devices(user_id,id,name) VALUES($1,$2,$3)").bind(user_id).bind(id).bind(input.device_name.trim()).execute(&mut *tx).await?;id
    };
    sqlx::query("INSERT INTO user_sync_state(user_id) VALUES($1) ON CONFLICT DO NOTHING").bind(user_id).execute(&mut *tx).await?;
    let tokens=SessionTokens{user_id,device_id,access_token:token(),refresh_token:token()};
    sqlx::query("INSERT INTO auth_sessions(id,user_id,device_id,access_digest,access_expires,refresh_digest,refresh_expires) VALUES($1,$2,$3,$4,$5,$6,$7)")
        .bind(Uuid::new_v4()).bind(user_id).bind(device_id).bind(digest(&app,&tokens.access_token)).bind(Utc::now()+Duration::minutes(15)).bind(digest(&app,&tokens.refresh_token)).bind(Utc::now()+Duration::days(30)).execute(&mut *tx).await?;
    tx.commit().await?;Ok(Json(tokens))
}
#[derive(Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct RefreshRequest {refresh_token:String}
pub async fn refresh(State(app):State<App>,Json(input):Json<RefreshRequest>)->Result<Json<SessionTokens>> {
    if input.refresh_token.len()!=64 {return Err(ApiError::unauthorized());}
    let hash=digest(&app,&input.refresh_token);let mut tx=app.pool.begin().await?;
    // 先找到会话，再锁定；重读摘要识别并发刷新及旧 token 重放。
    let id:Option<Uuid>=sqlx::query_scalar("SELECT id FROM auth_sessions WHERE refresh_digest=$1 UNION SELECT session_id FROM used_refresh_tokens WHERE digest=$1 LIMIT 1").bind(&hash).fetch_optional(&mut *tx).await?;
    let id=id.ok_or_else(ApiError::unauthorized)?;
    let row=sqlx::query("SELECT s.*,d.revoked AS device_revoked FROM auth_sessions s JOIN devices d ON d.user_id=s.user_id AND d.id=s.device_id WHERE s.id=$1 FOR UPDATE OF s").bind(id).fetch_one(&mut *tx).await?;
    if row.get::<String,_>("refresh_digest")!=hash {
        sqlx::query("UPDATE auth_sessions SET revoked=true WHERE id=$1").bind(id).execute(&mut *tx).await?;
        tx.commit().await?;return Err(ApiError::unauthorized());
    }
    if row.get::<bool,_>("revoked")||row.get::<bool,_>("device_revoked")||row.get::<chrono::DateTime<Utc>,_>("refresh_expires")<=Utc::now() {return Err(ApiError::unauthorized());}
    let tokens=SessionTokens{user_id:row.get("user_id"),device_id:row.get("device_id"),access_token:token(),refresh_token:token()};
    sqlx::query("INSERT INTO used_refresh_tokens(digest,session_id) VALUES($1,$2)").bind(hash).bind(id).execute(&mut *tx).await?;
    sqlx::query("UPDATE auth_sessions SET access_digest=$2,access_expires=$3,refresh_digest=$4,refresh_expires=$5 WHERE id=$1").bind(id).bind(digest(&app,&tokens.access_token)).bind(Utc::now()+Duration::minutes(15)).bind(digest(&app,&tokens.refresh_token)).bind(Utc::now()+Duration::days(30)).execute(&mut *tx).await?;
    tx.commit().await?;Ok(Json(tokens))
}
pub async fn logout(State(app):State<App>,identity:Identity)->Result<Json<Value>> {
    sqlx::query("UPDATE auth_sessions SET revoked=true WHERE id=$1 AND user_id=$2").bind(identity.session_id).bind(identity.user_id).execute(&app.pool).await?;
    Ok(Json(json!({"ok":true})))
}
pub async fn devices(State(app):State<App>,identity:Identity)->Result<Json<Value>> {
    let rows=sqlx::query("SELECT id,name,revoked,created_at FROM devices WHERE user_id=$1 ORDER BY created_at").bind(identity.user_id).fetch_all(&app.pool).await?;
    Ok(Json(Value::Array(rows.into_iter().map(|r|json!({"id":r.get::<Uuid,_>("id"),"name":r.get::<String,_>("name"),"revoked":r.get::<bool,_>("revoked"),"createdAt":r.get::<chrono::DateTime<Utc>,_>("created_at")})).collect())))
}
pub async fn revoke(State(app):State<App>,identity:Identity,Path(id):Path<Uuid>)->Result<Json<Value>> {
    let mut tx=app.pool.begin().await?;
    sqlx::query("UPDATE devices SET revoked=true WHERE user_id=$1 AND id=$2").bind(identity.user_id).bind(id).execute(&mut *tx).await?;
    sqlx::query("UPDATE auth_sessions SET revoked=true WHERE user_id=$1 AND device_id=$2").bind(identity.user_id).bind(id).execute(&mut *tx).await?;
    tx.commit().await?;Ok(Json(json!({"ok":true})))
}
