use chrono::{DateTime, Duration, FixedOffset};
use std::collections::BTreeMap;

pub(super) struct DailyClock {
    previous: u64,
    sampled_at: DateTime<FixedOffset>,
    pub days: BTreeMap<String, u64>,
}

impl DailyClock {
    pub fn new(previous: u64, days: BTreeMap<String, u64>) -> Self {
        Self {
            previous,
            days,
            sampled_at: chrono::Local::now().fixed_offset(),
        }
    }

    pub fn sample(&mut self, seconds: u64, now: DateTime<FixedOffset>) {
        let delta = seconds.saturating_sub(self.previous);
        self.previous = seconds;
        let wall = now.signed_duration_since(self.sampled_at).num_seconds();
        // 清醒计时是唯一时长来源。遇到休眠或时钟调整，不把墙钟间隔补算为游玩。
        let start = if wall >= 0 && wall.abs_diff(delta as i64) <= 2 {
            self.sampled_at.with_timezone(now.offset())
        } else {
            now - Duration::seconds(delta.min(i64::MAX as u64) as i64)
        };
        let mut remaining = delta;
        let mut cursor = start;
        while remaining > 0 {
            let midnight = cursor
                .date_naive()
                .succ_opt()
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap()
                .and_local_timezone(*now.offset())
                .single()
                .unwrap();
            let available = midnight.signed_duration_since(cursor).num_seconds().max(1) as u64;
            let part = remaining.min(available);
            *self
                .days
                .entry(cursor.date_naive().to_string())
                .or_default() += part;
            remaining -= part;
            cursor += Duration::seconds(part as i64);
        }
        self.sampled_at = now;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn time(value: &str) -> DateTime<FixedOffset> {
        DateTime::parse_from_rfc3339(value).unwrap()
    }
    #[test]
    fn splits_midnight_and_repeated_samples_do_not_duplicate() {
        let mut clock = DailyClock::new(0, BTreeMap::new());
        clock.sampled_at = time("2026-10-02T23:59:58+08:00");
        clock.sample(5, time("2026-10-03T00:00:03+08:00"));
        clock.sample(5, time("2026-10-03T00:00:03+08:00"));
        assert_eq!(clock.days["2026-10-02"], 2);
        assert_eq!(clock.days["2026-10-03"], 3);
    }
    #[test]
    fn sleep_and_recovery_never_backfill_old_wall_time() {
        let mut days = BTreeMap::from([("2026-10-02".into(), 20)]);
        let mut clock = DailyClock::new(7200, days.clone());
        clock.sampled_at = time("2026-10-02T23:00:00+08:00");
        clock.sample(7202, time("2026-10-03T08:00:00+08:00"));
        days.insert("2026-10-03".into(), 2);
        assert_eq!(clock.days, days);
        clock.sample(7202, time("2026-10-03T09:00:00+08:00"));
        assert_eq!(clock.days, days);
    }
}
