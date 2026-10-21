use axum::{http::StatusCode, response::{IntoResponse,Response}, Json};
pub type Result<T> = std::result::Result<T,ApiError>;
#[derive(Debug)]
pub struct ApiError(pub StatusCode,pub String);
impl ApiError {
    pub fn bad(s:impl Into<String>)->Self {Self(StatusCode::BAD_REQUEST,s.into())}
    pub fn unauthorized()->Self {Self(StatusCode::UNAUTHORIZED,"登录已失效，请重新登录".into())}
    pub fn conflict(s:impl Into<String>)->Self {Self(StatusCode::CONFLICT,s.into())}
}
impl From<sqlx::Error> for ApiError {
    fn from(_:sqlx::Error)->Self {Self(StatusCode::INTERNAL_SERVER_ERROR,"数据库操作失败".into())}
}
impl IntoResponse for ApiError {
    fn into_response(self)->Response {(self.0,Json(serde_json::json!({"error":self.1}))).into_response()}
}
