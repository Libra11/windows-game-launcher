use super::*;
use std::collections::HashMap;

fn settings() -> HashMap<&'static str, String> {
    [
        ("YOUJI_TOKEN_KEY", "test-token-key-at-least-32-bytes-long"),
        ("YOUJI_MAIL_TRANSPORT", "smtp"),
        ("SMTP_HOST", "smtp.example.test"),
        ("SMTP_PORT", "465"),
        ("SMTP_SECURE", "true"),
        ("SMTP_USER", "test-user"),
        ("SMTP_PASS", "test-password"),
        ("SMTP_FROM", "游迹 <sender@example.test>"),
    ]
    .into_iter()
    .map(|(k, v)| (k, v.into()))
    .collect()
}

#[test]
fn smtp_modes_and_custom_port() {
    let mut values = settings();
    for (secure, port) in [("true", "465"), ("false", "587"), ("false", "2525")] {
        values.insert("SMTP_SECURE", secure.into());
        values.insert("SMTP_PORT", port.into());
        let config = Config::from_lookup(|key| values.get(key).cloned()).unwrap();
        let MailConfig::Smtp(smtp) = config.mail else {
            panic!("传输类型错误")
        };
        assert_eq!(smtp.secure, secure == "true");
        assert_eq!(smtp.port.to_string(), port);
        assert_eq!(smtp.from.email.to_string(), "sender@example.test");
    }
}

#[test]
fn smtp_rejects_missing_and_invalid_values_without_echoing_input() {
    for key in [
        "YOUJI_MAIL_TRANSPORT",
        "SMTP_HOST",
        "SMTP_PORT",
        "SMTP_SECURE",
        "SMTP_USER",
        "SMTP_PASS",
        "SMTP_FROM",
    ] {
        let mut values = settings();
        values.remove(key);
        assert!(Config::from_lookup(|name| values.get(name).cloned()).is_err());
        values.insert(key, " ".into());
        assert!(Config::from_lookup(|name| values.get(name).cloned()).is_err());
    }
    for (key, value) in [
        ("SMTP_SECURE", "maybe"),
        ("SMTP_PORT", "0"),
        ("SMTP_PORT", "65536"),
        ("SMTP_PORT", "secret-port"),
        ("SMTP_FROM", "secret-invalid-address"),
        ("SMTP_HOST", "smtp://secret-host"),
        ("SMTP_HOST", "host\r\nsecret"),
        ("YOUJI_MAIL_TRANSPORT", "plain"),
    ] {
        let mut values = settings();
        values.insert(key, value.into());
        let error = Config::from_lookup(|name| values.get(name).cloned())
            .err()
            .unwrap();
        assert!(!error.contains(value));
    }
}

#[test]
fn https_adapter_is_available_without_smtp_settings() {
    let mut values = HashMap::from([
        (
            "YOUJI_TOKEN_KEY",
            "test-token-key-at-least-32-bytes-long".into(),
        ),
        ("YOUJI_MAIL_TRANSPORT", "http".into()),
        ("YOUJI_MAIL_URL", "https://mail.example.test/send".into()),
        ("YOUJI_MAIL_TOKEN", "test-token".into()),
    ]);
    assert!(matches!(
        Config::from_lookup(|key| values.get(key).cloned())
            .unwrap()
            .mail,
        MailConfig::Http { .. }
    ));
    for url in [
        "http://mail.example.test",
        "https://user:secret@mail.example.test",
        "invalid",
    ] {
        values.insert("YOUJI_MAIL_URL", url.into());
        assert!(Config::from_lookup(|key| values.get(key).cloned()).is_err());
    }
}
