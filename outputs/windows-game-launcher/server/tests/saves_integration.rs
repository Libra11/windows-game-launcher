// 验证主路由实际挂接且所有存档入口均要求认证；不连接生产数据库。
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use std::sync::Arc;
use tower::ServiceExt;
use youji_cloud_server::{config::Config, App};
#[tokio::test]
async fn save_routes_require_authentication() {
    let app = App {
        pool: sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://unused:unused@127.0.0.1:1/unused")
            .unwrap(),
        config: Arc::new(Config {
            token_key: "test-only-key-123456789012345678901234".into(),
            mail: youji_cloud_server::mail::MailConfig::Http { url: "https://invalid.test".into(), token: "unused".into() },
            bind: "127.0.0.1:0".into(),
        }),
        http: reqwest::Client::new(),
        storage: None,
    };
    let router = youji_cloud_server::router(app);
    for (method, path) in [
        ("POST", "/v1/save-profiles"),
        (
            "GET",
            "/v1/save-profiles?gameId=00000000-0000-0000-0000-000000000000",
        ),
        ("POST", "/v1/save-uploads"),
        (
            "GET",
            "/v1/save-uploads/00000000-0000-0000-0000-000000000000",
        ),
        (
            "POST",
            "/v1/save-uploads/00000000-0000-0000-0000-000000000000/complete",
        ),
        (
            "GET",
            "/v1/save-profiles/00000000-0000-0000-0000-000000000000/snapshots",
        ),
        (
            "POST",
            "/v1/save-profiles/00000000-0000-0000-0000-000000000000/resolve",
        ),
        (
            "POST",
            "/v1/save-snapshots/00000000-0000-0000-0000-000000000000/download",
        ),
        (
            "DELETE",
            "/v1/save-snapshots/00000000-0000-0000-0000-000000000000",
        ),
    ] {
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(path)
                    .header("content-type", "application/json")
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "{method} {path}"
        );
    }
}
