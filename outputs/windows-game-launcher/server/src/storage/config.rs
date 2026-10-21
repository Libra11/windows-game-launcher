use super::*;

// 仅接受 OSS 地域根域；禁止 bucket 重复拼接、CNAME 与公网链接误用内网域名。
pub(super) fn endpoint(value: &str, bucket: &str, region: &str, internal: bool) -> Result<Url> {
    let raw = if value.contains("://") {
        value.to_owned()
    } else {
        format!("https://{value}")
    };
    let mut url = Url::parse(&raw).map_err(|_| unavailable())?;
    let expected = format!(
        "oss-{region}{}.aliyuncs.com",
        if internal { "-internal" } else { "" }
    );
    if url.scheme() != "https"
        || url.host_str() != Some(expected.as_str())
        || url.port().is_some()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(unavailable());
    }
    url.set_host(Some(&format!("{bucket}.{expected}")))
        .map_err(|_| unavailable())?;
    Ok(url)
}
pub(super) fn read(get: impl Fn(&str) -> Option<String>) -> Result<Option<Storage>> {
    const FIELDS: [&str; 7] = [
        "OSS_ENDPOINT",
        "OSS_INTERNAL_ENDPOINT",
        "OSS_BUCKET",
        "OSS_REGION",
        "OSS_ACCESS_KEY_ID",
        "OSS_ACCESS_KEY_SECRET",
        "OSS_SECURITY_TOKEN",
    ];
    let Some(public) = get("OSS_ENDPOINT") else {
        if FIELDS.iter().skip(1).any(|k| get(k).is_some()) {
            return Err(unavailable());
        }
        return Ok(None);
    };
    let required = |name| {
        get(name)
            .filter(|s| !s.is_empty() && s.trim() == s && !s.chars().any(char::is_control))
            .ok_or_else(unavailable)
    };
    let bucket = required("OSS_BUCKET")?;
    let region = required("OSS_REGION")?;
    let alnum = |b: u8| b.is_ascii_lowercase() || b.is_ascii_digit();
    if !(3..=63).contains(&bucket.len())
        || !bucket.bytes().all(|b| alnum(b) || b == b'-')
        || !alnum(bucket.as_bytes()[0])
        || !alnum(*bucket.as_bytes().last().unwrap())
        || region.starts_with("oss-")
        || region.len() > 64
        || !region.bytes().all(|b| alnum(b) || b == b'-')
    {
        return Err(unavailable());
    }
    let public_endpoint = endpoint(&public, &bucket, &region, false)?;
    let service_endpoint = match get("OSS_INTERNAL_ENDPOINT") {
        Some(value) if !value.is_empty() => endpoint(&value, &bucket, &region, true)?,
        _ => public_endpoint.clone(),
    };
    let security_token = get("OSS_SECURITY_TOKEN").filter(|s| !s.is_empty());
    if security_token
        .as_ref()
        .is_some_and(|s| s.trim() != s || s.chars().any(char::is_control))
    {
        return Err(unavailable());
    }
    let quota = match get("USER_QUOTA_BYTES") {
        Some(v) => v
            .parse::<u64>()
            .ok()
            .filter(|n| *n > 0 && *n <= i64::MAX as u64)
            .ok_or_else(unavailable)?,
        None => youji_cloud_protocol::saves::DEFAULT_QUOTA,
    };
    let object_prefix = get("OSS_PREFIX").unwrap_or_else(|| "games".into());
    if object_prefix.len() > 200
        || object_prefix.split('/').any(|part| {
            part.is_empty()
                || part == "."
                || part == ".."
                || !part
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        })
    {
        return Err(unavailable());
    }
    Ok(Some(Storage {
        object_prefix,
        public_endpoint,
        service_endpoint,
        bucket,
        region,
        access: required("OSS_ACCESS_KEY_ID")?,
        secret: required("OSS_ACCESS_KEY_SECRET")?,
        security_token,
        quota,
        client: client()?,
    }))
}
