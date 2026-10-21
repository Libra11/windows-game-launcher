//! 只连接显式指定的隔离测试数据库；不读取应用 DATABASE_URL。
use axum::{body::Body,http::{Request,StatusCode},Router};
use hmac::{Hmac,Mac};
use http_body_util::BodyExt;
use serde_json::{json,Value};
use sha2::Sha256;
use sqlx::PgPool;
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;
use youji_cloud_protocol::*;
use youji_cloud_server::{App,config::Config};

fn hash(value:&str)->String {let mut h=Hmac::<Sha256>::new_from_slice(b"test-key-only-never-use-in-production-123").unwrap();h.update(value.as_bytes());hex::encode(h.finalize().into_bytes())}
async fn request(router:&Router,method:&str,path:&str,token:&str,body:Value)->(StatusCode,Value) {
    let response=router.clone().oneshot(Request::builder().method(method).uri(path).header("authorization",format!("Bearer {token}")).header("content-type","application/json").body(Body::from(serde_json::to_vec(&body).unwrap())).unwrap()).await.unwrap();
    let status=response.status();let bytes=response.into_body().collect().await.unwrap().to_bytes();
    (status,serde_json::from_slice(&bytes).unwrap_or_else(|_|json!({"text":String::from_utf8_lossy(&bytes)})))
}
async fn device(pool:&PgPool,user:Uuid)->(Uuid,String,String) {
    let id=Uuid::new_v4();let access=format!("{}{}",Uuid::new_v4().simple(),Uuid::new_v4().simple());let refresh=format!("{}{}",Uuid::new_v4().simple(),Uuid::new_v4().simple());
    sqlx::query("INSERT INTO devices(user_id,id,name) VALUES($1,$2,'测试设备')").bind(user).bind(id).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO auth_sessions(id,user_id,device_id,access_digest,access_expires,refresh_digest,refresh_expires) VALUES($1,$2,$3,$4,now()+interval '15 minutes',$5,now()+interval '30 days')").bind(Uuid::new_v4()).bind(user).bind(id).bind(hash(&access)).bind(hash(&refresh)).execute(pool).await.unwrap();
    (id,access,refresh)
}
async fn user(pool:&PgPool)->Uuid {
    let id=Uuid::new_v4();sqlx::query("INSERT INTO users(id,email) VALUES($1,$2)").bind(id).bind(format!("{id}@example.com")).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO user_sync_state(user_id) VALUES($1)").bind(id).execute(pool).await.unwrap();id
}
fn op(kind:ResourceType,id:impl Into<String>,payload:Value,base:i64)->Operation {
    Operation{operation_id:Uuid::new_v4(),resource_type:kind,resource_id:id.into(),action:Action::Set,base_revision:base,depends_on_operation_id:None,payload}
}
async fn push(router:&Router,token:&str,ops:Vec<Operation>)->Vec<OperationResult> {
    let (status,value)=request(router,"POST","/v1/sync/push",token,json!(PushRequest{protocol_version:1,operations:ops})).await;
    assert_eq!(status,StatusCode::OK,"{value}");serde_json::from_value(value).unwrap()
}

#[tokio::test]
async fn postgres_sync_identity_import_conflict_replay_and_auth() {
    let Ok(url)=std::env::var("YOUJI_TEST_DATABASE_URL") else {eprintln!("未设置 YOUJI_TEST_DATABASE_URL，未执行 Postgres 集成测试");return;};
    let pool=PgPool::connect(&url).await.unwrap();youji_cloud_server::migrate(&pool).await.unwrap();
    let app=App{pool:pool.clone(),config:Arc::new(Config{token_key:"test-key-only-never-use-in-production-123".into(),mail: youji_cloud_server::mail::MailConfig::Http { url: "https://example.invalid/mail".into(), token: "unused".into() },bind:"127.0.0.1:0".into()}),http:reqwest::Client::new(),storage:None};
    let router=youji_cloud_server::router(app);let owner=user(&pool).await;let (a,ta,refresh)=device(&pool,owner).await;let (b,tb,_)=device(&pool,owner).await;
    let other=user(&pool).await;let (_,tc,_)=device(&pool,other).await;
    let local=format!("local-{}",Uuid::new_v4());let local_digest="a".repeat(64);
    let initial=json!({"resources":[{"resourceType":"game","resourceId":local,"payload":{"source":"local","sourceIdentity":local,"displayTitle":"测试游戏","metadataReference":"480","achievementReference":"480"}},{"resourceType":"historical_baseline","resourceId":local,"payload":{"gameId":local,"seconds":360000}}],"localDigest":local_digest});
    let (s,preview)=request(&router,"POST","/v1/imports/prepare",&ta,initial.clone()).await;assert_eq!(s,StatusCode::OK,"{preview}");
    let game=preview["mappings"][0]["cloudId"].as_str().unwrap().to_string();
    let path=format!("/v1/imports/{}/commit",preview["importId"].as_str().unwrap());
    let (s,result)=request(&router,"POST",&path,&ta,json!({"localDigest":local_digest})).await;assert_eq!(s,StatusCode::OK,"{result}");
    assert_eq!(request(&router,"POST",&path,&ta,json!({"localDigest":local_digest})).await.1,result);
    assert_eq!(request(&router,"POST",&path,&tc,json!({"localDigest":local_digest})).await.0,StatusCode::BAD_REQUEST);
    let game_id=Uuid::parse_str(&game).unwrap();
    for (device,token,seconds) in [(a,&ta,7200),(b,&tb,10800)] {
        let payload=PlaySession{game_id,owner_device_id:device,checkpoint:1,started_at:"2026-10-21T00:00:00Z".into(),ended_at:Some("2026-10-21T04:00:00Z".into()),seconds,days:[("2026-10-21".into(),seconds)].into(),utc_offset_seconds:28800,historical:false};
        let operation=op(ResourceType::PlaySession,Uuid::new_v4().to_string(),json!(payload),0);
        let one=push(&router,token,vec![operation.clone()]).await;let retry=push(&router,token,vec![operation]).await;
        assert_eq!(one[0].sequence,retry[0].sequence);assert_eq!(one[0].outcome,Outcome::Applied);
    }
    let total:i64=sqlx::query_scalar("SELECT SUM((payload->>'seconds')::bigint)::bigint FROM sync_resources WHERE user_id=$1 AND kind IN ('historical_baseline','play_session')").bind(owner).fetch_one(&pool).await.unwrap();assert_eq!(total,105*3600);
    let favorite=op(ResourceType::Favorite,&game,json!({"value":true}),0);assert_eq!(push(&router,&ta,vec![favorite.clone()]).await[0].outcome,Outcome::Applied);
    let stale=op(ResourceType::Favorite,&game,json!({"value":false}),0);assert_eq!(push(&router,&tb,vec![stale]).await[0].outcome,Outcome::Conflict);
    let mut dependent=op(ResourceType::Favorite,&game,json!({"value":false}),0);dependent.depends_on_operation_id=Some(favorite.operation_id);assert_eq!(push(&router,&ta,vec![dependent]).await[0].outcome,Outcome::Applied);
    let mut mismatched=favorite.clone();mismatched.payload=json!({"value":false});assert_eq!(request(&router,"POST","/v1/sync/push",&ta,json!(PushRequest{protocol_version:1,operations:vec![mismatched]})).await.0,StatusCode::BAD_REQUEST);
    // 两个事务同时修改同一版本，仅一个成功；其余必须显式冲突。
    let first=op(ResourceType::Title,&game,json!({"value":"电脑标题"}),0);
    let second=op(ResourceType::Title,&game,json!({"value":"掌机标题"}),0);
    let (left,right)=tokio::join!(push(&router,&ta,vec![first]),push(&router,&tb,vec![second]));
    assert!(matches!((left[0].outcome,right[0].outcome),(Outcome::Applied,Outcome::Conflict)|(Outcome::Conflict,Outcome::Applied)));
    // 开启另一个用户空库后，引用其他用户游戏仍必须拒绝。
    sqlx::query("UPDATE user_sync_state SET initialized=true WHERE user_id=$1").bind(other).execute(&pool).await.unwrap();
    assert_eq!(push(&router,&tc,vec![op(ResourceType::Favorite,&game,json!({"value":true}),0)]).await[0].outcome,Outcome::Rejected);
    let tag=Uuid::new_v4().to_string();let created=op(ResourceType::Tag,&tag,json!({"name":"标签"}),0);assert_eq!(push(&router,&ta,vec![created]).await[0].outcome,Outcome::Applied);
    let mut deleted=op(ResourceType::Tag,&tag,Value::Null,1);deleted.action=Action::Delete;assert_eq!(push(&router,&ta,vec![deleted]).await[0].outcome,Outcome::Applied);
    let member=op(ResourceType::TagMember,format!("{game}:{tag}"),json!({"gameId":game,"categoryId":tag,"present":true}),0);assert_eq!(push(&router,&tb,vec![member]).await[0].outcome,Outcome::Rejected);
    let (_,snapshot)=request(&router,"POST","/v1/sync/snapshots",&ta,Value::Null).await;
    let snap_path=format!("/v1/sync/snapshots/{}?offset=0&limit=500",snapshot["snapshotId"].as_str().unwrap());
    assert_eq!(request(&router,"GET",&snap_path,&tc,Value::Null).await.0,StatusCode::BAD_REQUEST);
    let (_,page)=request(&router,"GET","/v1/sync/changes?cursor=0&limit=2",&ta,Value::Null).await;
    let page:ChangesPage=serde_json::from_value(page).unwrap();assert_eq!(page.next_cursor,2);assert!(page.has_more);
    let mut cursor=page.next_cursor;while cursor<page.high_watermark {
        let path=format!("/v1/sync/changes?cursor={cursor}&highWatermark={}&limit=2",page.high_watermark);
        let (s,v)=request(&router,"GET",&path,&ta,Value::Null).await;assert_eq!(s,StatusCode::OK);let p:ChangesPage=serde_json::from_value(v).unwrap();assert_eq!(p.changes[0].sequence,cursor+1);cursor=p.next_cursor;
    }
    // 刷新成功后旧 refresh 重放撤销新 access，不能继续访问。
    let (s,tokens)=request(&router,"POST","/v1/auth/refresh","",json!({"refreshToken":refresh})).await;assert_eq!(s,StatusCode::OK);
    assert_eq!(request(&router,"POST","/v1/auth/refresh","",json!({"refreshToken":refresh})).await.0,StatusCode::UNAUTHORIZED);
    assert_eq!(request(&router,"GET","/v1/sync/status",tokens["accessToken"].as_str().unwrap(),Value::Null).await.0,StatusCode::UNAUTHORIZED);
    // 设备撤销对仍有效的 access 立即生效。
    let revoke_path=format!("/v1/devices/{b}");assert_eq!(request(&router,"DELETE",&revoke_path,&tb,Value::Null).await.0,StatusCode::OK);
    assert_eq!(request(&router,"GET","/v1/sync/status",&tb,Value::Null).await.0,StatusCode::UNAUTHORIZED);
    // 验证码失败次数持久化，不能通过第五次之后的正确验证码绕过限制。
    let email=format!("{}@example.com",Uuid::new_v4());let challenge=Uuid::new_v4();
    sqlx::query("INSERT INTO invites(email) VALUES($1)").bind(&email).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO login_challenges(id,email,digest,expires_at) VALUES($1,$2,$3,now()+interval '10 minutes')").bind(challenge).bind(&email).bind(hash(&format!("{challenge}:12345678"))).execute(&pool).await.unwrap();
    for _ in 0..5 {assert_eq!(request(&router,"POST","/v1/auth/verify","",json!({"challengeId":challenge,"code":"00000000","deviceName":"test"})).await.0,StatusCode::UNAUTHORIZED);}
    assert_eq!(request(&router,"POST","/v1/auth/verify","",json!({"challengeId":challenge,"code":"12345678","deviceName":"test"})).await.0,StatusCode::UNAUTHORIZED);
    let challenge=Uuid::new_v4();
    sqlx::query("INSERT INTO login_challenges(id,email,digest,expires_at) VALUES($1,$2,$3,now()+interval '10 minutes')").bind(challenge).bind(&email).bind(hash(&format!("{challenge}:87654321"))).execute(&pool).await.unwrap();
    assert_eq!(request(&router,"POST","/v1/auth/verify","",json!({"challengeId":challenge,"code":"87654321","deviceName":"新设备"})).await.0,StatusCode::OK);
    assert_eq!(request(&router,"POST","/v1/auth/verify","",json!({"challengeId":challenge,"code":"87654321","deviceName":"重复"})).await.0,StatusCode::UNAUTHORIZED);
}
