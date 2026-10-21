use super::*;
fn config_values() -> BTreeMap<&'static str, String> {
    BTreeMap::from([
        ("OSS_ENDPOINT", "oss-cn-hangzhou.aliyuncs.com".into()),
        (
            "OSS_INTERNAL_ENDPOINT",
            "oss-cn-hangzhou-internal.aliyuncs.com".into(),
        ),
        ("OSS_BUCKET", "examplebucket".into()),
        ("OSS_REGION", "cn-hangzhou".into()),
        ("OSS_ACCESS_KEY_ID", "ak".into()),
        ("OSS_ACCESS_KEY_SECRET", "sk".into()),
    ])
}
fn configured(values: &BTreeMap<&str, String>) -> Storage {
    config::read(|k| values.get(k).cloned())
        .unwrap_or_else(|e| panic!("{}", e.1))
        .unwrap()
}
#[test]
fn config_uses_virtual_hosts_and_rejects_ambiguous_or_partial_settings() {
    let values = config_values();
    let s = configured(&values);
    assert_eq!(
        s.public_endpoint.as_str(),
        "https://examplebucket.oss-cn-hangzhou.aliyuncs.com/"
    );
    assert_eq!(
        s.service_endpoint.as_str(),
        "https://examplebucket.oss-cn-hangzhou-internal.aliyuncs.com/"
    );
    assert!(config::read(|_| None)
        .unwrap_or_else(|e| panic!("{}", e.1))
        .is_none());
    for (key, value) in [
        ("OSS_ENDPOINT", "http://oss-cn-hangzhou.aliyuncs.com"),
        (
            "OSS_ENDPOINT",
            "https://examplebucket.oss-cn-hangzhou.aliyuncs.com",
        ),
        ("OSS_ENDPOINT", "oss-cn-hangzhou-internal.aliyuncs.com"),
        ("OSS_ENDPOINT", "oss-cn-beijing.aliyuncs.com"),
        ("OSS_ENDPOINT", "https://oss-cn-hangzhou.aliyuncs.com/?acl"),
        ("OSS_INTERNAL_ENDPOINT", "oss-cn-hangzhou.aliyuncs.com"),
        ("OSS_BUCKET", "invalid.bucket"),
        ("OSS_BUCKET", "-invalid"),
        ("OSS_REGION", "oss-cn-hangzhou"),
        ("OSS_ACCESS_KEY_SECRET", ""),
        ("USER_QUOTA_BYTES", "0"),
        ("USER_QUOTA_BYTES", "18446744073709551615"),
    ] {
        let mut invalid = values.clone();
        invalid.insert(key, value.into());
        assert!(
            config::read(|k| invalid.get(k).cloned()).is_err(),
            "{key}={value}"
        );
    }
    let mut partial = values.clone();
    partial.remove("OSS_ENDPOINT");
    assert!(config::read(|k| partial.get(k).cloned()).is_err());
    let mut public_only = values.clone();
    public_only.remove("OSS_INTERNAL_ENDPOINT");
    let s = configured(&public_only);
    assert_eq!(s.public_endpoint, s.service_endpoint);
}
#[test]
fn oss_query_subresource_sts_and_additional_headers_are_canonical() {
    let mut values = config_values();
    values.insert("OSS_SECURITY_TOKEN", "token+/=".into());
    let s = configured(&values);
    let now = chrono::DateTime::parse_from_rfc3339("2024-12-03T03:44:20Z")
        .unwrap()
        .with_timezone(&Utc);
    let headers = BTreeMap::from([
        ("content-length".into(), "17".into()),
        ("content-type".into(), "application/octet-stream".into()),
        ("x-oss-object-acl".into(), "private".into()),
        ("x-oss-server-side-encryption".into(), "AES256".into()),
    ]);
    let (public, returned) = s
        .presign(
            &s.public_endpoint,
            signing::Request {
                method: "PUT",
                key: "目录/a +%.zip",
                query: &[("acl", "")],
                expires: 600,
                headers: headers.clone(),
            },
            now,
        )
        .unwrap_or_else(|e| panic!("{}", e.1));
    assert!(public.contains("/%E7%9B%AE%E5%BD%95/a%20%2B%25.zip?acl&"));
    assert!(public.contains("x-oss-additional-headers=content-length%3Bhost"));
    assert!(public.contains("x-oss-security-token=token%2B%2F%3D"));
    assert!(public.contains("%2Fcn-hangzhou%2Foss%2Faliyun_v4_request"));
    assert!(!public.contains("x-amz") && !public.contains("AWS4"));
    assert_eq!(returned, headers);
    let (internal, _) = s
        .presign(
            &s.service_endpoint,
            signing::Request {
                method: "PUT",
                key: "目录/a +%.zip",
                query: &[("acl", "")],
                expires: 600,
                headers,
            },
            now,
        )
        .unwrap_or_else(|e| panic!("{}", e.1));
    assert_ne!(
        public.split("x-oss-signature=").last(),
        internal.split("x-oss-signature=").last()
    );
}

#[derive(Clone)]
struct Mock {
    responses: std::sync::Arc<tokio::sync::Mutex<BTreeMap<String, (u16, String)>>>,
    calls: std::sync::Arc<tokio::sync::Mutex<Vec<String>>>,
}
async fn handler(
    axum::extract::State(mock): axum::extract::State<Mock>,
    req: axum::extract::Request,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    let path = req.uri().path().to_owned();
    let q: BTreeMap<String, String> =
        url::form_urlencoded::parse(req.uri().query().unwrap_or("").as_bytes())
            .into_owned()
            .collect();
    assert_eq!(
        q.get("x-oss-signature-version").map(String::as_str),
        Some("OSS4-HMAC-SHA256")
    );
    assert!(q
        .get("x-oss-credential")
        .unwrap()
        .contains("/oss/aliyun_v4_request"));
    assert!(q.get("x-oss-signature").unwrap().len() == 64);
    mock.calls
        .lock()
        .await
        .push(format!("{} {path}", req.method()));
    for sub in ["acl", "publicAccessBlock", "policyStatus", "encryption"] {
        if q.contains_key(sub) {
            let (code, body) = mock.responses.lock().await.get(sub).unwrap().clone();
            return (axum::http::StatusCode::from_u16(code).unwrap(), body).into_response();
        }
    }
    if req.method() == reqwest::Method::PUT {
        assert_eq!(req.headers()["x-oss-server-side-encryption"], "AES256");
        assert_eq!(req.headers()["x-oss-object-acl"], "private");
        assert_eq!(req.headers()["content-length"], "3");
        let bytes = axum::body::to_bytes(req.into_body(), 10).await.unwrap();
        assert_eq!(&bytes[..], b"abc");
    }
    "abc".into_response()
}
async fn mock() -> (Storage, Mock, tokio::task::JoinHandle<()>) {
    let m = Mock {
        responses: Default::default(),
        calls: Default::default(),
    };
    *m.responses.lock().await=BTreeMap::from([
        ("acl".into(),(200,"<AccessControlPolicy><AccessControlList><Grant>private</Grant></AccessControlList></AccessControlPolicy>".into())),
        ("publicAccessBlock".into(),(200,"<PublicAccessBlockConfiguration><BlockPublicAccess>true</BlockPublicAccess></PublicAccessBlockConfiguration>".into())),
        ("policyStatus".into(),(200,"<PolicyStatus><IsPublic>false</IsPublic></PolicyStatus>".into())),
        ("encryption".into(),(200,"<ServerSideEncryptionRule><ApplyServerSideEncryptionByDefault><SSEAlgorithm>AES256</SSEAlgorithm></ApplyServerSideEncryptionByDefault></ServerSideEncryptionRule>".into())),
    ]);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let app = axum::Router::new().fallback(handler).with_state(m.clone());
    let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (Storage::test(endpoint, 100), m, task)
}
#[tokio::test]
async fn oss_http_security_fails_closed_and_preserves_kms() {
    let (s, m, task) = mock().await;
    assert!(s.check_private().await.is_ok());
    for (sub,body) in [
        ("acl","<AccessControlPolicy><AccessControlList><Grant>public-read</Grant></AccessControlList></AccessControlPolicy>"),
        ("publicAccessBlock","<PublicAccessBlockConfiguration><BlockPublicAccess>false</BlockPublicAccess></PublicAccessBlockConfiguration>"),
        ("policyStatus","<PolicyStatus><IsPublic>true</IsPublic></PolicyStatus>"),
        ("encryption","<ServerSideEncryptionRule><ApplyServerSideEncryptionByDefault><SSEAlgorithm>aws:kms</SSEAlgorithm></ApplyServerSideEncryptionByDefault></ServerSideEncryptionRule>"),
        ("acl","<AccessControlPolicy><AccessControlList><Grant>private</Grant><Grant>public-read</Grant></AccessControlList></AccessControlPolicy>"),
    ]{
        let old=m.responses.lock().await.insert(sub.into(),(200,body.into())).unwrap();
        assert!(s.check_private().await.is_err(),"{sub}");
        m.responses.lock().await.insert(sub.into(),old);
    }
    let old = m
        .responses
        .lock()
        .await
        .insert("policyStatus".into(), (403, "<Error/>".into()))
        .unwrap();
    assert!(s.download_url("object").await.is_err());
    m.responses.lock().await.insert("policyStatus".into(), old);
    m.responses.lock().await.insert("encryption".into(),(200,"<ServerSideEncryptionRule><ApplyServerSideEncryptionByDefault><SSEAlgorithm>KMS</SSEAlgorithm><KMSMasterKeyID>key-id</KMSMasterKeyID></ApplyServerSideEncryptionByDefault></ServerSideEncryptionRule>".into()));
    let (_, headers) = s
        .upload_url("staging/file", 3)
        .await
        .unwrap_or_else(|e| panic!("{}", e.1));
    assert_eq!(headers["x-oss-server-side-encryption"], "KMS");
    assert_eq!(headers["x-oss-server-side-encryption-key-id"], "key-id");
    task.abort();
}
#[tokio::test]
async fn oss_http_internal_transfers_and_public_urls_are_separate() {
    let (mut s, m, task) = mock().await;
    s.public_endpoint = Url::parse("https://test.oss-cn-hangzhou.aliyuncs.com/").unwrap();
    let (upload, headers) = s
        .upload_url("staging/file", 3)
        .await
        .unwrap_or_else(|e| panic!("{}", e.1));
    assert!(upload.starts_with("https://test.oss-cn-hangzhou.aliyuncs.com/staging/file?"));
    assert_eq!(headers["content-length"], "3");
    assert!(s
        .download_url("objects/file")
        .await
        .unwrap_or_else(|e| panic!("{}", e.1))
        .starts_with("https://test.oss-cn-hangzhou.aliyuncs.com/objects/file?"));
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("file");
    assert!(s.fetch("staging/file", &path, 3).await.is_ok());
    assert_eq!(tokio::fs::read(&path).await.unwrap(), b"abc");
    assert!(s.fetch("staging/file", &path, 2).await.is_err());
    tokio::fs::write(&path, b"abc").await.unwrap();
    assert!(s.publish("objects/file", &path, 3).await.is_ok());
    assert!(s.delete("staging/file").await.is_ok());
    let calls = m.calls.lock().await;
    assert!(calls.contains(&"GET /staging/file".into()));
    assert!(calls.contains(&"PUT /objects/file".into()));
    assert!(calls.contains(&"DELETE /staging/file".into()));
    task.abort();
}

// 官方 SDK 固定凭据/时间/查询/输出向量，非本实现自行生成的期望值：
// https://github.com/aliyun/alibabacloud-oss-python-sdk-v2/blob/master/tests/unit/signer/test_v4.py
#[test]
fn official_oss_sdk_v4_presign_vectors() {
    let mut values = config_values();
    values.insert("OSS_BUCKET", "bucket".into());
    let mut s = configured(&values);
    let query = [
        ("param1", "value1"),
        ("+param1", "value3"),
        ("|param1", "value4"),
        ("+param2", ""),
        ("|param2", ""),
        ("param2", ""),
    ];
    for (timestamp, token, additional, expected) in [
        (
            1702781677,
            None,
            false,
            "a39966c61718be0d5b14e668088b3fa07601033f6518ac7b523100014269c0fe",
        ),
        (
            1702785388,
            Some("token"),
            false,
            "3817ac9d206cd6dfc90f1c09c00be45005602e55898f26f5ddb06d7892e1f8b5",
        ),
        (
            1702783809,
            None,
            true,
            "6bd984bfe531afb6db1f7550983a741b103a8c58e5e14f83ea474c2322dfa2b7",
        ),
    ] {
        s.security_token = token.map(str::to_owned);
        let mut headers = BTreeMap::from([
            ("content-type".into(), "application/octet-stream".into()),
            ("x-oss-head1".into(), "value".into()),
        ]);
        if additional {
            headers.insert("abc".into(), "value".into());
            headers.insert("zabc".into(), "value".into());
        }
        let (url, _) = s
            .presign_headers(
                &s.public_endpoint,
                signing::Request {
                    method: "PUT",
                    key: "1234+-/123/1.txt",
                    query: &query,
                    expires: 599,
                    headers,
                },
                chrono::DateTime::from_timestamp(timestamp, 0).unwrap(),
            )
            .unwrap_or_else(|e| panic!("{}", e.1));
        let parsed = Url::parse(&url).unwrap();
        let signature = parsed
            .query_pairs()
            .find(|(k, _)| k == "x-oss-signature")
            .unwrap()
            .1
            .into_owned();
        assert_eq!(
            signature, expected,
            "token={token:?}, additional={additional}"
        );
    }
}

#[test]
fn object_prefix_is_required_and_bounds_every_object_key() {
    let mut values = config_values();
    values.insert("OSS_PREFIX", "games/saves".into());
    let s = configured(&values);
    assert_eq!(
        s.object_key("staging/user/upload").unwrap().as_str(),
        "games/saves/staging/user/upload"
    );
    for key in [
        "",
        "/outside",
        "../outside",
        "objects/../outside",
        "objects//bad",
    ] {
        assert!(s.object_key(key).is_err());
    }
    for prefix in ["", "/", "../bad", "a/../b", "a//b", "a/"] {
        values.insert("OSS_PREFIX", prefix.into());
        assert!(config::read(|k| values.get(k).cloned()).is_err());
    }
}
