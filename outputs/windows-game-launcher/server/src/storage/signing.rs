// OSS V4 原生签名，不使用 S3 的 scope、规范查询或头部规则。
// https://www.alibabacloud.com/help/en/oss/developer-reference/add-signatures-to-urls
use super::*;
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

pub(super) fn encode(value: &str) -> String {
    value
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}
fn mac(key: &[u8], value: &str) -> Vec<u8> {
    let mut h = Hmac::<Sha256>::new_from_slice(key).expect("HMAC 支持任意长度密钥");
    h.update(value.as_bytes());
    h.finalize().into_bytes().to_vec()
}
pub(super) struct Request<'a> {
    pub method: &'a str,
    pub key: &'a str,
    pub query: &'a [(&'a str, &'a str)],
    pub expires: u32,
    pub headers: BTreeMap<String, String>,
}
impl Storage {
    pub(super) fn presign(
        &self,
        endpoint: &Url,
        request: Request<'_>,
        now: chrono::DateTime<Utc>,
    ) -> Result<(String, BTreeMap<String, String>)> {
        let mut request = request;
        request.headers.insert(
            "host".into(),
            endpoint[url::Position::BeforeHost..url::Position::AfterPort].into(),
        );
        self.presign_headers(endpoint, request, now)
    }
    // 签名核心接受明确的待签名头，生产入口始终固定 host。
    pub(super) fn presign_headers(
        &self,
        endpoint: &Url,
        request: Request<'_>,
        now: chrono::DateTime<Utc>,
    ) -> Result<(String, BTreeMap<String, String>)> {
        let Request {
            method,
            key,
            query,
            expires,
            mut headers,
        } = request;
        if expires == 0
            || expires
                > if self.security_token.is_some() {
                    43200
                } else {
                    604800
                }
            || key.starts_with('/')
            || key.split('/').any(|p| p == "." || p == "..")
            || key.chars().any(char::is_control)
        {
            return Err(unavailable());
        }
        let date = now.format("%Y%m%d").to_string();
        let time = now.format("%Y%m%dT%H%M%SZ").to_string();
        let scope = format!("{date}/{}/oss/aliyun_v4_request", self.region);
        // Content-Type、Content-MD5、x-oss-* 自动参与签名，不列入 AdditionalHeaders。
        let additional = headers
            .keys()
            .filter(|k| {
                !k.starts_with("x-oss-") && !matches!(k.as_str(), "content-type" | "content-md5")
            })
            .cloned()
            .collect::<Vec<_>>()
            .join(";");
        let mut pairs: Vec<(String, String)> =
            query.iter().map(|(k, v)| (encode(k), encode(v))).collect();
        pairs.extend(
            [
                ("x-oss-signature-version", "OSS4-HMAC-SHA256".into()),
                ("x-oss-credential", format!("{}/{}", self.access, scope)),
                ("x-oss-date", time.clone()),
                ("x-oss-expires", expires.to_string()),
            ]
            .into_iter()
            .map(|(k, v)| (encode(k), encode(&v))),
        );
        if !additional.is_empty() {
            pairs.push(("x-oss-additional-headers".into(), encode(&additional)));
        }
        if let Some(token) = &self.security_token {
            pairs.push(("x-oss-security-token".into(), encode(token)));
        }
        // OSS 无值子资源只写 key，不能写 key=。
        pairs.sort_by(|a, b| a.0.cmp(&b.0));
        let canonical_query = pairs
            .iter()
            .map(|(k, v)| {
                if v.is_empty() {
                    k.clone()
                } else {
                    format!("{k}={v}")
                }
            })
            .collect::<Vec<_>>()
            .join("&");
        let path = key.split('/').map(encode).collect::<Vec<_>>().join("/");
        let canonical_headers = headers
            .iter()
            .map(|(k, v)| format!("{k}:{}\n", v.trim()))
            .collect::<String>();
        // virtual hosted 的实际 URL 不含 bucket，规范 URI 仍须包含 bucket。
        let canonical=format!("{method}\n/{}/{path}\n{canonical_query}\n{canonical_headers}\n{additional}\nUNSIGNED-PAYLOAD",self.bucket);
        let to_sign = format!(
            "OSS4-HMAC-SHA256\n{time}\n{scope}\n{}",
            hex::encode(Sha256::digest(canonical))
        );
        let date_key = mac(format!("aliyun_v4{}", self.secret).as_bytes(), &date);
        let region_key = mac(&date_key, &self.region);
        let service_key = mac(&region_key, "oss");
        let signing_key = mac(&service_key, "aliyun_v4_request");
        let signature = hex::encode(mac(&signing_key, &to_sign));
        headers.remove("host");
        Ok((
            format!(
                "{}/{path}?{canonical_query}&x-oss-signature={signature}",
                endpoint.as_str().trim_end_matches('/')
            ),
            headers,
        ))
    }
}
