use ::windows::{
    core::{Error, HRESULT, PCWSTR},
    Win32::{
        Foundation::{CloseHandle, ERROR_CANCELLED, RPC_E_CHANGED_MODE},
        System::{
            Com::{
                CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
            },
            Threading::GetProcessId,
        },
        UI::{
            Shell::{
                ShellExecuteExW, SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS,
                SHELLEXECUTEINFOW,
            },
            WindowsAndMessaging::SW_SHOWNORMAL,
        },
    },
};
use std::{os::windows::ffi::OsStrExt, path::Path};

struct ComGuard(bool);
impl Drop for ComGuard {
    fn drop(&mut self) {
        if self.0 {
            unsafe { CoUninitialize() }
        }
    }
}

pub(super) fn elevated(exe: &Path, directory: &Path) -> Result<u32, String> {
    let initialized =
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) };
    let _com = if initialized == RPC_E_CHANGED_MODE {
        ComGuard(false)
    } else {
        initialized
            .ok()
            .map_err(|e| format!("无法准备游戏授权启动：{e}"))?;
        ComGuard(true)
    };
    let wide = |path: &Path| {
        path.as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>()
    };
    let exe = wide(exe);
    let directory = wide(directory);
    let operation: Vec<u16> = "runas\0".encode_utf16().collect();
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        // 同步返回新进程句柄；只抑制重复的错误窗口，保留系统 UAC 确认。
        fMask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI,
        lpVerb: PCWSTR(operation.as_ptr()),
        lpFile: PCWSTR(exe.as_ptr()),
        lpDirectory: PCWSTR(directory.as_ptr()),
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };
    unsafe { ShellExecuteExW(&mut info) }.map_err(launch_error)?;
    if info.hProcess.is_invalid() {
        return Err("游戏启动请求已发送，但未获得进程句柄，请确认游戏是否已启动".into());
    }
    let pid = unsafe { GetProcessId(info.hProcess) };
    let error = (pid == 0).then(std::io::Error::last_os_error);
    // 运行检测使用 PID，立即释放本组件取得的句柄，不终止游戏进程。
    let _ = unsafe { CloseHandle(info.hProcess) };
    match error {
        Some(error) => Err(format!("游戏启动请求已发送，但无法获取进程编号：{error}")),
        None => Ok(pid),
    }
}

fn launch_error(error: Error) -> String {
    if error.code() == HRESULT::from_win32(ERROR_CANCELLED.0) {
        "已取消管理员授权，游戏未启动".into()
    } else {
        format!("无法以管理员权限启动游戏：{error}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancelling_uac_reports_cancellation_instead_of_launch_failure() {
        let error = Error::from_hresult(HRESULT::from_win32(ERROR_CANCELLED.0));
        assert_eq!(launch_error(error), "已取消管理员授权，游戏未启动");
    }
}
