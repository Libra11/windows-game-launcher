// 设置表只保存不透明凭据引用；随机引用避免 SQLite 回滚覆盖旧凭据。
// keyring 的 Windows password 使用 UTF-16：960 原始字节经 Base64 后
// 为 1280 个 ASCII 字符，即 CRED_MAX_CREDENTIAL_BLOB_SIZE 的 2560 字节。
#[cfg(any(windows,test))]
const CREDENTIAL_CHUNK_BYTES: usize = 960;
#[cfg(all(windows,not(test)))]
pub fn sensitive(key:&str)->bool {["steam_api_key","steam_family_session","epic_oauth_session"].contains(&key)}
#[cfg(all(windows,not(test)))]
pub fn encode(key:&str,value:&str)->Result<String,String>{
    if value.is_empty(){return Ok(String::new());}
    let id=uuid::Uuid::new_v4().to_string();
    use base64::Engine;
    let chunks=value.as_bytes().chunks(CREDENTIAL_CHUNK_BYTES).collect::<Vec<_>>();
    if chunks.len()>128{return Err("平台凭据超过大小限制".into());}
    for (index,bytes) in chunks.iter().enumerate(){
        super::credentials::write("local-platform",&format!("{key}:{id}:{index}"),&base64::engine::general_purpose::STANDARD.encode(bytes))?;
    }
    Ok(format!("credential:{id}:{}",chunks.len()))
}
#[cfg(all(windows,not(test)))]
pub fn decode(key:&str,value:String)->Result<String,String>{
    use base64::Engine;
    let Some(reference)=value.strip_prefix("credential:") else{return Ok(value);};
    let(id,count)=reference.rsplit_once(':').ok_or("凭据引用无效")?;
    uuid::Uuid::parse_str(id).map_err(|_|"凭据引用无效")?;
    let count:usize=count.parse().map_err(|_|"凭据分片无效")?;
    if count==0||count>128{return Err("凭据分片数量无效".into());}
    let mut bytes=Vec::new();
    for index in 0..count{
        let part=super::credentials::read("local-platform",&format!("{key}:{id}:{index}"))?;
        bytes.extend(base64::engine::general_purpose::STANDARD.decode(part).map_err(|_|"凭据内容无效")?);
    }
    String::from_utf8(bytes).map_err(|_|"凭据内容无效".into())
}
pub fn migrate(c:&rusqlite::Connection)->Result<(),String>{
    #[cfg(all(windows,not(test)))]
    for key in ["steam_api_key","steam_family_session","epic_oauth_session"]{
        let raw:Option<String>=rusqlite::OptionalExtension::optional(c.query_row("SELECT value FROM settings WHERE key=?1",[key],|r|r.get(0))).map_err(|e|e.to_string())?;
        if let Some(value)=raw.filter(|s|!s.is_empty()&&!s.starts_with("credential:")){
            // 先写入并读回验证，成功后才替换数据库中的原值。
            crate::db::set_setting(c,key,&value)?;
        }
    }
    let _=c;Ok(())
}

#[cfg(test)]
mod tests {
    use super::CREDENTIAL_CHUNK_BYTES;
    use base64::{engine::general_purpose::STANDARD, Engine};

    #[test]
    fn platform_credentials_fit_windows_password_limit_and_roundtrip() {
        for value in ["x".repeat(5553), "登录凭据🔑".repeat(700), "x".repeat(960), "x".repeat(961)] {
            let mut restored = Vec::new();
            for chunk in value.as_bytes().chunks(CREDENTIAL_CHUNK_BYTES) {
                let password = STANDARD.encode(chunk);
                assert!(password.encode_utf16().count() * 2 <= 2560);
                restored.extend(STANDARD.decode(password).unwrap());
            }
            assert_eq!(String::from_utf8(restored).unwrap(), value);
        }
        // 原分片即使全为 ASCII，也超过 Windows 凭据上限。
        assert!(STANDARD.encode(vec![b'x'; 1536]).encode_utf16().count() * 2 > 2560);
    }
}
