//! ffmpeg 与 ffprobe 共用程序定位规则；显式路径无效时不静默改用其他版本。
use std::path::{Path, PathBuf};

pub(super) fn discover(name: &str, configured: &str, companion: &str) -> Option<PathBuf> {
    let search = std::env::var_os("PATH");
    locate(name, configured, companion, search.as_deref())
}

fn locate(
    name: &str,
    configured: &str,
    companion: &str,
    search: Option<&std::ffi::OsStr>,
) -> Option<PathBuf> {
    if !configured.trim().is_empty() {
        let path = PathBuf::from(configured.trim());
        return path.is_file().then_some(path);
    }
    let sibling = Path::new(companion.trim())
        .parent()
        .map(|dir| dir.join(name));
    sibling.filter(|path| path.is_file()).or_else(|| {
        search
            .into_iter()
            .flat_map(std::env::split_paths)
            .map(|dir| dir.join(name))
            .chain([
                Path::new("C:/Program Files/Jellyfin/Server").join(name),
                Path::new("C:/Program Files/ffmpeg/bin").join(name),
                Path::new("C:/ffmpeg/bin").join(name),
            ])
            .find(|path| path.is_file())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn both_tools_share_explicit_sibling_and_path_rules() {
        let temp = tempfile::tempdir().unwrap();
        let explicit = temp.path().join("explicit");
        let sibling = temp.path().join("sibling");
        let searched = temp.path().join("searched");
        for dir in [&explicit, &sibling, &searched] {
            std::fs::create_dir(dir).unwrap();
            for name in ["ffmpeg.exe", "ffprobe.exe"] {
                std::fs::write(dir.join(name), []).unwrap();
            }
        }
        let search = std::env::join_paths([&searched]).unwrap();
        for (name, other) in [("ffmpeg.exe", "ffprobe.exe"), ("ffprobe.exe", "ffmpeg.exe")] {
            let configured = explicit.join(name);
            let companion = sibling.join(other);
            assert_eq!(
                locate(
                    name,
                    &configured.to_string_lossy(),
                    &companion.to_string_lossy(),
                    Some(&search)
                ),
                Some(configured)
            );
            assert_eq!(
                locate(name, "", &companion.to_string_lossy(), Some(&search)),
                Some(sibling.join(name))
            );
            assert_eq!(
                locate(name, "", "", Some(&search)),
                Some(searched.join(name))
            );
            assert_eq!(
                locate(
                    name,
                    "Z:/missing/tool.exe",
                    &companion.to_string_lossy(),
                    Some(&search)
                ),
                None
            );
        }
    }
}
