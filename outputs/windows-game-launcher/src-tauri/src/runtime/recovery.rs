use super::{clock::Clock, helper_process, same_path, Session, Tracker};
use crate::model::Game;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::Path, time::Instant};
use sysinfo::Pid;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Record {
    pub game_id: String,
    pub session_id: String,
    pub processes: Vec<ProcessIdentity>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProcessIdentity {
    pub pid: u32,
    pub started: u64,
    pub exe: String,
}

impl Tracker {
    pub(crate) fn recovery_records(&self) -> Vec<Record> {
        self.sessions
            .iter()
            .filter(|(_, session)| session.started.is_some())
            .map(|(id, session)| {
                let processes = session
                    .known
                    .iter()
                    .filter_map(|(pid, birth)| {
                        let process = self.system.process(*pid)?;
                        if process.start_time() != *birth || helper_process(process) {
                            return None;
                        }
                        Some(ProcessIdentity {
                            pid: pid.as_u32(),
                            started: *birth,
                            exe: process.exe()?.to_string_lossy().into_owned(),
                        })
                    })
                    .collect();
                Record {
                    game_id: id.clone(),
                    session_id: session.id.clone(),
                    processes,
                }
            })
            .collect()
    }

    pub(crate) fn recover(
        &mut self,
        record: &Record,
        game: Game,
        seconds: u64,
        days: std::collections::BTreeMap<String, u64>,
    ) -> bool {
        self.refresh_processes(game.source == "steam");
        let known: HashMap<_, _> = record
            .processes
            .iter()
            .filter_map(|saved| {
                let pid = Pid::from_u32(saved.pid);
                let process = self.system.process(pid)?;
                if process.start_time() != saved.started
                    || helper_process(process)
                    || !process
                        .exe()
                        .is_some_and(|exe| same_path(exe, Path::new(&saved.exe)))
                {
                    return None;
                }
                Some((pid, saved.started))
            })
            .collect();
        if known.is_empty() {
            return false;
        }
        self.sessions.insert(
            game.id.clone(),
            Session {
                id: record.session_id.clone(),
                game,
                requested: Instant::now(),
                started: Some(Instant::now()),
                pending_start: None,
                known,
                baseline: self
                    .system
                    .processes()
                    .iter()
                    .map(|(pid, process)| (*pid, process.start_time()))
                    .collect(),
                empty_ticks: 0,
                credited: seconds,
                clock: Clock::new(seconds, true),
                daily: super::daily::DailyClock::new(seconds, days),
            },
        );
        true
    }

    pub(crate) fn checkpoints(&mut self) -> Vec<super::Update> {
        self.sessions
            .iter_mut()
            .filter(|(_, session)| session.started.is_some())
            .map(|(game_id, session)| {
                let seconds = session.clock.seconds();
                session
                    .daily
                    .sample(seconds, chrono::Local::now().fixed_offset());
                super::Update {
                    game_id: game_id.clone(),
                    session_id: session.id.clone(),
                    started_at: session.pending_start.clone(),
                    seconds,
                    finished: false,
                    failed: false,
                    daily: session.daily.days.clone(),
                }
            })
            .collect()
    }
}
