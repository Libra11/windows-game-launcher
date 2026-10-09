use crate::{db, lock_db, AppState};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::Manager;

const CLIENT_ID: &str = "34a02cf8f4414e29b15921876da36f9a";
const CLIENT_SECRET: &str = "daafbccc737745039dffe53d94fc76cf";
const TOKEN_URL: &str =
    "https://account-public-service-prod03.ol.epicgames.com/account/api/oauth/token";
const SESSION_KEY: &str = "epic_oauth_session";
const PRIVACY_ACTION: &str = "Epic 要求处理账号隐私政策，请先在 Epic 商店或官方客户端登录同一账号，处理实际出现的账号提示，再重新获取授权码。开发者协议正文不能完成该操作";

fn corrective_error(data: &Value) -> Option<&'static str> {
    if data.get("errorCode").and_then(Value::as_str) != Some("errors.com.epicgames.oauth.corrective_action_required") { return None; }
    let action = data.pointer("/metadata/correctiveAction").or_else(|| data.get("correctiveAction")).and_then(Value::as_str);
    Some(if action == Some("PRIVACY_POLICY_ACCEPTANCE") { PRIVACY_ACTION } else { "Epic 要求完成账号验证，请先在 Epic 官网处理账号提示，再重新登录" })
}
pub(crate) static AUTH_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Session {
    pub access_token: String,
    refresh_token: String,
    pub(crate) account_id: String,
    display_name: String,
    expires_at: DateTime<Utc>,
}
impl Session {
    fn from_response(data: Value) -> Result<Self, String> {
        let text = |key| {
            data.get(key)
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
        };
        Ok(Self {
            access_token: text("access_token").ok_or("Epic 未返回访问凭证")?,
            refresh_token: text("refresh_token").ok_or("Epic 未返回刷新凭证，请重新登录")?,
            account_id: text("account_id").ok_or("Epic 未返回账号身份")?,
            display_name: text("displayName").unwrap_or_default(),
            expires_at: text("expires_at")
                .ok_or("Epic 未返回凭证有效期")?
                .parse()
                .map_err(|_| "Epic 凭证有效期无效")?,
        })
    }
    fn needs_refresh(&self) -> bool {
        self.expires_at <= Utc::now() + chrono::Duration::seconds(120)
    }
}

pub(crate) fn client() -> Result<reqwest::Client, String> {
    crate::network::client(crate::network::Service::Epic)
}

pub(crate) fn network_error(error: reqwest::Error) -> String {
    // 不输出请求 URL、Authorization 或 Epic 返回的原始错误正文。
    if error.is_timeout() {
        "Epic 连接超时，请检查网络后重试".into()
    } else {
        "无法连接 Epic，请检查网络和代理配置后重试".into()
    }
}

async fn exchange(kind: &str, key: &str, value: &str) -> Result<Session, String> {
    let response = client()?
        .post(TOKEN_URL)
        .basic_auth(CLIENT_ID, Some(CLIENT_SECRET))
        .form(&[("grant_type", kind), (key, value), ("token_type", "eg1")])
        .send()
        .await
        .map_err(network_error)?;
    if !response.status().is_success() {
        let status = response.status().as_u16();
        if let Ok(data) = response.json::<Value>().await {
            if let Some(message) = corrective_error(&data) { return Err(message.into()); }
        }
        return Err(match status {
            400 | 401 => "Epic 授权码或授权已失效，请重新登录并复制新的授权码".into(),
            403 => "Epic 拒绝授权，请在浏览器完成账号验证后重新登录".into(),
            429 => "Epic 请求过于频繁，请稍后重试".into(),
            _ => "Epic 授权服务暂时不可用，请稍后重试".into(),
        });
    }
    Session::from_response(response.json().await.map_err(|_| "Epic 授权响应格式无效")?)
}

fn read(app: &tauri::AppHandle) -> Result<Option<Session>, String> {
    let raw = db::setting(&*lock_db(&app.state::<AppState>())?, SESSION_KEY)?;
    if raw.is_empty() {
        return Ok(None);
    }
    serde_json::from_str(&raw)
        .map(Some)
        .map_err(|_| "Epic 授权记录无效，请重新登录".into())
}
fn save(app: &tauri::AppHandle, session: &Session) -> Result<(), String> {
    db::set_setting(
        &*lock_db(&app.state::<AppState>())?,
        SESSION_KEY,
        &serde_json::to_string(session).map_err(|_| "无法保存 Epic 授权")?,
    )
}

// 调用方持有 AUTH_LOCK，避免刷新凭证轮换与登录、断开连接相互覆盖。
pub(crate) async fn session(app: &tauri::AppHandle, force: bool) -> Result<Session, String> {
    let current = read(app)?.ok_or("请先在设置中登录 Epic 账号")?;
    if !force && !current.needs_refresh() {
        return Ok(current);
    }
    let mut updated = exchange("refresh_token", "refresh_token", &current.refresh_token).await?;
    if updated.account_id != current.account_id {
        return Err("Epic 刷新授权的账号不一致，请重新登录".into());
    }
    if updated.display_name.is_empty() {
        updated.display_name = current.display_name;
    }
    save(app, &updated)?;
    Ok(updated)
}

fn authorization_code(input: &str) -> Result<String, String> {
    let trimmed = input.trim();
    let code = if trimmed.starts_with('{') {
        let data: Value =
            serde_json::from_str(trimmed).map_err(|_| "请粘贴 Epic 返回的授权码或完整授权结果")?;
        if let Some(message) = corrective_error(&data) { return Err(message.into()); }
        data.get("authorizationCode")
            .and_then(Value::as_str)
            .ok_or("授权结果中没有 authorizationCode")?
            .to_owned()
    } else {
        trimmed.to_owned()
    };
    if code.is_empty()
        || code.len() > 1024
        || !code
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b))
    {
        return Err("授权码格式无效，请复制 Epic 页面中的 authorizationCode".into());
    }
    Ok(code)
}

#[tauri::command]
pub(crate) fn epic_begin_login() -> Result<(), String> {
    open_browser(login_url()?.as_str())
}

fn login_url() -> Result<reqwest::Url, String> {
    let mut url = reqwest::Url::parse("https://www.epicgames.com/id/login")
        .map_err(|_| "无法生成 Epic 登录地址")?;
    url.query_pairs_mut().append_pair(
        "redirectUrl",
        &format!(
            "https://www.epicgames.com/id/api/redirect?clientId={CLIENT_ID}&responseType=code"
        ),
    );
    Ok(url)
}

#[tauri::command]
pub(crate) fn epic_open_account_login() -> Result<(), String> {
    open_browser("https://www.epicgames.com/id/login?redirectUrl=https%3A%2F%2Fstore.epicgames.com%2F")
}

fn open_browser(url: &str) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL};
        // 通过 HTTPS 默认关联打开完整 URL，避免 Explorer 将登录参数当成文件路径。
        let operation: Vec<u16> = "open\0".encode_utf16().collect();
        let target: Vec<u16> = url.encode_utf16().chain(Some(0)).collect();
        let result = unsafe {
            ShellExecuteW(
                std::ptr::null_mut(), operation.as_ptr(), target.as_ptr(),
                std::ptr::null(), std::ptr::null(), SW_SHOWNORMAL,
            )
        } as isize;
        return if result > 32 {
            Ok(())
        } else {
            Err("无法打开系统浏览器，请在 Windows 设置中确认默认浏览器及 HTTPS 链接关联后重试".into())
        };
    }
    #[cfg(not(target_os = "windows"))]
    {
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open").arg(url).spawn();
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let result = std::process::Command::new("xdg-open")
        .arg(url)
        .spawn();
    let mut child = result.map_err(|_| "无法打开系统浏览器，请检查默认浏览器设置")?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
    }
}

#[tauri::command]
pub(crate) async fn epic_complete_login(
    app: tauri::AppHandle,
    code: String,
) -> Result<Value, String> {
    let code = authorization_code(&code)?;
    let _guard = AUTH_LOCK.lock().await;
    let session = exchange("authorization_code", "code", &code).await?;
    save(&app, &session)?;
    Ok(serde_json::json!({"connected":true,"displayName":session.display_name}))
}

#[tauri::command]
pub(crate) fn epic_connection_status(app: tauri::AppHandle) -> Result<Value, String> {
    let current = read(&app)?;
    Ok(
        serde_json::json!({"connected":current.is_some(),"displayName":current.map(|s| s.display_name).unwrap_or_default()}),
    )
}

#[tauri::command]
pub(crate) async fn epic_disconnect(app: tauri::AppHandle) -> Result<(), String> {
    let _guard = AUTH_LOCK.lock().await;
    db::set_setting(&*lock_db(&app.state::<AppState>())?, SESSION_KEY, "")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn browser_login_preserves_official_redirect_parameters() {
        let url = login_url().unwrap();
        assert_eq!(url.scheme(), "https");
        assert_eq!(url.host_str(), Some("www.epicgames.com"));
        assert_eq!(url.path(), "/id/login");
        let pairs: Vec<_> = url.query_pairs().collect();
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].0, "redirectUrl");
        let redirect = reqwest::Url::parse(&pairs[0].1).unwrap();
        assert_eq!(redirect.host_str(), Some("www.epicgames.com"));
        assert_eq!(redirect.path(), "/id/api/redirect");
        assert!(redirect.query_pairs().any(|(key, value)| key == "clientId" && value == CLIENT_ID));
        assert!(redirect.query_pairs().any(|(key, value)| key == "responseType" && value == "code"));
    }
    #[test]
    fn privacy_policy_error_is_actionable_and_does_not_expose_continuation() {
        let data = serde_json::json!({"errorCode":"errors.com.epicgames.oauth.corrective_action_required","metadata":{"correctiveAction":"PRIVACY_POLICY_ACCEPTANCE","continuation":"private-continuation"}});
        assert_eq!(corrective_error(&data), Some(PRIVACY_ACTION));
        let error = authorization_code(&data.to_string()).unwrap_err();
        assert!(error.contains("Epic 商店或官方客户端"));
        assert!(!error.contains("private-continuation"));
    }
    #[test]
    fn accepts_only_authorization_codes_and_epic_redirect_json() {
        assert_eq!(authorization_code(" abc-123 ").unwrap(), "abc-123");
        assert_eq!(
            authorization_code(r#"{"authorizationCode":"abc123"}"#).unwrap(),
            "abc123"
        );
        assert!(authorization_code(r#"{"access_token":"secret"}"#).is_err());
        assert!(authorization_code("https://example.com/?code=secret").is_err());
        assert!(authorization_code("").is_err());
    }
    #[test]
    fn validates_session_and_refreshes_before_expiry() {
        let mut data = serde_json::json!({"access_token":"access","refresh_token":"refresh","account_id":"account","expires_at":(Utc::now()+chrono::Duration::hours(1)).to_rfc3339()});
        assert!(!Session::from_response(data.clone())
            .unwrap()
            .needs_refresh());
        data["expires_at"] =
            serde_json::json!((Utc::now() + chrono::Duration::seconds(60)).to_rfc3339());
        assert!(Session::from_response(data.clone())
            .unwrap()
            .needs_refresh());
        data["refresh_token"] = Value::Null;
        assert!(Session::from_response(data).is_err());
    }
}
