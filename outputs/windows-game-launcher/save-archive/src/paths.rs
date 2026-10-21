use std::{fs, path::{Path, PathBuf}};
pub fn check_tree_path(path: &Path) -> Result<(), String> {
    // 检查祖先，拒绝通过 junction/symlink 绕过目录边界。
    for ancestor in path.ancestors() {
        if ancestor.as_os_str().is_empty() { continue; }
        let meta = fs::symlink_metadata(ancestor).map_err(|e| e.to_string())?;
        if meta.file_type().is_symlink() { return Err("不支持符号链接存档".into()); }
        #[cfg(windows)] {
            use std::os::windows::fs::MetadataExt;
            if meta.file_attributes() & 0x400 != 0 { return Err("不支持 Windows 重解析点存档".into()); }
        }
    }
    Ok(())
}
pub fn validate_root(root: &Path) -> Result<PathBuf, String> {
    if !root.is_absolute() { return Err("请选择绝对路径的专用存档目录".into()); }
    check_tree_path(root)?;
    let path = root.canonicalize().map_err(|e| e.to_string())?;
    if !path.is_dir() || path.parent().is_none() { return Err("不能使用磁盘根目录或普通文件".into()); }
    for key in ["USERPROFILE", "WINDIR", "SystemRoot", "ProgramFiles", "ProgramFiles(x86)", "PROGRAMDATA"] {
        if let Some(value) = std::env::var_os(key) {
            if let Ok(protected) = Path::new(&value).canonicalize() {
                let system = matches!(key, "WINDIR" | "SystemRoot" | "ProgramFiles" | "ProgramFiles(x86)");
                if path == protected || protected.starts_with(&path) || (system && path.starts_with(&protected)) {
                    return Err("请选择专用存档目录，不能使用用户根目录或系统目录".into());
                }
            }
        }
    }
    Ok(path)
}
