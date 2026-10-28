//! Windows 双向命名管道必须采用 OVERLAPPED I/O，阻塞读不能锁住同一连接的写入。
use std::{
    ffi::c_void,
    io::{self, Read, Write},
    sync::Arc,
    time::{Duration, Instant},
};

type Handle = isize;
#[repr(C)]
#[derive(Default)]
struct Overlapped {
    internal: usize,
    internal_high: usize,
    offset: u32,
    offset_high: u32,
    event: Handle,
}
#[link(name = "kernel32")]
unsafe extern "system" {
    fn CreateFileW(
        name: *const u16,
        access: u32,
        share: u32,
        security: *const c_void,
        creation: u32,
        flags: u32,
        template: Handle,
    ) -> Handle;
    fn CreateEventW(security: *const c_void, manual: i32, initial: i32, name: *const u16)
        -> Handle;
    fn ReadFile(
        file: Handle,
        buffer: *mut u8,
        count: u32,
        read: *mut u32,
        overlapped: *mut Overlapped,
    ) -> i32;
    fn WriteFile(
        file: Handle,
        buffer: *const u8,
        count: u32,
        written: *mut u32,
        overlapped: *mut Overlapped,
    ) -> i32;
    fn GetOverlappedResult(
        file: Handle,
        overlapped: *mut Overlapped,
        count: *mut u32,
        wait: i32,
    ) -> i32;
    fn WaitForSingleObject(handle: Handle, millis: u32) -> u32;
    fn CancelIoEx(file: Handle, overlapped: *mut Overlapped) -> i32;
    fn CloseHandle(handle: Handle) -> i32;
}
struct OwnedHandle(Handle);
impl Drop for OwnedHandle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}

#[derive(Clone)]
pub(super) struct Pipe(Arc<OwnedHandle>);
impl Pipe {
    pub fn connect(path: &str) -> io::Result<Self> {
        let name: Vec<u16> = path.encode_utf16().chain([0]).collect();
        let start = Instant::now();
        loop {
            let handle = unsafe {
                CreateFileW(
                    name.as_ptr(),
                    0xc0000000,
                    0,
                    std::ptr::null(),
                    3,
                    0x40000000,
                    0,
                )
            };
            if handle != -1 {
                return Ok(Self(Arc::new(OwnedHandle(handle))));
            }
            if start.elapsed() > Duration::from_secs(12) {
                return Err(io::Error::last_os_error());
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    fn transfer(&self, buffer: *mut u8, count: usize, write: bool) -> io::Result<usize> {
        let event = OwnedHandle(unsafe { CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) });
        if event.0 == 0 {
            return Err(io::Error::last_os_error());
        }
        let mut op = Overlapped {
            event: event.0,
            ..Default::default()
        };
        let mut transferred = 0;
        let result = unsafe {
            if write {
                WriteFile(
                    self.0 .0,
                    buffer,
                    count.min(u32::MAX as usize) as u32,
                    &mut transferred,
                    &mut op,
                )
            } else {
                ReadFile(
                    self.0 .0,
                    buffer,
                    count.min(u32::MAX as usize) as u32,
                    &mut transferred,
                    &mut op,
                )
            }
        };
        if result != 0 {
            return Ok(transferred as usize);
        }
        let error = io::Error::last_os_error();
        if error.raw_os_error() != Some(997) {
            return Err(error);
        }
        let waited = unsafe { WaitForSingleObject(event.0, if write { 3000 } else { u32::MAX }) };
        if waited != 0 {
            // 取消后等待完成，确保 OVERLAPPED 与调用方缓冲区仍有效。
            unsafe {
                CancelIoEx(self.0 .0, &mut op);
                GetOverlappedResult(self.0 .0, &mut op, &mut transferred, 1);
            }
            return Err(io::Error::new(io::ErrorKind::TimedOut, "mpv 管道写入超时"));
        }
        if unsafe { GetOverlappedResult(self.0 .0, &mut op, &mut transferred, 0) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(transferred as usize)
    }
}
impl Read for Pipe {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        self.transfer(buf.as_mut_ptr(), buf.len(), false)
    }
}
impl Write for Pipe {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        self.transfer(buf.as_ptr() as *mut u8, buf.len(), true)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
