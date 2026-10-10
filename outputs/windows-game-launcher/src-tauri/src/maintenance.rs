use std::sync::{atomic::{AtomicBool,Ordering},Mutex,MutexGuard};

// 更新和恢复共享维护标记；失败离开作用域时自动恢复，已安排重启时保持维护。
pub(crate) struct Guard<'a>{flag:&'a AtomicBool,keep:bool}
impl<'a> Guard<'a>{
    pub fn enter(flag:&'a AtomicBool)->Result<Self,String>{
        flag.compare_exchange(false,true,Ordering::AcqRel,Ordering::Acquire).map_err(|_|"游迹正在维护，请稍后重试")?;
        Ok(Self{flag,keep:false})
    }
    pub fn keep_active(mut self){self.keep=true;}
}
impl Drop for Guard<'_>{fn drop(&mut self){if !self.keep{self.flag.store(false,Ordering::Release);}}}
pub(crate) fn lock_operation<'a>(gate:&'a Mutex<()>,flag:&AtomicBool)->Result<MutexGuard<'a,()>,String>{
    let guard=gate.lock().map_err(|_|"启动操作状态不可用")?;
    if flag.load(Ordering::Acquire){return Err("更新或恢复期间不能启动游戏或捕获".into());}
    Ok(guard)
}
#[cfg(test)]mod tests{
    use super::*;
    #[test]fn failed_maintenance_releases_flag_and_existing_owner_is_not_cleared(){
        let flag=AtomicBool::new(false);{let _guard=Guard::enter(&flag).unwrap();assert!(Guard::enter(&flag).is_err());assert!(flag.load(Ordering::Acquire));}
        assert!(!flag.load(Ordering::Acquire));let guard=Guard::enter(&flag).unwrap();guard.keep_active();assert!(Guard::enter(&flag).is_err());assert!(flag.load(Ordering::Acquire));
    }
    #[test]fn a_start_waiting_for_installation_gate_is_rejected_after_maintenance_begins(){
        let shared=std::sync::Arc::new((Mutex::new(()),AtomicBool::new(false)));
        let gate=shared.0.lock().unwrap();let copy=shared.clone();
        let start=std::thread::spawn(move||lock_operation(&copy.0,&copy.1).is_err());
        Guard::enter(&shared.1).unwrap().keep_active();drop(gate);assert!(start.join().unwrap());
    }
}
