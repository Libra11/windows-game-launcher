pub mod auth;
pub mod social;
pub mod config;
pub mod mail;
pub mod error;
pub mod imports;
pub mod sync;
pub mod constraints;
pub mod saves;
pub mod storage;

use axum::{routing::{get, post, delete}, Router, extract::DefaultBodyLimit};
use sqlx::PgPool;
use std::sync::Arc;
pub use error::{ApiError, Result};

#[derive(Clone)]
pub struct App { pub pool: PgPool, pub config: Arc<config::Config>, pub http: reqwest::Client, pub storage: Option<Arc<storage::Storage>> }

pub fn router(app: App) -> Router {
    Router::new()
        .merge(saves::routes())
        .merge(social::routes())
        .route("/health",get(|| async { "ok" }))
        .route("/v1/auth/challenges",post(auth::challenge))
        .route("/v1/auth/verify",post(auth::verify))
        .route("/v1/auth/refresh",post(auth::refresh))
        .route("/v1/auth/logout",post(auth::logout))
        .route("/v1/devices",get(auth::devices))
        .route("/v1/devices/{id}",delete(auth::revoke))
        .route("/v1/sync/status",get(sync::status))
        .route("/v1/sync/push",post(sync::push))
        .route("/v1/sync/changes",get(sync::changes))
        .route("/v1/sync/snapshots",post(sync::snapshot_create))
        .route("/v1/sync/snapshots/{id}",get(sync::snapshot_page))
        .route("/v1/imports/prepare",post(imports::prepare).layer(DefaultBodyLimit::max(64*1024*1024)))
        .route("/v1/imports/{id}/commit",post(imports::commit))
        .layer(DefaultBodyLimit::max(youji_cloud_protocol::MAX_REQUEST_BYTES))
        .with_state(app)
}

pub async fn migrate(pool: &PgPool) -> std::result::Result<(), sqlx::migrate::MigrateError> {
    sqlx::migrate!().run(pool).await
}
