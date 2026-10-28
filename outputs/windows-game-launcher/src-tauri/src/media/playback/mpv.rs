use super::pipe::Pipe;
use super::windows::{awake_seconds, Sample};
use serde_json::{json, Value};
use std::{
    io::{BufRead, BufReader, Write},
    sync::mpsc,
    time::Duration,
};

// 每条消息形成不可变快照，文件边界不会被后来的属性通知覆盖。
#[derive(Default)]
struct EventState {
    values: Value,
    loading: bool,
    last: Option<Sample>,
}
impl EventState {
    fn sample(&self, at: f64) -> Option<Sample> {
        if self.loading {
            return None;
        }
        let v = &self.values;
        let path = v["path"].as_str()?.to_string();
        let position = v["time-pos"].as_f64()?;
        let duration = v["duration"].as_f64()?;
        if path.is_empty()
            || !position.is_finite()
            || position < 0.0
            || !duration.is_finite()
            || duration <= 0.0
        {
            return None;
        }
        Some(Sample {
            path,
            position,
            duration,
            running: v["pause"] == false && v["idle-active"] != true,
            stopped: false,
            buffering: v["paused-for-cache"] == true,
            seeking: v["seeking"] == true,
            speed: v["speed"].as_f64(),
            at,
            end_reason: None,
        })
    }
    fn end(&mut self, at: f64, reason: &str) -> Option<Sample> {
        let mut sample = self.last.take()?;
        if reason == "eof" && (sample.duration - sample.position).abs() < 0.5 {
            sample.position = sample.duration;
        }
        sample.running = false;
        sample.stopped = true;
        sample.at = at;
        sample.end_reason = Some(reason.into());
        Some(sample)
    }
    fn receive(&mut self, message: &Value, at: f64) -> Option<Sample> {
        match message["event"].as_str()? {
            "start-file" => {
                let ended = self.end(at, "switch");
                self.loading = true;
                self.values = json!({"pause":self.values["pause"],"speed":self.values["speed"]});
                ended
            }
            "end-file" => {
                self.loading = true;
                self.end(at, message["reason"].as_str().unwrap_or("stop"))
            }
            "shutdown" => self.end(at, "quit"),
            "file-loaded" => {
                self.loading = false;
                let sample = self.sample(at)?;
                self.last = Some(sample.clone());
                Some(sample)
            }
            "property-change" => {
                let name = message["name"].as_str()?;
                self.values[name] = message["data"].clone();
                let sample = self.sample(at)?;
                // path/duration 的过渡值只在 file-loaded 或后续有效属性样本就绪后使用。
                self.last = Some(sample.clone());
                Some(sample)
            }
            _ => None,
        }
    }
}

pub(super) struct Mpv {
    writer: Pipe,
    events: mpsc::Receiver<Sample>,
}
impl Mpv {
    pub fn connect(pipe: &str) -> Result<Self, String> {
        let stream = Pipe::connect(pipe).map_err(|_| "无法连接本次 mpv 播放器的进度接口")?;
        let reader = stream.clone();
        let (tx, events) = mpsc::channel();
        std::thread::spawn(move || {
            let mut state = EventState {
                values: json!({}),
                ..Default::default()
            };
            for line in BufReader::new(reader).lines() {
                let Ok(line) = line else { break };
                if line.len() > 512 * 1024 {
                    break;
                }
                let Ok(message) = serde_json::from_str::<Value>(&line) else {
                    continue;
                };
                if let Some(sample) = state.receive(&message, awake_seconds()) {
                    if tx.send(sample).is_err() {
                        return;
                    }
                }
            }
            if let Some(sample) = state.end(awake_seconds(), "disconnect") {
                let _ = tx.send(sample);
            }
        });
        let mut result = Self {
            writer: stream,
            events,
        };
        for (index, name) in [
            "path",
            "time-pos",
            "duration",
            "pause",
            "paused-for-cache",
            "seeking",
            "speed",
            "idle-active",
        ]
        .iter()
        .enumerate()
        {
            result.command(json!(["observe_property", index + 1, name]))?;
        }
        Ok(result)
    }
    pub fn command(&mut self, command: Value) -> Result<(), String> {
        let mut bytes =
            serde_json::to_vec(&json!({"command":command})).map_err(|_| "mpv 命令编码失败")?;
        bytes.push(b'\n');
        self.writer
            .write_all(&bytes)
            .map_err(|_| "mpv 控制接口已断开".into())
    }
    pub fn playlist(
        &mut self,
        paths: &[String],
        position: f64,
        start_index: usize,
    ) -> Result<(), String> {
        for (index, path) in paths.iter().enumerate() {
            self.command(json!(["loadfile",path,"append",-1,{"start":format!("{:.6}",if index==start_index {position} else {0.0})}]))?;
        }
        self.command(json!(["playlist-play-index", start_index]))?;
        self.command(json!(["set_property", "idle", "no"]))?;
        self.command(json!(["set_property", "pause", false]))
    }
    pub fn sample(&mut self) -> Result<Option<Sample>, String> {
        match self.events.recv_timeout(Duration::from_millis(100)) {
            Ok(sample) => Ok(Some(sample)),
            Err(mpsc::RecvTimeoutError::Timeout) => Ok(None),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                Err("mpv 控制接口已断开，已暂停计时".into())
            }
        }
    }
    pub fn close(&mut self) {
        let _ = self.command(json!(["quit"]));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn file_boundaries_keep_final_sample_and_clear_old_position() {
        let mut state = EventState {
            values: json!({"path":"one.mkv","time-pos":18.25,"duration":20,"pause":false,"speed":1}),
            ..Default::default()
        };
        state.last = state.sample(1.0);
        let ended = state
            .receive(&json!({"event":"end-file","reason":"stop"}), 2.0)
            .unwrap();
        assert_eq!(ended.path, "one.mkv");
        assert_eq!(ended.position, 18.25);
        assert!(ended.stopped);
        state.receive(&json!({"event":"start-file"}), 2.01);
        assert!(state
            .receive(
                &json!({"event":"property-change","name":"path","data":"two.mkv"}),
                2.02
            )
            .is_none());
        assert!(state
            .receive(&json!({"event":"file-loaded"}), 2.03)
            .is_none());
        state.receive(
            &json!({"event":"property-change","name":"duration","data":30}),
            2.04,
        );
        let next = state
            .receive(
                &json!({"event":"property-change","name":"time-pos","data":0.0}),
                2.05,
            )
            .unwrap();
        assert_eq!(next.path, "two.mkv");
        assert_eq!(next.position, 0.0);
        assert_eq!(next.duration, 30.0);
        assert!(next.running);
    }
    #[test]
    fn null_position_before_eof_does_not_discard_last_progress() {
        let mut state = EventState {
            values: json!({"path":"one.mkv","time-pos":19.9,"duration":20,"pause":false}),
            ..Default::default()
        };
        state.last = state.sample(1.0);
        state.receive(
            &json!({"event":"property-change","name":"time-pos","data":null}),
            1.1,
        );
        let ended = state
            .receive(&json!({"event":"end-file","reason":"eof"}), 1.2)
            .unwrap();
        assert_eq!(ended.position, 20.0);
        assert_eq!(ended.end_reason.as_deref(), Some("eof"));
    }
}
