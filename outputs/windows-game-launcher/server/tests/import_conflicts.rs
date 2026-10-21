// 只使用显式指定的隔离数据库，不读取 DATABASE_URL。
use axum::{body::Body,http::{Request,StatusCode,HeaderMap},Router};
use hmac::{Hmac,Mac};
use sha2::Sha256;
use serde_json::{json,Value};
use sqlx::{PgPool};
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
async fn new_device(pool:&PgPool,user:Uuid)->(Uuid,String) {
    let device=Uuid::new_v4();let access=format!("{}{}",Uuid::new_v4().simple(),Uuid::new_v4().simple());
    sqlx::query("INSERT INTO devices(user_id,id,name) VALUES($1,$2,'回归测试')").bind(user).bind(device).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO auth_sessions(id,user_id,device_id,access_digest,access_expires,refresh_digest,refresh_expires) VALUES($1,$2,$3,$4,now()+interval '15 minutes',$5,now()+interval '30 days')")
        .bind(Uuid::new_v4()).bind(user).bind(device).bind(hash(&access)).bind(hash(&Uuid::new_v4().to_string())).execute(pool).await.unwrap();
    (device,access)
}
async fn seed(pool:&PgPool,user:Uuid,device:Uuid,kind:&str,id:String,payload:Value){
    sqlx::query("INSERT INTO sync_resources(user_id,kind,id,revision,payload,device_id) VALUES($1,$2,$3,1,$4,$5)")
        .bind(user).bind(kind).bind(id).bind(payload).bind(device).execute(pool).await.unwrap();
}
#[tokio::test]
async fn classification_and_mutable_imports_require_explicit_decisions(){
    let Some((pool,router))=setup().await else{return};
    let user=Uuid::new_v4();let game=Uuid::new_v4();let category=Uuid::new_v4();let local=Uuid::new_v4();
    sqlx::query("INSERT INTO users(id,email) VALUES($1,$2)").bind(user).bind(format!("{user}@test.invalid")).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO user_sync_state(user_id) VALUES($1)").bind(user).execute(&pool).await.unwrap();
    let(device,token)=new_device(&pool,user).await;
    seed(&pool,user,device,"game",game.to_string(),json!({"source":"steam","sourceIdentity":"480","displayTitle":"test","metadataReference":"480","achievementReference":"480"})).await;
    seed(&pool,user,device,"collection",category.to_string(),json!({"name":"Action"})).await;
    seed(&pool,user,device,"collection_order","order".into(),json!({"ids":[category]})).await;
    seed(&pool,user,device,"favorite",game.to_string(),json!({"value":false})).await;
    seed(&pool,user,device,"collection_member",format!("{game}:{category}"),json!({"gameId":game,"categoryId":category,"present":false})).await;
    let unlock=youji_cloud_protocol::unlock_id(game,"480","A","manual");
    seed(&pool,user,device,"manual_unlock",unlock.clone(),json!({"gameId":game,"namespace":"480","apiName":"A","source":"manual","unlockedAt":null,"present":false})).await;
    let mut body=json!({"localDigest":"a".repeat(64),"resources":[
        {"resourceType":"collection","resourceId":local,"payload":{"name":"action"}},
        {"resourceType":"collection_member","resourceId":"local-member","payload":{"gameId":game,"categoryId":local,"present":true}},
        {"resourceType":"collection_order","resourceId":"order","payload":{"ids":[local]}},
        {"resourceType":"favorite","resourceId":game,"payload":{"value":true}},
        {"resourceType":"manual_unlock","resourceId":"local-unlock","payload":{"gameId":game,"namespace":"480","apiName":"A","source":"manual","unlockedAt":null,"present":true}}
    ]});
    let(status,_,preview)=call(&router,"/v1/imports/prepare",&token,body.clone()).await;
    assert_eq!(status,StatusCode::OK,"{preview}");assert!(preview["unresolvedCount"].as_u64().unwrap()>0);
    let path=format!("/v1/imports/{}/commit",preview["importId"].as_str().unwrap());
    assert_eq!(call(&router,&path,&token,json!({"localDigest":"a".repeat(64)})).await.0,StatusCode::CONFLICT);
    body["resolutions"]=json!([
        {"key":format!("collection:{local}"),"choice":"map","targetId":category},
        {"key":format!("favorite:{game}"),"choice":"local"},
        {"key":"manual_unlock:local-unlock","choice":"cloud"},
        {"key":"collection_member:local-member","choice":"local"}
    ]);
    let(status,_,preview)=call(&router,"/v1/imports/prepare",&token,body.clone()).await;
    assert_eq!(status,StatusCode::OK,"{preview}");assert_eq!(preview["unresolvedCount"],0,"{preview}");
    assert_eq!(preview["categoryMappings"][0]["cloudId"],category.to_string());
    // 预演不得改变同步序号、幂等日志和真实数据。
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT sequence FROM user_sync_state WHERE user_id=$1").bind(user).fetch_one(&pool).await.unwrap(),0);
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM sync_operations WHERE user_id=$1").bind(user).fetch_one(&pool).await.unwrap(),0);
    let path=format!("/v1/imports/{}/commit",preview["importId"].as_str().unwrap());
    let(status,_,result)=call(&router,&path,&token,json!({"localDigest":"a".repeat(64)})).await;
    assert_eq!(status,StatusCode::OK,"{result}");
    let favorite:Value=sqlx::query_scalar("SELECT payload FROM sync_resources WHERE user_id=$1 AND kind='favorite'").bind(user).fetch_one(&pool).await.unwrap();
    assert_eq!(favorite["value"],true);
    let manual:Value=sqlx::query_scalar("SELECT payload FROM sync_resources WHERE user_id=$1 AND kind='manual_unlock'").bind(user).fetch_one(&pool).await.unwrap();
    assert_eq!(manual["present"],false);
    let member:Value=sqlx::query_scalar("SELECT payload FROM sync_resources WHERE user_id=$1 AND kind='collection_member'").bind(user).fetch_one(&pool).await.unwrap();
    assert_eq!(member["categoryId"],category.to_string());assert_eq!(member["present"],true);
    // 重命名路径 + 采用云端顺序也要包含本次新建分类。
    body["resources"]=json!([
        {"resourceType":"collection","resourceId":local,"payload":{"name":"action"}},
        {"resourceType":"collection_order","resourceId":"order","payload":{"ids":[local]}}
    ]);
    body["resolutions"]=json!([
        {"key":format!("collection:{local}"),"choice":"rename","name":"新分类"},
        {"key":"collection_order:order","choice":"cloud"}
    ]);
    let(status,_,preview)=call(&router,"/v1/imports/prepare",&token,body).await;
    assert_eq!(status,StatusCode::OK,"{preview}");assert_eq!(preview["unresolvedCount"],0,"{preview}");
    let path=format!("/v1/imports/{}/commit",preview["importId"].as_str().unwrap());
    let(status,_,result)=call(&router,&path,&token,json!({"localDigest":"a".repeat(64)})).await;assert_eq!(status,StatusCode::OK,"{result}");
    let order:Value=sqlx::query_scalar("SELECT payload FROM sync_resources WHERE user_id=$1 AND kind='collection_order'").bind(user).fetch_one(&pool).await.unwrap();
    assert_eq!(order["ids"],json!([category,local]));
}
#[tokio::test]
async fn import_batch_over_one_mib_is_allowed_but_single_resource_is_not(){
    let Some((pool,router))=setup().await else{return};
    let user=Uuid::new_v4();
    sqlx::query("INSERT INTO users(id,email) VALUES($1,$2)").bind(user).bind(format!("{user}@test.invalid")).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO user_sync_state(user_id) VALUES($1)").bind(user).execute(&pool).await.unwrap();
    let(_,token)=new_device(&pool,user).await;
    // 三个独立定义缓存各小于 1 MiB，总请求超过旧的全局限制。
    let mut resources=Vec::new();
    for _ in 0..3{
        let game=Uuid::new_v4();let definitions:Vec<_>=(0..2000).map(|i|json!({"apiName":format!("A{i}"),"name":"测试","description":"x".repeat(180),"icon":"","hidden":false})).collect();
        resources.push(json!({"resourceType":"game","resourceId":game,"payload":{"source":"local","sourceIdentity":format!("local-{game}"),"displayTitle":"test","metadataReference":"","achievementReference":"480"}}));
        resources.push(json!({"resourceType":"definition_cache","resourceId":game,"payload":{"gameId":game,"namespace":"480","source":"test","language":"zh-CN","complete":true,"definitions":definitions}}));
    }
    let body=json!({"localDigest":"a".repeat(64),"resources":resources});
    assert!(serde_json::to_vec(&body).unwrap().len()>1024*1024);
    let(status,_,preview)=call(&router,"/v1/imports/prepare",&token,body).await;
    assert_eq!(status,StatusCode::OK,"{preview}");
    let(status,_,error)=call(&router,"/v1/imports/prepare",&token,json!({"localDigest":"a".repeat(64),"resources":[{"resourceType":"tag","resourceId":Uuid::new_v4(),"payload":{"name":"x".repeat(1024*1024)}}]})).await;
    assert_eq!(status,StatusCode::BAD_REQUEST,"{error}");assert!(error.to_string().contains("1 MiB"));
}

#[tokio::test]
async fn independent_game_fields_compare_initial_values_and_current_reference(){
    let Some((pool,router))=setup().await else{return};
    let user=Uuid::new_v4();let game=Uuid::new_v4();
    sqlx::query("INSERT INTO users(id,email) VALUES($1,$2)").bind(user).bind(format!("{user}@test.invalid")).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO user_sync_state(user_id) VALUES($1)").bind(user).execute(&pool).await.unwrap();
    let(device,token)=new_device(&pool,user).await;
    seed(&pool,user,device,"game",game.to_string(),json!({"source":"local","sourceIdentity":format!("local-{}",Uuid::new_v4()),"displayTitle":"云名称","metadataReference":"480","achievementReference":"480"})).await;
    seed(&pool,user,device,"achievement_reference",game.to_string(),json!({"value":"xbox:123"})).await;
    let body=json!({"localDigest":"a".repeat(64),"mappings":[{"localId":"local-A","cloudId":game}],"resources":[
        {"resourceType":"game","resourceId":"local-A","payload":{"source":"local","sourceIdentity":format!("local-{}",Uuid::new_v4()),"displayTitle":"本机名称","metadataReference":"999","achievementReference":"xbox:123"}},
        {"resourceType":"title","resourceId":"local-A","payload":{"value":"本机名称"}},
        {"resourceType":"metadata_reference","resourceId":"local-A","payload":{"value":"999"}},
        {"resourceType":"achievement_reference","resourceId":"local-A","payload":{"value":"xbox:123"}}
    ]});
    let(status,_,preview)=call(&router,"/v1/imports/prepare",&token,body.clone()).await;
    assert_eq!(status,StatusCode::OK,"{preview}");assert_eq!(preview["unresolvedCount"],2,"{preview}");
    let mut resolved=body.clone();resolved["resolutions"]=json!([{"key":"title:local-A","choice":"local"},{"key":"metadata_reference:local-A","choice":"cloud"}]);
    let(status,_,preview)=call(&router,"/v1/imports/prepare",&token,resolved).await;
    assert_eq!(status,StatusCode::OK,"{preview}");assert_eq!(preview["unresolvedCount"],0,"{preview}");
    let path=format!("/v1/imports/{}/commit",preview["importId"].as_str().unwrap());
    assert_eq!(call(&router,&path,&token,json!({"localDigest":"a".repeat(64)})).await.0,StatusCode::OK);
    let title:Value=sqlx::query_scalar("SELECT payload FROM sync_resources WHERE user_id=$1 AND kind='title'").bind(user).fetch_one(&pool).await.unwrap();
    assert_eq!(title["value"],"本机名称");
    let metadata:Value=sqlx::query_scalar("SELECT payload FROM sync_resources WHERE user_id=$1 AND kind='metadata_reference' AND id=$2").bind(user).bind(game.to_string()).fetch_one(&pool).await.unwrap();
    assert_eq!(metadata["value"],"480");
    let revision:i64=sqlx::query_scalar("SELECT revision FROM sync_resources WHERE user_id=$1 AND kind='metadata_reference' AND id=$2").bind(user).bind(game.to_string()).fetch_one(&pool).await.unwrap();
    assert_eq!(revision,1);
    // 同值无需用户解决，但仍应物化独立字段，不能只依赖 Game 初始载荷。
    let equal_game=Uuid::new_v4();
    seed(&pool,user,device,"game",equal_game.to_string(),json!({"source":"steam","sourceIdentity":"481","displayTitle":"同名","metadataReference":"481","achievementReference":"481"})).await;
    let(status,_,equal)=call(&router,"/v1/imports/prepare",&token,json!({"localDigest":"a".repeat(64),"resources":[
        {"resourceType":"title","resourceId":equal_game,"payload":{"value":"同名"}},
        {"resourceType":"metadata_reference","resourceId":equal_game,"payload":{"value":"481"}},
        {"resourceType":"achievement_reference","resourceId":equal_game,"payload":{"value":"481"}}
    ]})).await;
    assert_eq!(status,StatusCode::OK,"{equal}");assert_eq!(equal["unresolvedCount"],0);
    let path=format!("/v1/imports/{}/commit",equal["importId"].as_str().unwrap());
    assert_eq!(call(&router,&path,&token,json!({"localDigest":"a".repeat(64)})).await.0,StatusCode::OK);
    let count:i64=sqlx::query_scalar("SELECT count(*) FROM sync_resources WHERE user_id=$1 AND id=$2 AND kind IN ('title','metadata_reference','achievement_reference') AND revision=1").bind(user).bind(equal_game.to_string()).fetch_one(&pool).await.unwrap();
    assert_eq!(count,3);
    let other=Uuid::new_v4();
    sqlx::query("INSERT INTO users(id,email) VALUES($1,$2)").bind(other).bind(format!("{other}@test.invalid")).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO user_sync_state(user_id) VALUES($1)").bind(other).execute(&pool).await.unwrap();
    let(_,token)=new_device(&pool,other).await;
    assert_eq!(call(&router,"/v1/imports/prepare",&token,body).await.0,StatusCode::BAD_REQUEST);
}
