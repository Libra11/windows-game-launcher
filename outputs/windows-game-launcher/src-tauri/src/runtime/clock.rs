use std::time::Duration;

#[cfg(windows)]
fn awake_time() -> Duration {
    #[link(name = "kernel32")]
    extern "system" {
        fn QueryUnbiasedInterruptTime(time: *mut u64) -> i32;
    }
    let mut ticks = 0;
    // Windows 清醒时间不包含休眠，也不受系统日期调整影响。
    unsafe {
        QueryUnbiasedInterruptTime(&mut ticks);
    }
    Duration::from_nanos(ticks.saturating_mul(100))
}

#[cfg(not(windows))]
fn awake_time() -> Duration {
    use std::{sync::OnceLock, time::Instant};
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed()
}

pub(super) struct Clock {
    elapsed: Duration,
    sampled: Duration,
    running: bool,
}
impl Clock {
    pub(super) fn new(seconds: u64, running: bool) -> Self {
        Self {
            elapsed: Duration::from_secs(seconds),
            sampled: awake_time(),
            running,
        }
    }
    pub(super) fn sample(&mut self, alive: bool) {
        let now = awake_time();
        if self.running {
            let delta = now.saturating_sub(self.sampled);
            // 消失的进程只补最后一个正常轮询间隔，不把长时间未观测的间隔算作游玩。
            self.elapsed = self.elapsed.saturating_add(if alive {
                delta
            } else {
                delta.min(Duration::from_secs(1))
            });
        }
        self.sampled = now;
        self.running = alive;
    }
    pub(super) fn seconds(&self) -> u64 {
        let delta = if self.running {
            awake_time().saturating_sub(self.sampled)
        } else {
            Duration::ZERO
        };
        self.elapsed.saturating_add(delta).as_secs()
    }
}
