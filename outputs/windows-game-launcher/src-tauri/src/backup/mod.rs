mod archive;
mod export;
mod model;
mod paths;
mod restore;
mod schema;
mod commands;

pub(crate) use commands::*;
pub(crate) use restore::{appearance_script, startup};
use std::{collections::HashMap, path::PathBuf, sync::{Mutex, MutexGuard, atomic::{AtomicBool, Ordering}}};

struct Prepared { path: PathBuf, sha256: String, loaded: archive::Loaded }
#[derive(Default)]
pub(crate) struct Manager {
    operation: Mutex<()>,
    prepared: Mutex<HashMap<String, Prepared>>,
    active: Mutex<Option<String>>,
    cancelled: AtomicBool,
    cancelable: AtomicBool,
}

struct Operation<'a> { manager: &'a Manager, _guard: MutexGuard<'a, ()> }
impl Manager {
    fn seal(&self) -> Result<(), String> {
        let _active = self.active.lock().map_err(|_| "备份操作状态不可用")?;
        if self.cancelled.load(Ordering::Acquire) { return Err("备份操作已取消".into()); }
        self.cancelable.store(false, Ordering::Release); Ok(())
    }
    fn begin(&self, id: &str) -> Result<Operation<'_>, String> {
        uuid::Uuid::parse_str(id).map_err(|_| "备份操作标识无效")?;
        let guard = self.operation.try_lock().map_err(|_| "另一项备份操作正在进行")?;
        *self.active.lock().map_err(|_| "备份操作状态不可用")? = Some(id.into());
        self.cancelled.store(false, Ordering::Release); self.cancelable.store(true, Ordering::Release);
        Ok(Operation { manager:self, _guard:guard })
    }
}
impl Drop for Operation<'_> {
    fn drop(&mut self) { if let Ok(mut active) = self.manager.active.lock() { *active = None; } }
}

#[cfg(test)]
mod tests;
