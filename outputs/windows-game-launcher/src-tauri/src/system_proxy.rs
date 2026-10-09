use std::ffi::OsString;

// 与当前 reqwest/hyper-util 实际读取的来源一致。不持久化或打印代理凭据。
#[derive(Clone, Default, PartialEq, Eq)]
pub(crate) struct Snapshot {
    pub(crate) environment: [Option<OsString>; 9],
    #[cfg(windows)]
    pub(crate) windows: WindowsProxy,
}

#[cfg(windows)]
#[derive(Clone, Default, PartialEq, Eq)]
pub(crate) struct WindowsProxy {
    pub(crate) enabled: bool,
    pub(crate) server: Option<String>,
    pub(crate) bypass: Option<String>,
}

impl Snapshot {
    pub(crate) fn read() -> Self {
        const VARIABLES: [&str; 9] = [
            "ALL_PROXY",
            "all_proxy",
            "HTTP_PROXY",
            "http_proxy",
            "HTTPS_PROXY",
            "https_proxy",
            "NO_PROXY",
            "no_proxy",
            "REQUEST_METHOD",
        ];
        Self {
            environment: std::array::from_fn(|index| std::env::var_os(VARIABLES[index])),
            #[cfg(windows)]
            windows: WindowsProxy::read(),
        }
    }
}

#[cfg(windows)]
impl WindowsProxy {
    fn read() -> Self {
        use winreg::{enums::HKEY_CURRENT_USER, RegKey};
        let Ok(key) = RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Internet Settings")
        else {
            return Self::default();
        };
        if key.get_value::<u32, _>("ProxyEnable").unwrap_or(0) == 0 {
            return Self::default();
        }
        Self {
            enabled: true,
            server: key.get_value("ProxyServer").ok(),
            bypass: key.get_value("ProxyOverride").ok(),
        }
    }
}
