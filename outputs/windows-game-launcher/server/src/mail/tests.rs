use super::*;

fn smtp(secure: bool) -> SmtpConfig {
    SmtpConfig {
        host: "localhost".into(),
        port: if secure { 465 } else { 587 },
        secure,
        user: "fake-user".into(),
        pass: "fake-password".into(),
        from: "游迹 <sender@example.test>".parse().unwrap(),
    }
}

#[tokio::test]
async fn constructs_both_tls_modes_and_message_without_connecting() {
    for secure in [true, false] {
        let config = smtp(secure);
        let transport = smtp_transport(&config).unwrap();
        let diagnostic = format!("{transport:?}");
        assert!(diagnostic.contains(if secure { "Wrapper" } else { "Required" }));
        let mail = message(&config, "recipient@example.test", "测试正文".into()).unwrap();
        assert_eq!(
            mail.envelope().to()[0].to_string(),
            "recipient@example.test"
        );
        assert!(message(&config, "invalid\r\nBcc: secret", "secret-code".into()).is_err());
    }
}

#[tokio::test(start_paused = true)]
async fn total_timeout_is_twenty_seconds() {
    let start = tokio::time::Instant::now();
    assert_eq!(
        bounded_send(std::future::pending()).await,
        Err("邮件发送超时")
    );
    assert_eq!(start.elapsed(), Duration::from_secs(20));
    assert_eq!(bounded_send(async { Ok(()) }).await, Ok(()));
    assert_eq!(
        bounded_send(async { Err("SMTP 邮件发送失败") }).await,
        Err("SMTP 邮件发送失败")
    );
}

#[tokio::test]
async fn starttls_refuses_server_without_tls_before_credentials_or_message() {
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut config = smtp(false);
    config.host = "127.0.0.1".into();
    config.port = listener.local_addr().unwrap().port();
    let peer = tokio::spawn(async move {
        let (socket, _) = listener.accept().await.unwrap();
        let mut socket = BufReader::new(socket);
        socket
            .get_mut()
            .write_all(b"220 localhost ESMTP\r\n")
            .await
            .unwrap();
        let mut greeting = String::new();
        socket.read_line(&mut greeting).await.unwrap();
        assert!(greeting.starts_with("EHLO "));
        socket
            .get_mut()
            .write_all(b"250-localhost\r\n250 AUTH PLAIN LOGIN\r\n")
            .await
            .unwrap();
        let mut rest = String::new();
        socket.read_to_string(&mut rest).await.unwrap();
        assert!(!rest.contains("AUTH"));
        assert!(!rest.contains("MAIL FROM"));
        assert!(!rest.contains("DATA"));
        assert!(!rest.contains("fake-password"));
        assert!(!rest.contains("12345678"));
    });
    let result = send(
        &MailConfig::Smtp(config),
        &reqwest::Client::new(),
        "recipient@example.test",
        "12345678",
    )
    .await;
    assert_eq!(result, Err("SMTP 邮件发送失败"));
    tokio::time::timeout(Duration::from_secs(2), peer)
        .await
        .unwrap()
        .unwrap();
}
