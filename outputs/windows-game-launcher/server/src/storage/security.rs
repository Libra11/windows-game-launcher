use super::*;
pub(super) fn field(xml: &str, path: &[&str]) -> Result<String> {
    let doc = roxmltree::Document::parse(xml).map_err(|_| unavailable())?;
    let mut node = doc.root_element();
    if path.is_empty() || !node.has_tag_name(path[0]) {
        return Err(unavailable());
    }
    for name in &path[1..] {
        let mut nodes = node.children().filter(|n| n.has_tag_name(*name));
        node = nodes.next().ok_or_else(unavailable)?;
        if nodes.next().is_some() {
            return Err(unavailable());
        }
    }
    if node.children().any(|n| n.is_element()) {
        return Err(unavailable());
    }
    node.text()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .ok_or_else(unavailable)
}
impl Storage {
    async fn control(&self, query: &str) -> Result<String> {
        let (url, _) = self.presign(
            &self.service_endpoint,
            signing::Request {
                method: "GET",
                key: "",
                query: &[(query, "")],
                expires: 60,
                headers: BTreeMap::new(),
            },
            Utc::now(),
        )?;
        let mut response = self
            .client
            .get(url)
            .timeout(std::time::Duration::from_secs(15))
            .send()
            .await
            .map_err(|_| unavailable())?;
        if !response.status().is_success() {
            return Err(unavailable());
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| unavailable())? {
            if bytes.len() + chunk.len() > 65536 {
                return Err(unavailable());
            }
            bytes.extend_from_slice(&chunk);
        }
        String::from_utf8(bytes).map_err(|_| unavailable())
    }
    pub(super) async fn security_headers(&self) -> Result<BTreeMap<String, String>> {
        let acl = self.control("acl").await?;
        if field(&acl, &["AccessControlPolicy", "AccessControlList", "Grant"])? != "private" {
            return Err(unavailable());
        }
        let block = self.control("publicAccessBlock").await?;
        if field(
            &block,
            &["PublicAccessBlockConfiguration", "BlockPublicAccess"],
        )? != "true"
        {
            return Err(unavailable());
        }
        let policy = self.control("policyStatus").await?;
        if field(&policy, &["PolicyStatus", "IsPublic"])? != "false" {
            return Err(unavailable());
        }
        let xml = self.control("encryption").await?;
        let algorithm = field(
            &xml,
            &[
                "ServerSideEncryptionRule",
                "ApplyServerSideEncryptionByDefault",
                "SSEAlgorithm",
            ],
        )?;
        if !matches!(algorithm.as_str(), "AES256" | "KMS") {
            return Err(unavailable());
        }
        let mut headers = BTreeMap::from([
            ("x-oss-server-side-encryption".into(), algorithm.clone()),
            ("x-oss-object-acl".into(), "private".into()),
        ]);
        if algorithm == "KMS" {
            // KMSMasterKeyID 可缺省；若存在则精确保留，禁止静默覆盖为 AES256。
            let doc = roxmltree::Document::parse(&xml).map_err(|_| unavailable())?;
            if doc.descendants().any(|n| {
                n.has_tag_name("KMSMasterKeyID") && n.text().is_some_and(|s| !s.trim().is_empty())
            }) {
                let key = field(
                    &xml,
                    &[
                        "ServerSideEncryptionRule",
                        "ApplyServerSideEncryptionByDefault",
                        "KMSMasterKeyID",
                    ],
                )?;
                if key.chars().any(char::is_control) {
                    return Err(unavailable());
                }
                headers.insert("x-oss-server-side-encryption-key-id".into(), key);
            }
        }
        Ok(headers)
    }
    pub async fn check_private(&self) -> Result<()> {
        self.security_headers().await.map(|_| ())
    }
}
