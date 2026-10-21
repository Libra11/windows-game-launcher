use lettre::{
    message::Mailbox, transport::smtp::authentication::Credentials, AsyncSmtpTransport,
    AsyncTransport, Message, Tokio1Executor,
};
use std::{future::Future, time::Duration};

pub const SEND_TIMEOUT: Duration = Duration::from_secs(20);

// 不派生 Debug，避免配置中的凭据意外进入日志。
pub enum MailConfig {
    Smtp(SmtpConfig),
    Http { url: String, token: String },
}

pub struct SmtpConfig {
    pub host: String,
    pub port: u16,
    /// true 使用隐式 TLS；false 必须升级 STARTTLS，禁止明文回退。
    pub secure: bool,
    pub user: String,
    pub pass: String,
    pub from: Mailbox,
}

fn smtp_transport(config: &SmtpConfig) -> Result<AsyncSmtpTransport<Tokio1Executor>, &'static str> {
    let builder = if config.secure {
        AsyncSmtpTransport::<Tokio1Executor>::relay(&config.host)
    } else {
        AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&config.host)
    }
    .map_err(|_| "SMTP TLS 配置无效")?;
    Ok(builder
        .port(config.port)
        .credentials(Credentials::new(config.user.clone(), config.pass.clone()))
        .timeout(Some(SEND_TIMEOUT))
        .build())
}

fn message(config: &SmtpConfig, to: &str, text: String) -> Result<Message, &'static str> {
    Message::builder()
        .from(config.from.clone())
        .to(to.parse().map_err(|_| "邮件收件人格式无效")?)
        .subject("游迹登录验证码")
        .body(text)
        .map_err(|_| "邮件构造失败")
}

async fn bounded_send(
    delivery: impl Future<Output = Result<(), &'static str>>,
) -> Result<(), &'static str> {
    tokio::time::timeout(SEND_TIMEOUT, delivery)
        .await
        .map_err(|_| "邮件发送超时")?
}

/// 只返回固定错误，不暴露验证码、SMTP 凭据或服务端响应。
pub async fn send(
    config: &MailConfig,
    http: &reqwest::Client,
    to: &str,
    code: &str,
) -> Result<(), &'static str> {
    bounded_send(async {
        let text = format!("验证码：{code}。10 分钟内有效，请勿向他人提供。");
        match config {
            MailConfig::Smtp(config) => {
                let mail = message(config, to, text)?;
                smtp_transport(config)?
                    .send(mail)
                    .await
                    .map(|_| ())
                    .map_err(|_| "SMTP 邮件发送失败")
            }
            MailConfig::Http { url, token } => {
                let response = http
                    .post(url)
                    .bearer_auth(token)
                    .json(&serde_json::json!({"to":to,"subject":"游迹登录验证码","text":text}))
                    .send()
                    .await
                    .map_err(|_| "邮件接口请求失败")?;
                if response.status().is_success() {
                    Ok(())
                } else {
                    Err("邮件接口发送失败")
                }
            }
        }
    })
    .await
}

#[cfg(test)]
#[path = "mail/tests.rs"]
mod tests;
