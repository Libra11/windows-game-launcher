//! 外部播放器会话只管理由本模块启动的实例；未知或切换到库外文件时停止计时。
mod mpv;
mod pipe;
mod playlist;
mod windows;

use super::{store, State};
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, OnceLock,
    },
    time::{Duration, Instant},
};
use tauri::{Emitter, Manager};
use windows::{Adapter, Sample};

#[derive(Default)]
struct Runtime {
    active: AtomicBool,
    stop: AtomicBool,
    status: Mutex<Value>,
}
fn runtime(dir: &Path) -> Arc<Runtime> {
    static MAP: OnceLock<Mutex<HashMap<PathBuf, Arc<Runtime>>>> = OnceLock::new();
    let mut map = MAP
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    map.entry(dir.to_path_buf()).or_default().clone()
}
fn dir(app: &tauri::AppHandle) -> PathBuf {
    app.state::<State>().dir.clone()
}
fn emit(app: &tauri::AppHandle, rt: &Runtime, mut value: Value) {
    value["sampleAt"] = json!(windows::awake_seconds());
    let mut notify = true;
    if let Ok(mut state) = rt.status.lock() {
        notify = ["state", "active", "itemId", "resourceId"]
            .iter()
            .any(|key| state[*key] != value[*key])
            || (state["positionSeconds"].as_f64().unwrap_or(-1.0)
                - value["positionSeconds"].as_f64().unwrap_or(-1.0))
            .abs()
                >= 0.1;
        if notify {
            *state = value.clone();
        }
        if value["active"] == false {
            // 终态与会话许可一起发布，看到已停止即可立即再次播放。
            rt.active.store(false, Ordering::Release);
        }
    }
    if notify {
        let _ = app.emit("media-playback-changed", value);
    }
}
fn changed(app: &tauri::AppHandle) {
    let _ = app.emit("media-changed", ());
}
pub async fn status(app: tauri::AppHandle, _input: Value) -> Result<Value, String> {
    let rt = runtime(&dir(&app));
    let result = rt.status.lock().map_err(|_| "播放状态不可用")?.clone();
    Ok(if result.is_null() {
        json!({"state":"idle","active":false})
    } else {
        result
    })
}
pub fn is_active(app: &tauri::AppHandle) -> bool {
    runtime(&dir(app)).active.load(Ordering::Acquire)
}
pub async fn stop(app: tauri::AppHandle, _input: Value) -> Result<Value, String> {
    let rt = runtime(&dir(&app));
    if rt.active.load(Ordering::Acquire) {
        rt.stop.store(true, Ordering::Release);
    }
    Ok(json!({"state":"stopping"}))
}

#[derive(Clone)]
struct Resource {
    id: String,
    item_id: String,
    path: String,
    position: f64,
    duration: f64,
    size: i64,
    modified_at: String,
}
fn resource_by_id(
    conn: &rusqlite::Connection,
    id: &str,
    requested_item: Option<&str>,
) -> Result<Resource, String> {
    let row=conn.query_row("SELECT r.id,r.path,r.position_seconds,r.duration_seconds,r.size,r.modified_at,r.source_id FROM resources r WHERE r.id=?1 AND r.available=1 AND r.missing=0",
[id],
|row|Ok((row.get::<_,
String>(0)?,
row.get::<_,
String>(1)?,
row.get::<_,
f64>(2)?,
row.get::<_,
f64>(3)?,
row.get::<_,
i64>(4)?,
row.get::<_,
String>(5)?,row.get::<_,String>(6)?))).optional().map_err(|_|"无法读取媒体资源")?.ok_or("资源不存在、已离线或已删除")?;

    if super::ignored::contains(conn, &row.6, &row.1)? {
        return Err("媒体路径已忽略，不能播放".into());
    }

    let item_id = if let Some(item) = requested_item {
        conn.query_row("WITH RECURSIVE scope(id) AS (SELECT ?2 UNION SELECT m.id FROM media_items m JOIN scope s ON m.parent_id=s.id) SELECT l.item_id FROM resource_links l JOIN scope s ON s.id=l.item_id WHERE l.resource_id=?1 ORDER BY l.locked DESC,l.item_id LIMIT 1",
params![id,
item],
|r|r.get::<_,
String>(0)).optional().map_err(|_|"无法读取资源关联")?.ok_or("所选资源不属于当前影视条目或其季集")?
    } else {
        conn.query_row("SELECT item_id FROM resource_links WHERE resource_id=?1 ORDER BY locked DESC,item_id LIMIT 1",[id],|r|r.get::<_,String>(0)).map_err(|_|"资源尚未关联基础条目")?
    };
    Ok(Resource {
        id: row.0,
        path: row.1,
        position: row.2.max(0.0),
        duration: row.3.max(0.0),
        size: row.4,
        modified_at: row.5,
        item_id,
    })
}
fn selected_resource(conn: &rusqlite::Connection, input: &Value) -> Result<Resource, String> {
    let item = input["itemId"].as_str().filter(|s| !s.is_empty());
    if let Some(id) = input["resourceId"].as_str().filter(|s| !s.is_empty()) {
        return resource_by_id(conn, id, item);
    }
    let item = item.ok_or("请选择要播放的影视条目或资源")?;
    let id:String=conn.query_row("WITH RECURSIVE scope(id) AS (SELECT ?1 UNION SELECT m.id FROM media_items m JOIN scope s ON m.parent_id=s.id) SELECT r.id FROM resources r JOIN resource_links l ON l.resource_id=r.id JOIN scope s ON s.id=l.item_id JOIN media_items m ON m.id=l.item_id WHERE r.available=1 AND r.missing=0 ORDER BY CASE WHEN m.kind='episode' THEN 0 ELSE 1 END,COALESCE(m.season_number,(SELECT season_number FROM media_items WHERE id=m.parent_id),2147483647),COALESCE(m.episode_number,2147483647),r.path LIMIT 1",
[item],
|r|r.get(0)).map_err(|_|"当前条目没有可播放的本地资源")?;

    resource_by_id(conn, &id, Some(item))
}
fn player(conn: &rusqlite::Connection) -> Result<(PathBuf, String), String> {
    let configured = store::setting(conn, "playerPath")?
        .as_str()
        .unwrap_or("")
        .to_string();
    let kind = store::setting(conn, "playerKind")?
        .as_str()
        .unwrap_or("potplayer")
        .to_string();
    let path = if !configured.trim().is_empty() {
        PathBuf::from(configured)
    } else {
        let mut candidates = if kind == "mpv" {
            vec![PathBuf::from(r"C:\Program Files\mpv\mpv.exe")]
        } else {
            vec![
                PathBuf::from(r"C:\Program Files\DAUM\PotPlayer\PotPlayerMini64.exe"),
                PathBuf::from(r"C:\Program Files (x86)\DAUM\PotPlayer\PotPlayerMini.exe"),
            ]
        };
        if kind == "mpv" {
            if let Some(local) = std::env::var_os("LOCALAPPDATA") {
                candidates.push(PathBuf::from(local).join("Programs/mpv/mpv.exe"));
            }
        }
        candidates
            .into_iter()
            .find(|path| path.is_file())
            .ok_or("未找到播放器，请在影视设置中选择 PotPlayer 或 mpv")?
    };
    if !path.is_file() {
        return Err("播放器路径无效，请重新选择播放器程序".into());
    }
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let inferred = if name.contains("mpv") {
        "mpv"
    } else if name.contains("potplayer") {
        "potplayer"
    } else {
        kind.as_str()
    };
    if !matches!(inferred, "potplayer" | "mpv") {
        return Err("当前支持对接 PotPlayer 和 mpv 的进度接口".into());
    }
    Ok((path, inferred.to_string()))
}
// SQLite 保留 canonical 路径；播放器使用常规 Windows 路径，避免把 verbatim 前缀当作标题。
fn player_path(path: &str) -> String {
    if path
        .get(..8)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(r"\\?\UNC\"))
    {
        return format!(r"\\{}", &path[8..]);
    }
    if let Some(rest) = path.strip_prefix(r"\\?\") {
        let bytes = rest.as_bytes();
        if bytes.len() >= 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && matches!(bytes[2], b'\\' | b'/')
        {
            return rest.to_string();
        }
    }
    path.to_string()
}
pub(super) const DEFAULT_POTPLAYER_POLL_INTERVAL_MS: u64 = 500;

pub(super) fn potplayer_poll_interval_ms(value: Option<&Value>) -> Result<u64, String> {
    match value {
        None => Ok(DEFAULT_POTPLAYER_POLL_INTERVAL_MS),
        Some(value) => value
            .as_u64()
            .filter(|ms| (50..=2000).contains(ms))
            .ok_or_else(|| "PotPlayer 进度读取间隔须为 50 至 2000 毫秒的整数".into()),
    }
}

fn seek_argument(position: f64) -> String {
    let millis = (position.max(0.0) * 1000.0).round() as u64;
    format!(
        "/seek={:02}:{:02}:{:02}.{:03}",
        millis / 3_600_000,
        (millis / 60_000) % 60,
        (millis / 1000) % 60,
        millis % 1000
    )
}
pub async fn play(app: tauri::AppHandle, input: Value) -> Result<Value, String> {
    let media_dir = dir(&app);
    let rt = runtime(&media_dir);
    {
        let _status = rt.status.lock().map_err(|_| "播放状态不可用")?;
        if rt.active.swap(true, Ordering::AcqRel) {
            return Err("已有影视播放会话，请先结束当前播放")?;
        }
    }
    let prepare = (|| -> Result<_, String> {
        let conn = store::open(&media_dir)?;
        let resource = selected_resource(&conn, &input)?;
        if !Path::new(&resource.path).is_file() {
            return Err("媒体文件当前不可访问")?;
        }
        let (path, kind) = player(&conn)?;
        let playlist = playlist::paths(&conn, &resource)?;
        let completed = store::playback_complete(resource.position, resource.duration);
        Ok((resource, path, kind, playlist, completed))
    })();
    let (resource, executable, kind, playlist, completed) = match prepare {
        Ok(value) => value,
        Err(error) => {
            rt.active.store(false, Ordering::Release);
            return Err(error);
        }
    };
    let resume = input["resume"].as_bool().unwrap_or(true);
    let position = if resume && !completed {
        resource.position
    } else {
        0.0
    };
    let pipe = format!(r"\\.\pipe\youji-media-{}", uuid::Uuid::new_v4());
    let playlist_file = if kind == "potplayer" && playlist.paths.len() > 1 {
        match playlist::PlaylistFile::new(&playlist.paths, playlist.start_index) {
            Ok(file) => Some(file),
            Err(error) => {
                rt.active.store(false, Ordering::Release);
                return Err(error);
            }
        }
    } else {
        None
    };
    let mut command = Command::new(&executable);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(0x0800_0000);
    if kind == "mpv" {
        command
            .arg(format!("--input-ipc-server={pipe}"))
            .arg("--keep-open=no")
            .arg("--idle=yes")
            .arg("--pause=yes")
            .arg("--hr-seek=yes")
            .arg("--resume-playback=no")
            .arg("--save-position-on-quit=no");
    } else {
        command
            .arg(
                playlist_file
                    .as_ref()
                    .map(|file| file.0.to_string_lossy().into_owned())
                    .unwrap_or_else(|| player_path(&resource.path)),
            )
            .arg("/new")
            .arg(seek_argument(position));
    }
    let child = match command.spawn() {
        Ok(child) => child,
        Err(_) => {
            rt.active.store(false, Ordering::Release);
            return Err("无法启动播放器，请检查程序路径")?;
        }
    };
    let pid = child.id();
    rt.stop.store(false, Ordering::Release);
    let initial = json!({
        "state":"starting",
        "active":true,
        "player":kind,
        "pid":pid,
        "itemId":resource.item_id,
        "resourceId":resource.id,
        "positionSeconds":position,
        "message":"正在等待播放器确认当前文件",
    });

    emit(&app, &rt, initial.clone());
    std::thread::spawn(move || {
        let _playlist_file = playlist_file;
        let result = watch(
            &app, &rt, child, &kind, &pipe, resource, position, &playlist,
        );
        if let Err(error) = result {
            emit(
                &app,
                &rt,
                json!({"state":"error","active":false,"message":error}),
            );
        }
        changed(&app);
    });
    Ok(initial)
}

/// 覆盖区间保留实际播放范围，不参与「已看」或播放完成判定。
#[derive(Default, Clone)]
struct Coverage(Vec<[f64; 2]>);
impl Coverage {
    fn add(&mut self, start: f64, end: f64) {
        if !start.is_finite() || !end.is_finite() || start < 0.0 || end <= start {
            return;
        }
        self.0.push([start, end]);
        self.0.sort_by(|a, b| a[0].total_cmp(&b[0]));
        let mut merged: Vec<[f64; 2]> = Vec::new();
        for range in &self.0 {
            if let Some(last) = merged.last_mut() {
                if range[0] <= last[1] + 0.1 {
                    last[1] = last[1].max(range[1]);
                    continue;
                }
            }
            merged.push(*range);
        }
        self.0 = merged;
    }
    #[cfg(test)]
    fn seconds(&self) -> f64 {
        self.0.iter().map(|r| (r[1] - r[0]).max(0.0)).sum()
    }
    fn load(conn: &rusqlite::Connection, id: &str) -> Self {
        let raw: Option<String> = conn
            .query_row(
                "SELECT segments_json FROM media_playback_coverage WHERE resource_id=?1",
                [id],
                |r| r.get(0),
            )
            .optional()
            .ok()
            .flatten();
        let mut result = Self::default();
        if let Some(raw) = raw {
            if let Ok(ranges) = serde_json::from_str::<Vec<[f64; 2]>>(&raw) {
                for range in ranges {
                    result.add(range[0], range[1]);
                }
            }
        }
        result
    }
}
struct Session {
    resource: Resource,
    id: String,
    started_at: String,
    last_played_at: String,
    seconds: f64,
    position: f64,
    duration: f64,
    coverage: Coverage,
    played: bool,
    watched: bool,
    resume_floor: Option<f64>,
}
impl Session {
    fn new(conn: &rusqlite::Connection, resource: Resource) -> Self {
        Self {
            coverage: Coverage::load(conn, &resource.id),
            position: resource.position,
            duration: resource.duration,
            resource,
            id: uuid::Uuid::new_v4().to_string(),
            started_at: store::now(),
            last_played_at: store::now(),
            seconds: 0.0,
            played: false,
            watched: false,
            resume_floor: None,
        }
    }
    fn checkpoint(&mut self, conn: &rusqlite::Connection, ended: bool) -> Result<(), String> {
        let tx = conn
            .unchecked_transaction()
            .map_err(|_| "无法保存播放检查点")?;
        let now = store::now();
        let persisted_position = self
            .resume_floor
            .map(|floor| self.position.max(floor))
            .unwrap_or(self.position);
        // 大小和修改时间共同作为文件版本；与扫描串行提交，拒绝旧会话写回新文件。
        let updated = tx.execute(
            "UPDATE resources SET position_seconds=?2,duration_seconds=CASE WHEN ?3>0 THEN ?3 ELSE duration_seconds END,updated_at=?4 WHERE id=?1 AND size=?5 AND modified_at=?6",
            params![self.resource.id,persisted_position,self.duration,now,self.resource.size,self.resource.modified_at],
        ).map_err(|_| "无法保存播放进度")?;
        if updated == 0 {
            return Err("媒体文件已变化，已停止记录，请重新播放".into());
        }

        let coverage =
            serde_json::to_string(&self.coverage.0).map_err(|_| "无法保存播放覆盖区间")?;
        tx.execute(
            "INSERT INTO media_playback_coverage(resource_id,segments_json) VALUES(?1,?2)
             ON CONFLICT(resource_id) DO UPDATE SET segments_json=excluded.segments_json",
            params![self.resource.id, coverage],
        )
        .map_err(|_| "无法保存播放覆盖区间")?;
        if self.played {
            let ended_at = if ended {
                self.last_played_at.as_str()
            } else {
                ""
            };
            tx.execute(
                "INSERT INTO view_sessions(id,item_id,resource_id,started_at,ended_at,seconds,position_seconds)
                 VALUES(?1,?2,?3,?4,?5,?6,?7) ON CONFLICT(id) DO UPDATE SET
                 ended_at=excluded.ended_at,seconds=excluded.seconds,position_seconds=excluded.position_seconds",
                params![self.id,self.resource.item_id,self.resource.id,self.started_at,ended_at,self.seconds,self.position],
            ).map_err(|_| "无法保存观看记录")?;
            tx.execute(
                "INSERT INTO personal(item_id,last_played) VALUES(?1,?2)
                 ON CONFLICT(item_id) DO UPDATE SET last_played=excluded.last_played",
                params![self.resource.item_id, self.last_played_at],
            )
            .map_err(|_| "无法保存最近观看时间")?;
        }
        let completed = store::playback_complete(self.position, self.duration);
        if completed && self.played && !self.watched {
            store::update_record(&tx, &self.resource.item_id, &json!({"watched":true}), false)?;
        }
        tx.commit().map_err(|_| "无法保存播放检查点")?;
        if completed && self.played {
            self.watched = true;
        }
        Ok(())
    }
}
fn normalized(raw: &str) -> String {
    let raw = if raw.starts_with("file:") {
        tauri::Url::parse(raw)
            .ok()
            .and_then(|u| u.to_file_path().ok())
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| raw.into())
    } else {
        raw.into()
    };
    let path = std::fs::canonicalize(&raw).unwrap_or_else(|_| PathBuf::from(raw));
    player_path(&path.to_string_lossy())
        .replace('/', "\\")
        .to_lowercase()
}
fn resolve_sample(conn: &rusqlite::Connection, path: &str, current: &Resource) -> Option<Resource> {
    if normalized(path) == normalized(&current.path) {
        return Some(current.clone());
    }
    let mut stmt = conn
        .prepare("SELECT id,path FROM resources WHERE available=1 AND missing=0")
        .ok()?;
    let values = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .ok()?;
    for row in values.flatten() {
        if normalized(path) == normalized(&row.1) {
            return resource_by_id(conn, &row.0, None).ok();
        }
    }
    None
}
fn valid_interval(previous: &Sample, current: &Sample, wall: f64) -> bool {
    if !previous.running
        || !current.running
        || previous.buffering
        || current.buffering
        || previous.seeking
        || current.seeking
        || wall <= 0.0
        || wall > 2.5
    {
        return false;
    }
    let delta = current.position - previous.position;
    let speed = current
        .speed
        .filter(|s| *s >= 0.1 && *s <= 16.0)
        .unwrap_or(2.0);
    delta > 0.0 && delta <= (wall * speed * 1.5 + 0.75).max(1.0)
}
fn save_checkpoint(
    app: &tauri::AppHandle,
    session: &mut Session,
    conn: &rusqlite::Connection,
    ended: bool,
) -> Result<(), String> {
    let _operation = tauri::async_runtime::block_on(super::operation(app))?;
    session.checkpoint(conn, ended)
}
#[derive(Clone, Copy, Debug, PartialEq)]
enum InitialSeekAction {
    Wait,
    Seek(f64),
    Confirmed,
    Unavailable,
}
const INITIAL_POSITION_TOLERANCE_SECONDS: f64 = 5.0;
struct InitialSeek {
    target: f64,
    started: f64,
    ready_samples: u8,
    ready_at: Option<f64>,
    sent_at: Option<f64>,
    attempts: u8,
    candidate: Option<(f64, f64)>,
    failed: bool,
}
impl InitialSeek {
    fn new(target: f64, now: f64) -> Self {
        Self {
            target,
            started: now,
            ready_samples: 0,
            ready_at: None,
            sent_at: None,
            attempts: 0,
            candidate: None,
            failed: false,
        }
    }
    fn observe(&mut self, sample: &Sample, now: f64) -> InitialSeekAction {
        if self.failed || now - self.started > 20.0 {
            self.failed = true;
            return InitialSeekAction::Unavailable;
        }
        if !sample.duration.is_finite()
            || sample.duration <= 0.0
            || sample.stopped
            || sample.buffering
            || sample.seeking
            || !sample.position.is_finite()
            || sample.position < 0.0
        {
            self.ready_samples = 0;
            self.ready_at = None;
            self.candidate = None;
            return InitialSeekAction::Wait;
        }
        self.ready_samples = self.ready_samples.saturating_add(1);
        let ready_at = *self.ready_at.get_or_insert(now);
        if self.ready_samples < 2 {
            return InitialSeekAction::Wait;
        }
        let target = self.target.min((sample.duration - 0.1).max(0.0));
        let since = now - self.sent_at.unwrap_or(ready_at);
        if self.sent_at.is_some() && since < 0.5 {
            return InitialSeekAction::Wait;
        }
        // 优先确认命令行的落点；仅在稳定就绪后偏差超过 5 秒时补跳。
        // 向前的容差包含就绪检查期间的正常播放，避免把自然推进当作偏差。
        let speed = sample
            .speed
            .filter(|v| v.is_finite() && *v > 0.0 && *v <= 16.0)
            .unwrap_or(2.0);
        let close = sample.position >= (target - INITIAL_POSITION_TOLERANCE_SECONDS).max(0.0)
            && sample.position
                <= target
                    + INITIAL_POSITION_TOLERANCE_SECONDS
                    + if sample.running {
                        (since * speed).min(10.0)
                    } else {
                        0.0
                    }
            && (target <= INITIAL_POSITION_TOLERANCE_SECONDS
                || !sample.running
                || sample.position > 0.05);
        if close {
            if let Some((position, at)) = self.candidate {
                let delta = sample.position - position;
                if delta >= -0.25 && delta <= (now - at) * speed * 1.5 + 0.75 {
                    return if now - at >= 0.3 {
                        InitialSeekAction::Confirmed
                    } else {
                        InitialSeekAction::Wait
                    };
                }
            }
            self.candidate = Some((sample.position, now));
            InitialSeekAction::Wait
        } else {
            self.candidate = None;
            let retry_after = if self.sent_at.is_some() { 2.0 } else { 1.0 };
            if since >= retry_after && self.attempts < 2 {
                self.send(target, now)
            } else {
                InitialSeekAction::Wait
            }
        }
    }
    fn send(&mut self, target: f64, now: f64) -> InitialSeekAction {
        self.attempts += 1;
        self.sent_at = Some(now);
        self.candidate = None;
        InitialSeekAction::Seek(target)
    }
}
// 监测发生不可恢复错误时，也不留下应用已失去控制的自建播放器进程。
struct OwnedChild(Child);
impl std::ops::Deref for OwnedChild {
    type Target = Child;
    fn deref(&self) -> &Child {
        &self.0
    }
}
impl std::ops::DerefMut for OwnedChild {
    fn deref_mut(&mut self) -> &mut Child {
        &mut self.0
    }
}
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}
fn watch(
    app: &tauri::AppHandle,
    rt: &Runtime,
    child: Child,
    kind: &str,
    pipe: &str,
    initial: Resource,
    start_position: f64,
    playlist: &playlist::Queue,
) -> Result<(), String> {
    let mut child = OwnedChild(child);
    let mut adapter = match Adapter::connect(kind, child.id(), pipe) {
        Ok(adapter) => adapter,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
    };
    adapter.playlist(&playlist.paths, start_position, playlist.start_index)?;
    let media_dir = dir(app);
    let conn = store::open(&media_dir)?;
    let configured_interval = store::setting(&conn, "potplayerPollIntervalMs")?;
    let poll_interval = Duration::from_millis(potplayer_poll_interval_ms(
        (!configured_interval.is_null()).then_some(&configured_interval),
    )?);
    let mut current = Session::new(&conn, initial);
    let mut previous: Option<(Sample, f64)> = None;
    let mut checkpoint = Instant::now();
    let mut last_success = Instant::now();
    let mut last_error = String::new();
    let initial_id = current.resource.id.clone();
    let mut initial_seek =
        (kind == "potplayer").then(|| InitialSeek::new(start_position, windows::awake_seconds()));
    let mut initial_floor = (kind == "potplayer" && start_position > 0.0).then_some(start_position);
    let mut tracking = false;
    let mut closing: Option<Instant> = None;
    let mut cached_path = String::new();
    let mut cached_resource: Option<Resource> = None;
    let mut cache_at = Instant::now();
    loop {
        if rt.stop.load(Ordering::Acquire) && closing.is_none() {
            adapter.close();
            closing = Some(Instant::now());
        }
        if closing.is_some_and(|at| at.elapsed() > Duration::from_secs(3)) {
            let _ = child.kill();
            let _ = child.wait();
            break;
        }
        match adapter.sample() {
            Ok(Some(sample)) => {
                last_success = Instant::now();
                if cached_path != sample.path
                    || (cached_resource.is_none() && cache_at.elapsed() > Duration::from_secs(2))
                {
                    cached_path = sample.path.clone();
                    cached_resource = resolve_sample(&conn, &sample.path, &current.resource);
                    cache_at = Instant::now();
                }
                if let Some(resource) = cached_resource.clone() {
                    if resource.id != initial_id {
                        initial_seek = None;
                        initial_floor = None;
                    }
                    if let Some(gate) = initial_seek.as_mut() {
                        let action = gate.observe(&sample, windows::awake_seconds());
                        match action {
                            InitialSeekAction::Confirmed => {
                                initial_seek = None;
                            }
                            action => {
                                previous = None;
                                if let InitialSeekAction::Seek(position) = action {
                                    if let Err(error) = adapter.seek(position) {
                                        gate.failed = true;
                                        last_error = error;
                                    }
                                }
                                emit(
                                    app,
                                    rt,
                                    json!({
                                        "state":if action==InitialSeekAction::Unavailable || gate.failed { "unavailable"} else { "starting"},
                                        "active":true,
                                        "player":kind,
                                        "pid":child.id(),
                                        "itemId":current.resource.item_id,
                                        "resourceId":current.resource.id,
                                        "positionSeconds":start_position,
                                        "durationSeconds":sample.duration,
                                        "sessionSeconds":0,
                                        "message":if action==InitialSeekAction::Unavailable || gate.failed { "未能确认续播位置，已保留原进度并暂停记录；可停止后重试"} else { "正在等待播放器就绪并确认续播位置"},
                                    }),
                                );
                                adapter.wait(poll_interval);
                                continue;
                            }
                        }
                    }
                    if resource.id != current.resource.id || (!tracking && !sample.stopped) {
                        if tracking {
                            save_checkpoint(app, &mut current, &conn, true)?;
                        }
                        let floor = if resource.id == current.resource.id {
                            current.resume_floor.or(initial_floor.take())
                        } else {
                            None
                        };
                        current = Session::new(&conn, resource);
                        current.resume_floor = floor;
                        tracking = true;
                        previous = None;
                        changed(app);
                    }
                    if sample.duration > 0.0 && sample.duration.is_finite() {
                        current.duration = sample.duration;
                    }
                    if sample.position.is_finite()
                        && sample.position >= 0.0
                        && (kind == "mpv" || sample.running || sample.position > 0.0)
                    {
                        current.position = sample.position;
                        if current
                            .resume_floor
                            .is_some_and(|floor| sample.position >= floor)
                        {
                            current.resume_floor = None;
                        }
                    }
                    let clock = sample.at;
                    let transition = previous.as_ref().is_some_and(|(prior, _)| {
                        prior.running != sample.running
                            || prior.buffering != sample.buffering
                            || prior.seeking != sample.seeking
                    });
                    if let Some((prior, at)) = &previous {
                        let wall = clock - at;
                        if valid_interval(prior, &sample, wall) {
                            current.seconds += wall;
                            current.coverage.add(prior.position, sample.position);
                            current.played = true;
                            current.last_played_at = store::now();
                        }
                    }
                    if sample.stopped {
                        previous = None;
                        if tracking {
                            save_checkpoint(app, &mut current, &conn, true)?;
                            tracking = false;
                            changed(app);
                        }
                        emit(
                            app,
                            rt,
                            json!({
                                "state":"stopped",
                                "active":true,
                                "player":kind,
                                "pid":child.id(),
                                "itemId":current.resource.item_id,
                                "resourceId":current.resource.id,
                                "positionSeconds":current.position,
                                "durationSeconds":current.duration,
                                "sessionSeconds":current.seconds,
                                "message":"播放已停止",
                            }),
                        );
                        adapter.wait(poll_interval);
                        continue;
                    }
                    let state = if sample.buffering {
                        "buffering"
                    } else if sample.seeking {
                        "seeking"
                    } else if sample.running {
                        "playing"
                    } else {
                        "paused"
                    };
                    emit(
                        app,
                        rt,
                        json!({
                            "state":state,
                            "active":true,
                            "player":kind,
                            "pid":child.id(),
                            "itemId":current.resource.item_id,
                            "resourceId":current.resource.id,
                            "positionSeconds":current.position,
                            "durationSeconds":current.duration,
                            "sessionSeconds":current.seconds,
                            "lastPlayed":if current.played {Some(&current.last_played_at)} else {None},
                            "message":"",
                        }),
                    );
                    previous = Some((sample, clock));
                    if tracking && transition {
                        save_checkpoint(app, &mut current, &conn, false)?;
                        checkpoint = Instant::now();
                        changed(app);
                    }
                } else {
                    previous = None;
                    if tracking {
                        save_checkpoint(app, &mut current, &conn, true)?;
                        tracking = false;
                        changed(app);
                    }
                    emit(
                        app,
                        rt,
                        json!({"state":"untracked","active":true,"message":"播放器已切换到未收录文件，暂停记录当前资源的时间"}),
                    );
                }
                if tracking && checkpoint.elapsed() >= Duration::from_secs(2) {
                    save_checkpoint(app, &mut current, &conn, false)?;
                    checkpoint = Instant::now();
                    changed(app);
                }
            }
            Ok(None) => {
                if child
                    .try_wait()
                    .map_err(|_| "无法确认播放器进程状态")?
                    .is_some()
                {
                    break;
                }
                continue;
            }
            Err(error) => {
                if child
                    .try_wait()
                    .map_err(|_| "无法确认播放器进程状态")?
                    .is_some()
                {
                    break;
                }
                previous = None;
                last_error = error;
                if kind == "mpv" {
                    std::thread::sleep(Duration::from_millis(100));
                }
                if last_success.elapsed() > Duration::from_secs(15) {
                    if tracking {
                        save_checkpoint(app, &mut current, &conn, true)?;
                        tracking = false;
                        changed(app);
                    }
                    emit(
                        app,
                        rt,
                        json!({"state":"unavailable","active":true,"message":"暂时无法确认播放器当前文件，已暂停计时"}),
                    );
                }
            }
        }
        adapter.wait(poll_interval);
    }
    if tracking {
        save_checkpoint(app, &mut current, &conn, true)?;
    }
    emit(
        app,
        rt,
        json!({
            "state":"idle",
            "active":false,
            "itemId":current.resource.item_id,
            "resourceId":current.resource.id,
            "positionSeconds":current.position,
            "durationSeconds":current.duration,
            "sessionSeconds":current.seconds,
            "message":if current.played{ "播放会话已结束"} else if !last_error.is_empty(){ "播放器已退出，本次未取得有效播放证据"} else{ "播放会话已结束"},
        }),
    );
    Ok(())
}
fn recover_sessions(conn: &rusqlite::Connection) -> Result<(), String> {
    // 退出前最后一个持久化检查点已包含真实累计秒数；恢复只收尾，不补计停机时间。
    conn.execute("UPDATE view_sessions SET ended_at=COALESCE((SELECT NULLIF(last_played,'') FROM personal WHERE item_id=view_sessions.item_id AND last_played>=view_sessions.started_at),started_at) WHERE ended_at=''",
[]).map_err(|e|e.to_string())?;

    Ok(())
}
pub fn start(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        if let Ok(_operation) = super::operation(&app).await {
            if let Ok(conn) = store::open(&dir(&app)) {
                let _ = recover_sessions(&conn);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sample(pos: f64, running: bool) -> Sample {
        Sample {
            path: "x".into(),
            position: pos,
            duration: 100.0,
            running,
            stopped: false,
            buffering: false,
            seeking: false,
            speed: Some(1.0),
            ..Default::default()
        }
    }
    #[test]
    fn pause_seek_sleep_and_speed_are_not_process_wall_time() {
        assert!(valid_interval(
            &sample(10.0, true),
            &sample(11.0, true),
            1.0
        ));
        assert!(!valid_interval(
            &sample(10.0, false),
            &sample(10.0, false),
            1.0
        ));
        assert!(!valid_interval(
            &sample(10.0, true),
            &sample(90.0, true),
            1.0
        ));
        assert!(!valid_interval(
            &sample(10.0, true),
            &sample(11.0, true),
            60.0
        ));
        let mut fast = sample(12.0, true);
        fast.speed = Some(2.0);
        assert!(valid_interval(&sample(10.0, true), &fast, 1.0));
    }
    #[test]
    fn potplayer_poll_interval_has_default_and_rejects_invalid_values() {
        assert_eq!(potplayer_poll_interval_ms(None).unwrap(), 500);
        for interval in [50, 100, 500, 2000] {
            assert_eq!(
                potplayer_poll_interval_ms(Some(&json!(interval))).unwrap(),
                interval
            );
        }
        for invalid in [
            json!(0),
            json!(49),
            json!(2001),
            json!(-1),
            json!(100.5),
            json!("100"),
            Value::Null,
        ] {
            assert!(potplayer_poll_interval_ms(Some(&invalid)).is_err());
        }
    }
    #[test]
    fn initial_seek_confirms_with_frequent_samples() {
        let mut gate = InitialSeek::new(60.0, 0.0);
        assert_eq!(
            gate.observe(&sample(60.0, true), 0.0),
            InitialSeekAction::Wait
        );
        assert_eq!(
            gate.observe(&sample(60.05, true), 0.05),
            InitialSeekAction::Wait
        );
        for tick in 1..=20 {
            let now = 0.05 + f64::from(tick) * 0.05;
            let action = gate.observe(&sample(60.0 + now - 0.05, true), now);
            if action == InitialSeekAction::Confirmed {
                assert!(now < 1.0);
                assert_eq!(gate.attempts, 0);
                return;
            }
            assert_eq!(action, InitialSeekAction::Wait);
        }
        panic!("频繁采样也应累计足够的连续确认时间");
    }
    #[test]
    fn command_line_position_within_five_seconds_does_not_seek_again() {
        for (target, position, running) in [
            (60.0, 55.0, true),
            (60.0, 65.0, false),
            (0.0, 0.0, false),
            (0.0, 0.1, true),
            (4.0, 0.1, true),
        ] {
            let mut gate = InitialSeek::new(target, 0.0);
            assert_eq!(
                gate.observe(&sample(position, running), 0.0),
                InitialSeekAction::Wait
            );
            assert_eq!(
                gate.observe(&sample(position, running), 0.5),
                InitialSeekAction::Wait
            );
            assert_eq!(
                gate.observe(&sample(position, running), 1.0),
                InitialSeekAction::Confirmed
            );
            assert_eq!(gate.attempts, 0);
        }
    }
    #[test]
    fn command_line_seek_can_settle_during_ready_grace_period() {
        let mut gate = InitialSeek::new(60.0, 0.0);
        assert_eq!(
            gate.observe(&sample(0.0, true), 0.0),
            InitialSeekAction::Wait
        );
        assert_eq!(
            gate.observe(&sample(0.2, true), 0.5),
            InitialSeekAction::Wait
        );
        assert_eq!(
            gate.observe(&sample(60.0, true), 0.75),
            InitialSeekAction::Wait
        );
        assert_eq!(
            gate.observe(&sample(60.5, true), 1.25),
            InitialSeekAction::Confirmed
        );
        assert_eq!(gate.attempts, 0);
    }
    #[test]
    fn large_backward_or_forward_offset_triggers_fallback_seek() {
        for (target, position) in [(60.0, 54.9), (60.0, 68.0), (0.0, 30.0)] {
            let mut gate = InitialSeek::new(target, 0.0);
            assert_eq!(
                gate.observe(&sample(position, false), 0.0),
                InitialSeekAction::Wait
            );
            assert_eq!(
                gate.observe(&sample(position, false), 0.5),
                InitialSeekAction::Wait
            );
            assert_eq!(
                gate.observe(&sample(position, false), 1.0),
                InitialSeekAction::Seek(target)
            );
        }
    }
    #[test]
    fn initial_seek_waits_for_ready_and_requires_position_confirmation() {
        let mut gate = InitialSeek::new(60.0, 0.0);
        let mut loading = sample(0.0, true);
        loading.duration = 0.0;
        assert_eq!(gate.observe(&loading, 0.0), InitialSeekAction::Wait);
        let mut stopped = sample(0.0, false);
        stopped.stopped = true;
        assert_eq!(gate.observe(&stopped, 0.5), InitialSeekAction::Wait);
        assert_eq!(gate.attempts, 0);
        assert_eq!(
            gate.observe(&sample(0.0, true), 1.0),
            InitialSeekAction::Wait
        );
        assert_eq!(
            gate.observe(&sample(0.0, true), 1.5),
            InitialSeekAction::Wait
        );
        assert_eq!(
            gate.observe(&sample(0.0, true), 2.0),
            InitialSeekAction::Seek(60.0)
        );
        assert_eq!(
            gate.observe(&sample(0.0, true), 2.1),
            InitialSeekAction::Wait
        );
        // 允许关键帧回退，但必须有两次连贯的已就绪样本，不能把 seek 命令成功当成跳转成功。
        assert_eq!(
            gate.observe(&sample(55.0, true), 2.6),
            InitialSeekAction::Wait
        );
        assert_eq!(
            gate.observe(&sample(55.5, true), 3.1),
            InitialSeekAction::Confirmed
        );
    }
    #[test]
    fn unconfirmed_initial_seek_retries_are_bounded_and_zero_is_not_accepted() {
        let mut gate = InitialSeek::new(60.0, 0.0);
        assert_eq!(
            gate.observe(&sample(0.0, true), 0.0),
            InitialSeekAction::Wait
        );
        assert_eq!(
            gate.observe(&sample(0.0, true), 0.5),
            InitialSeekAction::Wait
        );
        assert_eq!(
            gate.observe(&sample(0.0, true), 1.0),
            InitialSeekAction::Seek(60.0)
        );
        assert_eq!(
            gate.observe(&sample(0.0, true), 3.1),
            InitialSeekAction::Seek(60.0)
        );
        assert_eq!(
            gate.observe(&sample(0.0, true), 5.0),
            InitialSeekAction::Wait
        );
        assert_eq!(gate.attempts, 2);
        assert_eq!(
            gate.observe(&sample(0.0, true), 21.0),
            InitialSeekAction::Unavailable
        );
        assert_eq!(gate.attempts, 2);
    }
    #[test]
    fn coverage_merges_rewatch_and_does_not_fill_skipped_range() {
        let mut coverage = Coverage::default();
        coverage.add(0.0, 10.0);
        coverage.add(5.0, 15.0);
        coverage.add(90.0, 100.0);
        assert_eq!(coverage.seconds(), 25.0);
        assert!(coverage.seconds() < 90.0);
    }
    #[test]
    fn player_paths_remove_only_supported_verbatim_prefixes() {
        assert_eq!(player_path(r"\\?\L:\youji\电影.mkv"), r"L:\youji\电影.mkv");
        assert_eq!(
            player_path(r"\\?\UNC\server\share\film.mkv"),
            r"\\server\share\film.mkv"
        );
        assert_eq!(
            player_path(r"\\?\unc\server\share\film.mkv"),
            r"\\server\share\film.mkv"
        );
        for path in [
            r"L:\youji\film.mkv",
            r"\\server\share\film.mkv",
            r"\\?\Volume{example}\film.mkv",
        ] {
            assert_eq!(player_path(path), path);
        }
        let temp = tempfile::tempdir().unwrap();
        let regular = temp.path().to_string_lossy();
        assert_eq!(
            normalized(&regular),
            normalized(&format!(r"\\?\{}", regular))
        );
    }
    #[test]
    fn selected_series_resource_uses_numeric_episode_order() {
        let root = tempfile::tempdir().unwrap();
        let conn = store::open(root.path()).unwrap();
        let series = store::new_item(&conn, &json!({"title":"剧","kind":"series"})).unwrap();
        let episode10=store::new_item(&conn,&json!({"title":"10","kind":"episode","parentId":series,"seasonNumber":1,"episodeNumber":10})).unwrap();
        let episode2=store::new_item(&conn,&json!({"title":"2","kind":"episode","parentId":series,"seasonNumber":1,"episodeNumber":2})).unwrap();
        conn.execute("INSERT INTO sources(id,path,label,kind,smart_folders,available,last_scan,error,created_at) VALUES('order-source','C:/ordering','测试','auto',1,1,'','','')",
[]).unwrap();

        for (id, item, path) in [
            ("order-10", episode10, "C:/ordering/10.mkv"),
            ("order-2", episode2.clone(), "C:/ordering/2.mkv"),
        ] {
            conn.execute("INSERT INTO resources(id,source_id,path,size,modified_at,available,missing,technical_json,position_seconds,duration_seconds,updated_at) VALUES(?1,'order-source',?2,1,'',1,0,'{}',0,100,'')",
params![id,
path]).unwrap();

            conn.execute("INSERT INTO resource_links(resource_id,item_id,method,locked) VALUES(?1,?2,'manual',1)",params![id,item]).unwrap();
        }
        assert_eq!(
            selected_resource(&conn, &json!({"itemId":series}))
                .unwrap()
                .item_id,
            episode2
        );
    }
    #[test]
    fn resume_argument_uses_real_position() {
        assert_eq!(seek_argument(3661.234), "/seek=01:01:01.234");
    }
    #[test]
    fn disk_marker_prevents_playback_before_the_next_scan() {
        let data = tempfile::tempdir().unwrap();
        let files = tempfile::tempdir().unwrap();
        let conn = store::open(data.path()).unwrap();
        let item = store::new_item(&conn, &json!({"title":"测试影片"})).unwrap();
        let path = files.path().join("Movie.mkv");
        std::fs::write(&path, b"sample").unwrap();
        conn.execute(
            "INSERT INTO sources(id,path,label,created_at) VALUES('s',?1,'示例','')",
            [files.path().to_string_lossy().as_ref()],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO resources(id,source_id,path,updated_at) VALUES('r','s',?1,'')",
            [path.to_string_lossy().as_ref()],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO resource_links(resource_id,item_id) VALUES('r',?1)",
            [&item],
        )
        .unwrap();
        assert!(resource_by_id(&conn, "r", Some(&item)).is_ok());
        std::fs::write(files.path().join(super::super::ignored::MARKER_FILE), b"").unwrap();
        assert!(resource_by_id(&conn, "r", Some(&item)).is_err());
        assert_eq!(
            conn.query_row("SELECT available FROM resources WHERE id='r'", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap(),
            1
        );
    }
    #[test]
    fn stale_checkpoint_cannot_restore_progress_or_mark_replacement_watched() {
        for change_size in [true, false] {
            let root = tempfile::tempdir().unwrap();
            let conn = store::open(root.path()).unwrap();
            let item = store::new_item(&conn, &json!({"title":"测试"})).unwrap();
            conn.execute(
                "INSERT INTO sources(id,path,label,created_at)VALUES('s','C:/test','test','')",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO resources(id,source_id,path,size,modified_at,duration_seconds,updated_at)VALUES('r','s','C:/test/film.mkv',8,'old',100,'')", [],
            ).unwrap();
            conn.execute(
                "INSERT INTO resource_links(resource_id,item_id)VALUES('r',?1)",
                [&item],
            )
            .unwrap();
            let mut session = Session::new(&conn, resource_by_id(&conn, "r", Some(&item)).unwrap());
            session.played = true;
            session.seconds = 10.0;
            session.position = 80.0;
            session.coverage.add(0.0, 80.0);
            session.checkpoint(&conn, false).unwrap();
            // 模拟扫描已经提交资源重置，但旧播放器还持有之前的版本和覆盖区间。
            conn.execute(
                if change_size {
                    "UPDATE resources SET size=9,position_seconds=0,duration_seconds=0"
                } else {
                    "UPDATE resources SET modified_at='new',position_seconds=0,duration_seconds=0"
                },
                [],
            )
            .unwrap();
            conn.execute(
                "DELETE FROM media_playback_coverage WHERE resource_id='r'",
                [],
            )
            .unwrap();
            session.position = 100.0;
            session.seconds = 20.0;
            session.coverage.add(80.0, 100.0);
            assert!(session
                .checkpoint(&conn, false)
                .unwrap_err()
                .contains("媒体文件已变化"));
            let resource = store::resource(&conn, "r").unwrap();
            assert_eq!(resource["positionSeconds"], 0.0);
            assert_eq!(resource["durationSeconds"], 0.0);
            assert_eq!(Coverage::load(&conn, "r").seconds(), 0.0);
            assert_eq!(store::item(&conn, &item).unwrap()["watched"], false);
            let history: (f64, f64) = conn
                .query_row(
                    "SELECT seconds,position_seconds FROM view_sessions WHERE id=?1",
                    [&session.id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .unwrap();
            assert_eq!(history, (10.0, 80.0));
            // 新版本重新播放仍能正常保存自己的进度。
            let mut replacement =
                Session::new(&conn, resource_by_id(&conn, "r", Some(&item)).unwrap());
            replacement.position = 5.0;
            replacement.checkpoint(&conn, false).unwrap();
            assert_eq!(store::resource(&conn, "r").unwrap()["positionSeconds"], 5.0);
        }
    }
    #[test]
    fn checkpoints_are_idempotent_and_keep_personal_date() {
        let root = tempfile::tempdir().unwrap();
        let conn = store::open(root.path()).unwrap();
        let id = store::new_item(&conn, &json!({"title":"测试"})).unwrap();
        conn.execute("INSERT INTO sources(id,path,label,kind,smart_folders,available,last_scan,error,created_at) VALUES('s','C:/test','测试','auto',1,1,'','','')",[]).unwrap();
        conn.execute("INSERT INTO resources(id,source_id,path,size,modified_at,available,missing,technical_json,position_seconds,duration_seconds,updated_at) VALUES('r','s','C:/test/a.mkv',1,'',1,0,'{}',0,100,'')",
[]).unwrap();

        conn.execute("INSERT INTO resource_links(resource_id,item_id,method,locked) VALUES('r',?1,'manual',1)",[&id]).unwrap();
        store::update_record(
            &conn,
            &id,
            &json!({"watched":false,"markedAt":"2020-01-01","rating":4,"comment":"保留短评"}),
            true,
        )
        .unwrap();
        let resource = resource_by_id(&conn, "r", Some(&id)).unwrap();
        let mut session = Session::new(&conn, resource);
        session.played = true;
        session.seconds = 10.0;
        session.position = 0.738;
        session.resume_floor = Some(4.304);
        session.checkpoint(&conn, false).unwrap();
        let saved: f64 = conn
            .query_row(
                "SELECT position_seconds FROM resources WHERE id='r'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(saved, 4.304);
        session.resume_floor = None;
        session.position = 94.99;
        session.checkpoint(&conn, false).unwrap();
        assert_eq!(store::item(&conn, &id).unwrap()["watched"], false);
        session.position = 95.0;
        assert_eq!(session.coverage.seconds(), 0.0);
        session.checkpoint(&conn, false).unwrap();
        assert_eq!(store::item(&conn, &id).unwrap()["watched"], true);
        // 同一会话已标记一次后，用户取消已看不被后续检查点覆盖。
        store::update_record(&conn, &id, &json!({"watched":false}), true).unwrap();
        session.checkpoint(&conn, false).unwrap();
        assert_eq!(store::item(&conn, &id).unwrap()["watched"], false);
        store::update_record(&conn, &id, &json!({"watched":true}), true).unwrap();
        recover_sessions(&conn).unwrap();
        let recovered: (f64, String) = conn
            .query_row("SELECT seconds,ended_at FROM view_sessions", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!(recovered.0, 10.0);
        assert_eq!(recovered.1, session.last_played_at);
        session.checkpoint(&conn, true).unwrap();
        let count: i64 = conn
            .query_row("SELECT count(*) FROM view_sessions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
        let item = store::item(&conn, &id).unwrap();
        assert_eq!(item["rating"].as_f64(), Some(4.0));
        assert_eq!(item["comment"], "保留短评");
        assert_eq!(item["markedAt"], "2020-01-01");
        assert_eq!(item["watched"], true);
    }
}
