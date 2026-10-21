// 只使用显式指定的隔离数据库，不读取 DATABASE_URL。
use axum::{body::Body,http::{Request,StatusCode,HeaderMap},Router};
use hmac::{Hmac,Mac};
use sha2::Sha256;
use serde_json::{json,Value};
use sqlx::{PgPool,Row};
use std::sync::Arc;
use tower::ServiceExt;
use http_body_util::BodyExt;
use uuid::Uuid;
use youji_cloud_server::{App,config::Config};
const KEY:&str="regression-test-key-not-production-1234567890";
fn hash(s:&str)->String {let mut h=Hmac::<Sha256>::new_from_slice(KEY.as_bytes()).unwrap();h.update(s.as_bytes());hex::encode(h.finalize().into_bytes())}
async fn call(router:&Router,path:&str,token:&str,body:Value)->(StatusCode,HeaderMap,Value) {
    let response=router.clone().oneshot(Request::builder().method("POST").uri(path)
        .header("authorization",format!("Bearer {token}")).header("content-type","application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap())).unwrap()).await.unwrap();
    let status=response.status();let headers=response.headers().clone();
    let bytes=response.into_body().collect().await.unwrap().to_bytes();
    (status,headers,serde_json::from_slice(&bytes).unwrap())
}
async fn setup()->Option<(PgPool,Router)> {
    let Ok(url)=std::env::var("YOUJI_TEST_DATABASE_URL") else {eprintln!("缺少隔离数据库 URL，跳过");return None};
    let pool=PgPool::connect(&url).await.unwrap();youji_cloud_server::migrate(&pool).await.unwrap();
    let app=App{pool:pool.clone(),config:Arc::new(Config{token_key:KEY.into(),mail: youji_cloud_server::mail::MailConfig::Http { url: "https://unused.invalid".into(), token: "unused".into() },bind:"127.0.0.1:0".into()}),http:reqwest::Client::new(),storage:None};
    Some((pool,youji_cloud_server::router(app)))
}
async fn device(pool:&PgPool,user:Uuid)->(Uuid,String) {
    let device=Uuid::new_v4();let access=format!("{}{}",Uuid::new_v4().simple(),Uuid::new_v4().simple());
    sqlx::query("INSERT INTO devices(user_id,id,name) VALUES($1,$2,'回归测试')").bind(user).bind(device).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO auth_sessions(id,user_id,device_id,access_digest,access_expires,refresh_digest,refresh_expires) VALUES($1,$2,$3,$4,now()+interval '15 minutes',$5,now()+interval '30 days')")
        .bind(Uuid::new_v4()).bind(user).bind(device).bind(hash(&access)).bind(hash(&Uuid::new_v4().to_string())).execute(pool).await.unwrap();
    (device,access)
}
#[tokio::test]
async fn imports_reuse_live_sessions_preserve_daily_history_and_retry_commit() {
    let Some((pool,router))=setup().await else{return};
    let user=Uuid::new_v4();let game=Uuid::new_v4();
    sqlx::query("INSERT INTO users(id,email) VALUES($1,$2)").bind(user).bind(format!("{user}@test.invalid")).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO user_sync_state(user_id,initialized) VALUES($1,true)").bind(user).execute(&pool).await.unwrap();
    let (owner,_)=device(&pool,user).await;let (restored,token)=device(&pool,user).await;
    sqlx::query("INSERT INTO sync_resources(user_id,kind,id,revision,payload,device_id) VALUES($1,'game',$2,1,$3,$4)")
        .bind(user).bind(game.to_string()).bind(json!({"source":"steam","sourceIdentity":"480","displayTitle":"test","metadataReference":"480","achievementReference":"480"})).bind(owner).execute(&pool).await.unwrap();
    let live=Uuid::new_v4();let historical=Uuid::new_v4();
    let payload=|historical| json!({"gameId":game,"ownerDeviceId":owner,"checkpoint":1,"startedAt":"2026-10-20T00:00:00Z",
        "endedAt":"2026-10-20T00:05:00Z","seconds":100,"days":{"2026-10-20":100},"utcOffsetSeconds":0,"historical":historical});
    for (id,hist) in [(live,false),(historical,true)] {
        sqlx::query("INSERT INTO sync_resources(user_id,kind,id,revision,payload,device_id) VALUES($1,'play_session',$2,1,$3,$4)")
            .bind(user).bind(id.to_string()).bind(payload(hist)).bind(owner).execute(&pool).await.unwrap();
    }
    let mut historical_backup=payload(true);historical_backup["seconds"]=json!(120);historical_backup["days"]=json!({});
    let prepare=|live_payload:Value|json!({"localDigest":"a".repeat(64),"resources":[
        {"resourceType":"play_session","resourceId":live,"payload":live_payload},
        {"resourceType":"play_session","resourceId":historical,"payload":historical_backup}]});
    let (status,_,preview)=call(&router,"/v1/imports/prepare",&token,prepare(payload(false))).await;
    assert_eq!(status,StatusCode::OK,"{preview}");
    let path=format!("/v1/imports/{}/commit",preview["importId"].as_str().unwrap());
    let (status,_,result)=call(&router,&path,&token,json!({"localDigest":"a".repeat(64)})).await;
    assert_eq!(status,StatusCode::OK,"{result}");
    assert_eq!(call(&router,&path,&token,json!({"localDigest":"a".repeat(64)})).await.2,result);
    let live_row=sqlx::query("SELECT payload,revision FROM sync_resources WHERE user_id=$1 AND kind='play_session' AND id=$2")
        .bind(user).bind(live.to_string()).fetch_one(&pool).await.unwrap();
    assert_eq!(live_row.get::<Value,_>("payload"),payload(false));assert_eq!(live_row.get::<i64,_>("revision"),1);
    let history:Value=sqlx::query_scalar("SELECT payload FROM sync_resources WHERE user_id=$1 AND kind='play_session' AND id=$2")
        .bind(user).bind(historical.to_string()).fetch_one(&pool).await.unwrap();
    assert_eq!(history["seconds"],120);assert_eq!(history["days"],json!({"2026-10-20":100}));
    assert_eq!(history["ownerDeviceId"],json!(owner));assert_ne!(owner,restored);
    let mut older=payload(false);older["seconds"]=json!(80);older["days"]=json!({"2026-10-20":80});
    assert_eq!(call(&router,"/v1/imports/prepare",&token,prepare(older)).await.0,StatusCode::OK);
    let mut newer=payload(false);newer["seconds"]=json!(150);newer["days"]=json!({"2026-10-20":150});
    assert_eq!(call(&router,"/v1/imports/prepare",&token,prepare(newer)).await.0,StatusCode::CONFLICT);
    let mut wrong=payload(false);wrong["startedAt"]=json!("2026-10-19T00:00:00Z");
    assert_eq!(call(&router,"/v1/imports/prepare",&token,prepare(wrong)).await.0,StatusCode::CONFLICT);
}
#[tokio::test]
async fn challenge_throttling_is_uniform_and_preserves_valid_challenge() {
    let Some((pool,router))=setup().await else{return};
    for invited in [true,false] {
        let email=format!("{}@test.invalid",Uuid::new_v4());let id=Uuid::new_v4();
        if invited {sqlx::query("INSERT INTO invites(email) VALUES($1)").bind(&email).execute(&pool).await.unwrap();}
        sqlx::query("INSERT INTO login_challenges(id,email,digest,expires_at) VALUES($1,$2,$3,now()+interval '10 minutes')")
            .bind(id).bind(&email).bind(hash(&format!("{id}:12345678"))).execute(&pool).await.unwrap();
        let (a,b)=tokio::join!(call(&router,"/v1/auth/challenges","",json!({"email":email})),call(&router,"/v1/auth/challenges","",json!({"email":email})));
        for response in [a,b] {
            assert_eq!(response.0,StatusCode::TOO_MANY_REQUESTS);
            assert_eq!(response.1.get("retry-after").unwrap(),"60");assert!(response.2.get("challengeId").is_none());
        }
        let count:i64=sqlx::query_scalar("SELECT count(*) FROM login_challenges WHERE email=$1").bind(&email).fetch_one(&pool).await.unwrap();
        assert_eq!(count,1);
        let (status,_,body)=call(&router,"/v1/auth/verify","",json!({"challengeId":id,"code":"12345678","deviceName":"首次登录"})).await;
        assert_eq!(status,if invited {StatusCode::OK}else{StatusCode::UNAUTHORIZED},"{body}");
    }
}
