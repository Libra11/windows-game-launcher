use super::mpv::Mpv;
use serde_json::json;
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex, OnceLock,
    },
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Default)]
pub(super) struct Sample {
    pub path: String,
    pub position: f64,
    pub duration: f64,
    pub running: bool,
    pub stopped: bool,
    pub buffering: bool,
    pub seeking: bool,
    pub speed: Option<f64>,
    pub at: f64,
    pub end_reason: Option<String>,
}
pub(super) enum Adapter {
    Pot(PotPlayer),
    Mpv(Mpv),
}
impl Adapter {
    pub fn connect(kind: &str, pid: u32, pipe: &str) -> Result<Self, String> {
        if kind == "mpv" {
            Ok(Self::Mpv(Mpv::connect(pipe)?))
        } else {
            Ok(Self::Pot(PotPlayer::connect(pid)?))
        }
    }
    pub fn sample(&mut self) -> Result<Option<Sample>, String> {
        match self {
            Self::Pot(player) => player.sample().map(Some),
            Self::Mpv(player) => player.sample(),
        }
    }
    pub fn playlist(
        &mut self,
        paths: &[String],
        position: f64,
        start_index: usize,
    ) -> Result<(), String> {
        match self {
            Self::Mpv(player) => player.playlist(paths, position, start_index),
            Self::Pot(_) => Ok(()),
        }
    }
    pub fn wait(&self, poll_interval: Duration) {
        if matches!(self, Self::Pot(_)) {
            std::thread::sleep(poll_interval);
        }
    }
    pub fn close(&mut self) {
        match self {
            Self::Pot(player) => player.close(),
            Self::Mpv(player) => player.close(),
        }
    }
    pub fn seek(&mut self, position: f64) -> Result<(), String> {
        if !position.is_finite() || position < 0.0 {
            return Err("续播位置无效".into());
        }
        match self {
            Self::Pot(player) => {
                if !player.owned() {
                    return Err("播放器窗口已退出".into());
                }
                query(player.hwnd, 0x5005, (position * 1000.0).round() as isize).map(|_| ())
            }
            Self::Mpv(player) => player.command(json!(["seek", position, "absolute+exact"])),
        }
    }
}

type Hwnd = isize;
#[repr(C)]
#[derive(Default)]
struct Point {
    x: i32,
    y: i32,
}
#[repr(C)]
#[derive(Default)]
struct Message {
    hwnd: Hwnd,
    message: u32,
    wparam: usize,
    lparam: isize,
    time: u32,
    point: Point,
    private: u32,
}
#[repr(C)]
struct WindowClass {
    style: u32,
    proc: Option<unsafe extern "system" fn(Hwnd, u32, usize, isize) -> isize>,
    class_extra: i32,
    window_extra: i32,
    instance: isize,
    icon: isize,
    cursor: isize,
    background: isize,
    menu: *const u16,
    name: *const u16,
}
#[repr(C)]
struct CopyData {
    kind: usize,
    bytes: u32,
    data: *const u8,
}
#[link(name = "user32")]
unsafe extern "system" {
    fn EnumWindows(callback: unsafe extern "system" fn(Hwnd, isize) -> i32, param: isize) -> i32;
    fn GetWindowThreadProcessId(hwnd: Hwnd, pid: *mut u32) -> u32;
    fn GetClassNameW(hwnd: Hwnd, class: *mut u16, count: i32) -> i32;
    fn IsWindow(hwnd: Hwnd) -> i32;
    fn RegisterClassW(class: *const WindowClass) -> u16;
    fn UnregisterClassW(name: *const u16, instance: isize) -> i32;
    fn CreateWindowExW(
        extended: u32,
        class: *const u16,
        name: *const u16,
        style: u32,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        parent: Hwnd,
        menu: isize,
        instance: isize,
        param: *mut std::ffi::c_void,
    ) -> Hwnd;
    fn DestroyWindow(hwnd: Hwnd) -> i32;
    fn DefWindowProcW(hwnd: Hwnd, message: u32, wparam: usize, lparam: isize) -> isize;
    fn PeekMessageW(message: *mut Message, hwnd: Hwnd, min: u32, max: u32, remove: u32) -> i32;
    fn TranslateMessage(message: *const Message) -> i32;
    fn DispatchMessageW(message: *const Message) -> isize;
    fn SendMessageTimeoutW(
        hwnd: Hwnd,
        message: u32,
        wparam: usize,
        lparam: isize,
        flags: u32,
        timeout: u32,
        result: *mut usize,
    ) -> isize;
    fn PostMessageW(hwnd: Hwnd, message: u32, wparam: usize, lparam: isize) -> i32;
}
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetModuleHandleW(name: *const u16) -> isize;
    fn QueryUnbiasedInterruptTime(value: *mut u64) -> i32;
}
pub(super) fn awake_seconds() -> f64 {
    let mut value = 0;
    unsafe {
        if QueryUnbiasedInterruptTime(&mut value) != 0 {
            return value as f64 / 10_000_000.0;
        }
    }
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_secs_f64()
}
fn receivers() -> &'static Mutex<HashMap<Hwnd, Arc<Mutex<Option<String>>>>> {
    static VALUES: OnceLock<Mutex<HashMap<Hwnd, Arc<Mutex<Option<String>>>>>> = OnceLock::new();
    VALUES.get_or_init(Default::default)
}
unsafe extern "system" fn receive(hwnd: Hwnd, message: u32, wparam: usize, lparam: isize) -> isize {
    if message == 0x004a && lparam != 0 {
        let data = unsafe { &*(lparam as *const CopyData) };
        if data.kind == 0x6020 && data.bytes > 0 && data.bytes <= 65536 && !data.data.is_null() {
            let bytes = unsafe { std::slice::from_raw_parts(data.data, data.bytes as usize) };
            if let Ok(path) = std::str::from_utf8(bytes) {
                if let Ok(map) = receivers().lock() {
                    if let Some(value) = map.get(&hwnd) {
                        if let Ok(mut value) = value.lock() {
                            *value = Some(path.trim_end_matches('\0').to_string());
                        }
                    }
                }
            }
            return 1;
        }
    }
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}
struct Receiver {
    hwnd: Hwnd,
    path: Arc<Mutex<Option<String>>>,
    stop: Arc<AtomicBool>,
}
impl Receiver {
    fn new() -> Result<Self, String> {
        let (tx, rx) = mpsc::channel();
        let path = Arc::new(Mutex::new(None));
        let value = path.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        std::thread::spawn(move || unsafe {
            let class: Vec<u16> = format!("YoujiMediaReceiver-{}", uuid::Uuid::new_v4())
                .encode_utf16()
                .chain([0])
                .collect();
            let instance = GetModuleHandleW(std::ptr::null());
            let window_class = WindowClass {
                style: 0,
                proc: Some(receive),
                class_extra: 0,
                window_extra: 0,
                instance,
                icon: 0,
                cursor: 0,
                background: 0,
                menu: std::ptr::null(),
                name: class.as_ptr(),
            };
            if RegisterClassW(&window_class) == 0 {
                let _ = tx.send(Err("无法创建播放器消息接收器".to_string()));
                return;
            }
            let hwnd = CreateWindowExW(
                0,
                class.as_ptr(),
                class.as_ptr(),
                0,
                0,
                0,
                0,
                0,
                -3,
                0,
                instance,
                std::ptr::null_mut(),
            );
            if hwnd == 0 {
                UnregisterClassW(class.as_ptr(), instance);
                let _ = tx.send(Err("无法创建播放器消息接收窗口".to_string()));
                return;
            }
            if let Ok(mut map) = receivers().lock() {
                map.insert(hwnd, value);
            }
            let _ = tx.send(Ok(hwnd));
            while !stopping.load(Ordering::Acquire) {
                let mut message = Message::default();
                while PeekMessageW(&mut message, 0, 0, 0, 1) != 0 {
                    TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            if let Ok(mut map) = receivers().lock() {
                map.remove(&hwnd);
            }
            DestroyWindow(hwnd);
            UnregisterClassW(class.as_ptr(), instance);
        });
        let hwnd = rx
            .recv_timeout(Duration::from_secs(5))
            .map_err(|_| "播放器接收器初始化超时")??;
        Ok(Self { hwnd, path, stop })
    }
}
impl Drop for Receiver {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
    }
}
struct FindWindow {
    pid: u32,
    hwnd: Hwnd,
}
unsafe extern "system" fn find_window(hwnd: Hwnd, param: isize) -> i32 {
    let find = unsafe { &mut *(param as *mut FindWindow) };
    let mut pid = 0;
    unsafe {
        GetWindowThreadProcessId(hwnd, &mut pid);
    }
    if pid != find.pid {
        return 1;
    }
    let mut class = [0u16; 256];
    let length = unsafe { GetClassNameW(hwnd, class.as_mut_ptr(), class.len() as i32) };
    if length > 0
        && String::from_utf16_lossy(&class[..length as usize])
            .to_ascii_lowercase()
            .contains("potplayer")
    {
        find.hwnd = hwnd;
        return 0;
    }
    1
}
fn query(hwnd: Hwnd, command: usize, param: isize) -> Result<usize, String> {
    let mut result = 0;
    let success = unsafe { SendMessageTimeoutW(hwnd, 0x0400, command, param, 2, 350, &mut result) };
    if success == 0 {
        Err("播放器查询超时或不可用".into())
    } else {
        Ok(result)
    }
}
pub(super) struct PotPlayer {
    pid: u32,
    hwnd: Hwnd,
    receiver: Receiver,
}
impl PotPlayer {
    fn connect(pid: u32) -> Result<Self, String> {
        let start = Instant::now();
        let mut find = FindWindow { pid, hwnd: 0 };
        while start.elapsed() < Duration::from_secs(12) {
            unsafe {
                EnumWindows(find_window, &mut find as *mut _ as isize);
            }
            if find.hwnd != 0 {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        if find.hwnd == 0 {
            return Err("未找到本次启动的 PotPlayer 窗口，未记录观看时间".into());
        }
        Ok(Self {
            pid,
            hwnd: find.hwnd,
            receiver: Receiver::new()?,
        })
    }
    fn owned(&self) -> bool {
        let mut pid = 0;
        unsafe {
            IsWindow(self.hwnd) != 0
                && GetWindowThreadProcessId(self.hwnd, &mut pid) != 0
                && pid == self.pid
        }
    }
    fn sample(&mut self) -> Result<Sample, String> {
        if !self.owned() {
            return Err("本次播放器窗口已退出".into());
        }
        if let Ok(mut path) = self.receiver.path.lock() {
            *path = None;
        }
        query(self.hwnd, 0x6020, self.receiver.hwnd)?;
        let path = self
            .receiver
            .path
            .lock()
            .ok()
            .and_then(|v| v.clone())
            .filter(|s| !s.is_empty())
            .ok_or("播放器未返回当前文件身份，已暂停计时")?;
        let position = query(self.hwnd, 0x5004, 0)? as f64 / 1000.0;
        let duration = query(self.hwnd, 0x5002, 0)? as f64 / 1000.0;
        let state = query(self.hwnd, 0x5006, 0)?;
        let speed = query(self.hwnd, 0x5015, 0)
            .ok()
            .filter(|value| *value >= 200 && *value <= 12000)
            .map(|value| value as f64 / 1000.0);
        if let Ok(mut value) = self.receiver.path.lock() {
            *value = None;
        }
        query(self.hwnd, 0x6020, self.receiver.hwnd)?;
        let confirmed = self
            .receiver
            .path
            .lock()
            .ok()
            .and_then(|value| value.clone());
        if confirmed.as_deref() != Some(path.as_str()) {
            return Err("播放器正在切换文件，已暂停计时".into());
        }
        if position > 31_536_000.0 || duration > 31_536_000.0 || state > 2 {
            return Err("播放器返回的播放参数无效".into());
        }
        Ok(Sample {
            path,
            position,
            duration,
            running: state == 2,
            stopped: state == 0,
            buffering: false,
            seeking: false,
            speed,
            at: awake_seconds(),
            end_reason: None,
        })
    }
    fn close(&self) {
        if self.owned() {
            unsafe {
                PostMessageW(self.hwnd, 0x0010, 0, 0);
            }
        }
    }
}
