use super::model::{Attempt,Cached,Identity};
use sha2::{Digest,Sha256};
use std::{io::Write,path::Path};

pub fn hash(bytes:&[u8])->String {format!("{:x}",Sha256::digest(bytes))}
fn decode(encoded:&str)->Result<String,String> {
    use base64::Engine;
    let bytes=base64::engine::general_purpose::STANDARD.decode(encoded).map_err(|_|"更新签名编码无效")?;
    String::from_utf8(bytes).map_err(|_|"更新签名编码无效".into())
}
pub fn signature(encoded:&str)->Result<minisign_verify::Signature,String> {
    minisign_verify::Signature::decode(&decode(encoded)?).map_err(|_|"更新签名格式无效".into())
}
pub fn verify(bytes:&[u8],encoded:&str,public:&str,version:&str)->Result<(),String> {
    let key=minisign_verify::PublicKey::decode(&decode(public)?).map_err(|_|"更新公钥配置无效")?;
    let signature=signature(encoded)?;
    key.verify(bytes,&signature,true).map_err(|_|"更新包签名验证失败，请重新下载")?;
    if let Some(signed)=signature.trusted_comment().split('\t').find_map(|field|field.strip_prefix("version:")) {
        let left=semver::Version::parse(signed.trim_start_matches('v')).map_err(|_|"签名版本无效")?;
        let right=semver::Version::parse(version.trim_start_matches('v')).map_err(|_|"更新版本无效")?;
        if left!=right{return Err("更新包签名版本与发布版本不一致".into());}
    }else{return Err("更新签名缺少版本绑定".into());}
    Ok(())
}
pub fn safe_file(file:&str)->bool {
    file.strip_suffix(".exe").is_some_and(|stem|uuid::Uuid::parse_str(stem).is_ok()) && !file.contains(['/', '\\'])
}
fn read_regular(path:&Path,limit:u64)->Result<Vec<u8>,String> {
    let metadata=std::fs::symlink_metadata(path).map_err(|e|e.to_string())?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len()>limit {return Err("更新缓存文件无效或超过限制".into());}
    std::fs::read(path).map_err(|e|e.to_string())
}
pub fn metadata(root:&Path)->Result<Option<Cached>,String> {
    let path=root.join("download.json");if !path.try_exists().map_err(|e|e.to_string())? {return Ok(None);}
    let bytes=read_regular(&path,64*1024)?;
    let value:Cached=serde_json::from_slice(&bytes).map_err(|_|"更新缓存索引无效")?;
    if !safe_file(&value.file) || value.sha256.len()!=64 || !value.sha256.chars().all(|ch|ch.is_ascii_hexdigit()) {return Err("更新缓存索引无效".into());}
    Ok(Some(value))
}
pub fn load(root:&Path,cached:&Cached,expected:&Identity,public:&str)->Result<Vec<u8>,String> {
    if &cached.identity!=expected || !safe_file(&cached.file) {return Err("下载包与当前更新不一致，请重新下载".into());}
    // 只读本模块创建的缓存文件，不能接受前端传入的可执行路径。
    let bytes=read_regular(&root.join(&cached.file),512*1024*1024)?;
    if bytes.len() as u64!=cached.size || hash(&bytes)!=cached.sha256 {return Err("更新缓存内容已变更，请重新下载".into());}
    verify(&bytes,&expected.signature,public,&expected.version)?;
    if !bytes.starts_with(b"MZ") {return Err("更新包不是 Windows NSIS 安装程序".into());}Ok(bytes)
}
pub fn atomic_json(path:&Path,value:&impl serde::Serialize)->Result<(),String> {
    let parent=path.parent().ok_or("更新缓存目录无效")?;std::fs::create_dir_all(parent).map_err(|e|e.to_string())?;
    let mut temp=tempfile::NamedTempFile::new_in(parent).map_err(|e|e.to_string())?;
    serde_json::to_writer(temp.as_file_mut(),value).map_err(|e|e.to_string())?;
    temp.as_file().sync_all().map_err(|e|e.to_string())?;temp.persist(path).map_err(|e|e.error.to_string())?;Ok(())
}
pub fn save(root:&Path,identity:Identity,bytes:&[u8],public:&str)->Result<Cached,String> {
    verify(bytes,&identity.signature,public,&identity.version)?;
    if bytes.len()>512*1024*1024 || !bytes.starts_with(b"MZ") {return Err("更新包格式无效或超过 512 MiB".into());}
    std::fs::create_dir_all(root).map_err(|e|e.to_string())?;
    let previous=metadata(root).ok().flatten();let file=format!("{}.exe",uuid::Uuid::new_v4());
    let mut temp=tempfile::NamedTempFile::new_in(root).map_err(|e|e.to_string())?;
    temp.write_all(bytes).map_err(|e|e.to_string())?;temp.as_file().sync_all().map_err(|e|e.to_string())?;
    temp.persist(root.join(&file)).map_err(|e|e.error.to_string())?;
    let cached=Cached{identity,file,sha256:hash(bytes),size:bytes.len() as u64};
    if let Err(error)=atomic_json(&root.join("download.json"),&cached){let _=std::fs::remove_file(root.join(&cached.file));return Err(error);}
    if let Some(old)=previous {if old.file!=cached.file&&safe_file(&old.file){let _=std::fs::remove_file(root.join(old.file));}}
    Ok(cached)
}
pub fn attempt(root:&Path)->Result<Option<Attempt>,String> {
    let path=root.join("install-attempt.json");if !path.try_exists().map_err(|e|e.to_string())? {return Ok(None);}
    serde_json::from_slice(&read_regular(&path,16*1024)?).map(Some).map_err(|_|"上次安装记录无效".into())
}
