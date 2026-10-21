// 只在 Windows 凭据管理器保存 refresh token，其他平台明确不可用。
#[cfg(windows)]
fn entry(base:&str,user:&str)->Result<keyring::Entry,String>{
    keyring::Entry::new(&format!("youji-cloud-{}",youji_cloud_protocol::digest(&base)?),user).map_err(|_|"无法打开 Windows 凭据管理器".into())
}
pub fn read(base:&str,user:&str)->Result<String,String>{
    #[cfg(windows)] {entry(base,user)?.get_password().map_err(|_|"登录已失效，请重新登录".into())}
    #[cfg(not(windows))] {let _=(base,user);Err("云账号凭据仅支持 Windows 凭据管理器".into())}
}
pub fn write(base:&str,user:&str,token:&str)->Result<(),String>{
    #[cfg(windows)] {let e=entry(base,user)?;e.set_password(token).map_err(|_|"保存登录凭据失败")?;if e.get_password().map_err(|_|"校验登录凭据失败")? != token{return Err("登录凭据校验失败".into());}Ok(())}
    #[cfg(not(windows))] {let _=(base,user,token);Err("云账号凭据仅支持 Windows 凭据管理器".into())}
}
pub fn delete(base:&str,user:&str)->Result<(),String>{
    #[cfg(windows)] {entry(base,user)?.delete_credential().map_err(|_|"清除登录凭据失败".into())}
    #[cfg(not(windows))] {let _=(base,user);Err("云账号凭据仅支持 Windows 凭据管理器".into())}
}
