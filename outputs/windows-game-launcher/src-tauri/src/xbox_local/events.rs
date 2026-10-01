use chrono::{DateTime, FixedOffset};
use std::collections::HashMap;

#[derive(Debug)]
pub(super) struct Event {
    pub id: String,
    #[cfg_attr(not(windows), allow(dead_code))]
    pub at: String,
    pub quiet: bool,
}

// 只接受同一附加周期内已观察到请求、且成功完成的 100% 回调。
pub(super) fn parse(text: &str) -> Vec<Event> {
    let mut armed: Option<DateTime<FixedOffset>> = None;
    let mut pending = HashMap::new();
    let mut events = Vec::new();
    for line in text.trim_start_matches('\u{feff}').lines() {
        parse_line(line, &mut armed, &mut pending, &mut events);
    }
    events
}

fn parse_line(
    line: &str,
    armed: &mut Option<DateTime<FixedOffset>>,
    pending: &mut HashMap<String, (String, bool)>,
    events: &mut Vec<Event>,
) {
    let mut parts = line.splitn(3, '\t');
    let (Some(time), Some(kind), Some(data)) = (parts.next(), parts.next(), parts.next()) else {
        return;
    };
    let Ok(at) = DateTime::parse_from_rfc3339(time) else {
        return;
    };
    if kind == "armed" {
        *armed = Some(at);
        pending.clear();
        return;
    }
    if matches!(kind, "detached" | "process-exited") {
        *armed = None;
        pending.clear();
        return;
    }
    let Some(start) = armed.as_ref() else { return };
    let fields: HashMap<_, _> = data
        .split_whitespace()
        .filter_map(|item| item.split_once('='))
        .collect();
    let Some(id) = fields
        .get("id")
        .filter(|id| !id.is_empty() && id.len() <= 10 && id.chars().all(|c| c.is_ascii_digit()))
    else {
        return;
    };
    if fields.get("progress") != Some(&"100") {
        return;
    }
    if kind == "achievement-request" {
        pending.insert(
            (*id).to_owned(),
            (at.to_rfc3339(), (at - *start).num_seconds() < 60),
        );
    } else if kind == "achievement-completed" && fields.get("HRESULT") == Some(&"0x00000000") {
        if let Some((time, quiet)) = pending.remove(*id) {
            events.push(Event {
                id: (*id).to_owned(),
                at: time,
                quiet,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepts_only_successful_complete_requests_and_marks_initial_replay_quiet() {
        let text = "2026-10-01T02:00:00+08:00\tarmed\tpid=1\n\
2026-10-01T02:00:05+08:00\tachievement-request\tid=21 progress=100\n\
2026-10-01T02:00:10+08:00\tachievement-completed\tid=21 progress=100 HRESULT=0x00000000\n\
2026-10-01T02:02:00+08:00\tachievement-request\tid=4 progress=100\n\
2026-10-01T02:02:08+08:00\tachievement-completed\tid=4 progress=100 HRESULT=0x00000000\n\
2026-10-01T02:02:09+08:00\tachievement-completed\tid=4 progress=100 HRESULT=0x00000000\n\
2026-10-01T02:02:10+08:00\tachievement-request\tid=5 progress=100\n\
2026-10-01T02:02:11+08:00\tachievement-completed\tid=5 progress=100 HRESULT=0x80004005\n\
2026-10-01T02:02:12+08:00\tachievement-completed\tid=6 progress=100 HRESULT=0x00000000\n";
        let result = parse(text);
        assert_eq!(result.len(), 2);
        assert!(result[0].quiet);
        assert_eq!(result[1].id, "4");
        assert!(!result[1].quiet);
    }
    #[test]
    fn does_not_carry_requests_across_processes_or_accept_partial_progress() {
        let text = "2026-10-01T02:00:00Z\tarmed\tpid=1\n2026-10-01T02:02:00Z\tachievement-request\tid=4 progress=100\n2026-10-01T02:02:01Z\tdetached\tdone\n2026-10-01T02:02:02Z\tarmed\tpid=2\n2026-10-01T02:02:03Z\tachievement-completed\tid=4 progress=100 HRESULT=0x00000000\n2026-10-01T02:02:04Z\tachievement-request\tid=5 progress=50\n2026-10-01T02:02:05Z\tachievement-completed\tid=5 progress=50 HRESULT=0x00000000\n";
        assert!(parse(text).is_empty());
    }
}
