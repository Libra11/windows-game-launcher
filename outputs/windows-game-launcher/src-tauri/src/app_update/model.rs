use serde::{Deserialize, Serialize};

#[derive(Clone, Default, Serialize)]
#[serde(rename_all="camelCase")]
pub struct Status {
    pub revision:u64, pub download_ready:bool,
    pub supported: bool, pub reason: String, pub current_version: String, pub current_notes: String, pub auto_check: bool,
    pub phase: String, pub preparation_id: String, pub version: String, pub notes: String,
    pub published_at: String, pub operation_id: String, pub downloaded: u64, pub total: Option<u64>,
    pub error: String, pub cache_version: String, pub install_message: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct Identity { pub version:String, pub url:String, pub signature:String }
impl Identity {
    pub fn validate(&self,current:&str)->Result<(),String> {
        let version=semver::Version::parse(&self.version).map_err(|_|"更新版本号无效")?;
        let current=semver::Version::parse(current).map_err(|_|"当前版本号无效")?;
        if !version.pre.is_empty() || version<=current {return Err("更新不是较新的正式版本".into());}
        let url=reqwest::Url::parse(&self.url).map_err(|_|"更新地址无效")?;
        if url.scheme()!="https" || url.host_str()!=Some("github.com") || !url.username().is_empty() || url.password().is_some()
            || !url.path().starts_with("/Libra11/windows-game-launcher/releases/download/") || !url.path().ends_with("_x64-setup.exe")
            || url.query().is_some() || url.fragment().is_some()
        {return Err("更新包地址不属于游迹正式发布源".into());}
        super::cache::signature(&self.signature)?;Ok(())
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct Cached { pub identity:Identity, pub file:String, pub sha256:String, pub size:u64 }
#[derive(Serialize, Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct Attempt {pub from:String,pub to:String,pub started_at:String}
pub fn attempt_message(attempt:&Attempt,current:&str)->String {
    if current==attempt.to {format!("已更新至 {}",current)} else {format!("上次尝试安装 {} 尚未确认完成，当前版本为 {}。可重新检查并重试。",attempt.to,current)}
}
