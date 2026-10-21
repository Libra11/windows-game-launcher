//! 客户端与服务端使用同一严格归档边界，不信任 ZIP 的路径、大小或属性。
mod paths;
mod reader;
mod writer;
pub use paths::{check_tree_path, validate_root};
pub use reader::{extract, verify};
pub use writer::create;
pub use youji_cloud_protocol::saves::{FileRecord, Manifest, UploadRequest, MAX_BYTES, MAX_FILES};
use sha2::{Digest, Sha256};
use std::{fs::File, io::Read, path::Path};
pub(crate) const MANIFEST: &str = "manifest.json";
pub(crate) const MANIFEST_LIMIT: u64 = 128 * 1024 * 1024;

pub fn file_hash(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let mut hash = Sha256::new();
    let mut buf = [0; 65536];
    let mut total = 0u64;
    loop {
        let n = file.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 { break; }
        total = total.checked_add(n as u64).filter(|n| *n <= MAX_BYTES).ok_or("文件超过 2 GiB")?;
        hash.update(&buf[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}
pub fn manifest_hash(manifest: &Manifest) -> Result<String, String> {
    youji_cloud_protocol::digest(manifest)
}
#[cfg(test)] mod tests;
#[cfg(feature = "http")]
pub mod transfer;
