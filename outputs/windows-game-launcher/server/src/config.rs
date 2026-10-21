use crate::mail::{MailConfig, SmtpConfig};

pub struct Config {
    pub token_key: String,
    pub mail: MailConfig,
    pub bind: String,
}
impl Config {
    pub fn from_env() -> Result<Self, String> {
        Self::from_lookup(|name| std::env::var(name).ok())
    }

    fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Result<Self, String> {
        let required = |name: &str| {
            get(name)
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| format!("缺少配置 {name}"))
        };
        let token_key = required("YOUJI_TOKEN_KEY")?;
        if token_key.len() < 32 {
            return Err("YOUJI_TOKEN_KEY 至少需要 32 字节随机密钥".into());
        }
        let mail = match required("YOUJI_MAIL_TRANSPORT")?.as_str() {
            "smtp" => {
                let host = required("SMTP_HOST")?;
                if host.chars().any(|c| c.is_whitespace() || c.is_control())
                    || host.contains(['/', '@', ':'])
                {
                    return Err("SMTP_HOST 必须为主机名或 IPv4 地址".into());
                }
                let secure = match required("SMTP_SECURE")?.as_str() {
                    "true" => true,
                    "false" => false,
                    _ => return Err("SMTP_SECURE 必须为 true 或 false".into()),
                };
                let port = required("SMTP_PORT")?
                    .parse::<u16>()
                    .ok()
                    .filter(|port| *port > 0)
                    .ok_or("SMTP_PORT 必须为 1–65535")?;
                let from = required("SMTP_FROM")?
                    .parse()
                    .map_err(|_| "SMTP_FROM 邮箱格式无效")?;
                MailConfig::Smtp(SmtpConfig {
                    host,
                    port,
                    secure,
                    from,
                    user: required("SMTP_USER")?,
                    pass: required("SMTP_PASS")?,
                })
            }
            "http" => {
                let url = required("YOUJI_MAIL_URL")?;
                let parsed = url::Url::parse(&url).map_err(|_| "邮件接口地址无效")?;
                if parsed.scheme() != "https"
                    || parsed.host_str().is_none()
                    || !parsed.username().is_empty()
                    || parsed.password().is_some()
                {
                    return Err("邮件接口必须使用无内嵌凭据的 HTTPS 地址".into());
                }
                MailConfig::Http {
                    url,
                    token: required("YOUJI_MAIL_TOKEN")?,
                }
            }
            _ => return Err("YOUJI_MAIL_TRANSPORT 必须为 smtp 或 http".into()),
        };
        Ok(Self {
            token_key,
            mail,
            bind: get("YOUJI_BIND").unwrap_or_else(|| "127.0.0.1:8090".into()),
        })
    }
}

#[cfg(test)]
#[path = "mail/config_tests.rs"]
mod tests;
