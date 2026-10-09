use std::{path::Path, process::Command};

#[cfg(windows)]
mod windows;

pub(super) fn local(exe: &Path) -> Result<u32, String> {
    if !exe.is_file() {
        return Err("游戏启动文件不存在，请在编辑游戏中重新选择".into());
    }
    let directory = exe.parent().ok_or("游戏目录不存在")?;
    match Command::new(exe).current_dir(directory).spawn() {
        Ok(mut child) => {
            let pid = child.id();
            std::thread::spawn(move || {
                let _ = child.wait();
            });
            Ok(pid)
        }
        Err(error) => {
            #[cfg(windows)]
            {
                retry_elevated(error, || windows::elevated(exe, directory))
            }
            #[cfg(not(windows))]
            {
                Err(format!("无法启动游戏：{error}"))
            }
        }
    }
}

#[cfg(windows)]
fn retry_elevated(
    error: std::io::Error,
    elevate: impl FnOnce() -> Result<u32, String>,
) -> Result<u32, String> {
    use ::windows::Win32::Foundation::ERROR_ELEVATION_REQUIRED;
    if error.raw_os_error() == Some(ERROR_ELEVATION_REQUIRED.0 as i32) {
        elevate()
    } else {
        Err(format!("无法启动游戏：{error}"))
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use ::windows::Win32::Foundation::{ERROR_ACCESS_DENIED, ERROR_ELEVATION_REQUIRED};

    #[test]
    fn elevation_required_retries_and_returns_the_game_pid() {
        assert_eq!(
            retry_elevated(
                std::io::Error::from_raw_os_error(ERROR_ELEVATION_REQUIRED.0 as i32),
                || Ok(1234),
            ),
            Ok(1234)
        );
    }

    #[test]
    fn unrelated_start_errors_do_not_request_elevation() {
        let result = retry_elevated(
            std::io::Error::from_raw_os_error(ERROR_ACCESS_DENIED.0 as i32),
            || panic!("权限拒绝不应被当成需要提权"),
        );
        assert!(result.unwrap_err().contains("无法启动游戏"));
    }
}
