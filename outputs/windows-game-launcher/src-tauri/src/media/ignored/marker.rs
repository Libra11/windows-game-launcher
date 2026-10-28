use super::{path_key, Rule, MARKER_FILE};
use std::{
    ffi::OsStr,
    fs,
    io::ErrorKind,
    path::{Component, Path, PathBuf},
};

fn lexical(path: &Path) -> Option<PathBuf> {
    let mut result = PathBuf::new();
    for component in path.components() {
        // Windows 扩展路径可能把点组件保留为 Normal，仍须按目录边界折叠。
        if component.as_os_str() == OsStr::new(".") {
            continue;
        }
        if component.as_os_str() == OsStr::new("..") {
            if !result.pop() {
                return None;
            }
            continue;
        }
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !result.pop() {
                    return None;
                }
            }
            component => result.push(component.as_os_str()),
        }
    }
    Some(result)
}

/// 只检查标记本身，不读取内容、枚举子目录或跟随链接。
fn marker_exists(directory: &Path) -> Result<bool, String> {
    match fs::symlink_metadata(directory.join(MARKER_FILE)) {
        Ok(metadata) => Ok(metadata.is_file() || metadata.file_type().is_symlink()),
        Err(error) if matches!(error.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory) => {
            Ok(false)
        }
        Err(_) => Err("无法检查目录忽略标记，已暂停访问该路径".into()),
    }
}

/// 路径既可为文件也可为目录；检查范围限定为媒体源根目录及其子树。
/// 用逐级 stat 支持扫描期间新放入的标记，不缓存“没有标记”的结果。
pub(in crate::media) fn directory_marked(root: &Path, path: &Path) -> Result<bool, String> {
    let root = lexical(root).ok_or("媒体源路径无效")?;
    let path = lexical(path).ok_or("媒体路径无效")?;
    let boundary = Rule {
        path: root.to_string_lossy().into_owned(),
        directory: true,
    };
    if !boundary.matches(&path.to_string_lossy()) {
        return Ok(false);
    }
    let root_key = path_key(&root.to_string_lossy());
    for directory in path.ancestors() {
        if !boundary.matches(&directory.to_string_lossy()) {
            break;
        }
        if marker_exists(directory)? {
            return Ok(true);
        }
        if path_key(&directory.to_string_lossy()) == root_key {
            break;
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marker_is_bounded_to_its_source_and_matches_case_insensitively() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("Media");
        let skipped = root.join("Windows.old");
        let sibling = root.join("Windows.old2");
        fs::create_dir_all(skipped.join("Nested")).unwrap();
        fs::create_dir_all(&sibling).unwrap();
        fs::write(skipped.join(".YOUJI-IGNORE"), b"").unwrap();
        assert!(directory_marked(&root, &skipped).unwrap());
        assert!(directory_marked(&root, &skipped.join("Nested/movie.mkv")).unwrap());
        assert!(!directory_marked(&root, &sibling.join("movie.mkv")).unwrap());
        let outside = root.join("../outside/movie.mkv");
        fs::write(temp.path().join(MARKER_FILE), b"").unwrap();
        assert!(!directory_marked(&root, &outside).unwrap());
        assert!(!directory_marked(&root, &root.join("movie.mkv")).unwrap());
        assert!(directory_marked(
            &root,
            &PathBuf::from(root.to_string_lossy().to_uppercase())
                .join("WINDOWS.OLD/NESTED/MOVIE.MKV")
        )
        .unwrap());
    }

    #[test]
    fn only_a_marker_file_excludes_a_directory_and_root_marker_is_supported() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join(".youji-ignore.txt"), b"").unwrap();
        assert!(!directory_marked(root.path(), &root.path().join("movie.mkv")).unwrap());
        fs::create_dir(root.path().join(MARKER_FILE)).unwrap();
        assert!(!directory_marked(root.path(), root.path()).unwrap());
        fs::remove_dir(root.path().join(MARKER_FILE)).unwrap();
        fs::write(root.path().join(MARKER_FILE), b"content is not interpreted").unwrap();
        assert!(directory_marked(root.path(), root.path()).unwrap());
        assert!(directory_marked(root.path(), &root.path().join("missing/sub/movie.mkv")).unwrap());
    }
}
