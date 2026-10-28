use super::{store, State};
use serde_json::{json, Value};
use std::{
    io::Read,
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
use tauri::{Emitter, Manager};

fn number(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str().and_then(|s| s.parse().ok()))
        .filter(|v| v.is_finite())
}
fn rate(value: &Value) -> Option<f64> {
    let s = value.as_str()?;
    if let Some((a, b)) = s.split_once('/') {
        let (a, b): (f64, f64) = (a.parse().ok()?, b.parse().ok()?);
        (b != 0.0).then_some(a / b)
    } else {
        s.parse().ok()
    }
}
pub fn normalize(raw: &Value) -> Value {
    let streams = raw["streams"].as_array().cloned().unwrap_or_default();
    let mut video = Vec::new();
    let mut audio = Vec::new();
    let mut subtitles = Vec::new();
    for s in streams {
        let mut track = json!({
            "index":s["index"],
            "codec":s["codec_name"],
            "codecLongName":s["codec_long_name"],
            "profile":s["profile"],
            "language":s["tags"]["language"],
            "title":s["tags"]["title"],
            "bitrate":number(&s["bit_rate"]),
            "default":s["disposition"]["default"].as_u64()==Some(1),
            "forced":s["disposition"]["forced"].as_u64()==Some(1),
        });

        match s["codec_type"].as_str() {
            Some("video") => {
                track["width"] = s["width"].clone();
                track["height"] = s["height"].clone();
                track["pixelFormat"] = s["pix_fmt"].clone();
                track["frameRate"] =
                    json!(rate(&s["avg_frame_rate"]).or_else(|| rate(&s["r_frame_rate"])));
                track["bitDepth"] = json!(number(&s["bits_per_raw_sample"]));
                track["colorPrimaries"] = s["color_primaries"].clone();
                track["colorTransfer"] = s["color_transfer"].clone();
                track["colorSpace"] = s["color_space"].clone();
                let side = s["side_data_list"].as_array().cloned().unwrap_or_default();
                let dolby = side.iter().any(|v| {
                    store::text(v, "side_data_type")
                        .to_ascii_lowercase()
                        .contains("dovi")
                });
                track["hdr"] = json!(if dolby {
                    Some("Dolby Vision")
                } else {
                    match s["color_transfer"].as_str() {
                        Some("smpte2084") => Some("PQ / HDR"),
                        Some("arib-std-b67") => Some("HLG"),
                        _ => None,
                    }
                });
                track["sideData"] = json!(side);
                video.push(track);
            }
            Some("audio") => {
                track["channels"] = s["channels"].clone();
                track["channelLayout"] = s["channel_layout"].clone();
                track["sampleRate"] = json!(number(&s["sample_rate"]));
                track["bitDepth"] = json!(number(&s["bits_per_raw_sample"]));
                audio.push(track);
            }
            Some("subtitle") => subtitles.push(track),
            _ => {}
        }
    }
    json!({
        "state":"ready",
        "container":raw["format"]["format_name"],
        "durationSeconds":number(&raw["format"]["duration"]),
        "totalBitrate":number(&raw["format"]["bit_rate"]),
        "video":video,
        "audio":audio,
        "subtitles":subtitles,
        "chapters":raw.get("chapters").cloned().unwrap_or(json!([])),
        "probedAt":store::now(),
    })
}
pub fn inspect(path: &Path, program: &Path) -> Result<Value, String> {
    let mut command = Command::new(program);
    command
        .args([
            "-v",
            "error",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
            "-show_chapters",
        ])
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command
        .spawn()
        .map_err(|_| "无法启动 ffprobe，请检查媒体探测程序路径")?;
    let stdout = child.stdout.take().ok_or("无法读取媒体探测输出")?;
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout
            .take(8 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map(|_| bytes)
    });
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|_| "媒体探测进程状态不可用")? {
            break status;
        }
        if started.elapsed() > Duration::from_secs(40) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = reader.join();
            return Err("媒体探测超时，基础文件记录已保留".into());
        }
        std::thread::sleep(Duration::from_millis(60));
    };
    let bytes = reader
        .join()
        .map_err(|_| "媒体探测输出线程失败")?
        .map_err(|_| "媒体探测输出读取失败")?;
    if !status.success() {
        return Err("媒体探测失败，文件可能不是可识别媒体或编码信息不完整".into());
    }
    if bytes.len() > 8 * 1024 * 1024 {
        return Err("媒体探测结果过大".into());
    }
    let raw: Value = serde_json::from_slice(&bytes).map_err(|_| "媒体探测结果无效")?;
    Ok(normalize(&raw))
}
pub fn probe_resource(dir: &Path, resource_id: &str) -> Result<Value, String> {
    let mut conn = store::open(dir)?;
    let resource = store::resource(&conn, resource_id)?;
    if resource["missing"] == true || resource["available"] != true {
        return Err("资源当前不可访问".into());
    }
    if resource["ignored"] == true {
        return Err("该文件已忽略，已取消媒体探测".into());
    }
    let configured = store::setting(&conn, "ffprobePath")?;
    let companion = store::setting(&conn, "ffmpegPath")?;
    let result = match super::tools::discover(
        "ffprobe.exe",
        configured.as_str().unwrap_or(""),
        companion.as_str().unwrap_or(""),
    ) {
        Some(program) => inspect(Path::new(store::text(&resource, "path")), &program),
        None => Err("未找到 ffprobe，可在影视设置中配置；文件仍可播放和关联".into()),
    };
    let value = match result {
        Ok(value) => value,
        Err(error) => json!({"state":"error","error":error,"probedAt":store::now()}),
    };
    let tx = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    if super::ignored::contains(
        &tx,
        store::text(&resource, "sourceId"),
        store::text(&resource, "path"),
    )? {
        return Err("该文件已忽略，已取消媒体探测".into());
    }
    let updated = tx.execute(
        "UPDATE resources SET technical_json=?2,duration_seconds=CASE WHEN ?3>0 THEN ?3 ELSE duration_seconds END,updated_at=?4 WHERE id=?1 AND size=?5 AND modified_at=?6 AND available=1 AND missing=0",
        rusqlite::params![resource_id,
value.to_string(),
value["durationSeconds"].as_f64().unwrap_or(0.0),
store::now(),
resource["size"].as_i64(),
store::text(&resource,
"modifiedAt")],

    ).map_err(|e|e.to_string())?;
    if updated == 0 {
        return Err("媒体文件已变化，请重新读取媒体信息".into());
    }
    tx.commit().map_err(|e| e.to_string())?;
    store::resource(&conn, resource_id)
}
pub async fn probe(app: tauri::AppHandle, input: Value) -> Result<Value, String> {
    let operation = super::operation(&app).await?;
    let dir = app.state::<State>().dir.clone();
    let id = store::text(&input, "resourceId").to_string();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let _operation = operation;
        probe_resource(&dir, &id)
    })
    .await
    .map_err(|_| "媒体探测任务失败")??;
    let _ = app.emit("media-changed", ());
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn separates_container_stream_bitrates_and_unknown_hdr() {
        let value = normalize(&json!({
            "format":{ "format_name":"matroska", "duration":"10.5", "bit_rate":"1000"},
            "streams":[{ "codec_type":"video", "codec_name":"hevc", "pix_fmt":"yuv420p10le", "width":1920, "height":1080, "avg_frame_rate":"24000/1001"} , { "codec_type":"audio", "codec_name":"aac", "bit_rate":"128000", "channels":2} ],
        }));
        assert_eq!(value["totalBitrate"], 1000.0);
        assert!(value["video"][0]["bitrate"].is_null());
        assert!(value["video"][0]["hdr"].is_null());
        assert_eq!(value["audio"][0]["bitrate"], 128000.0);
    }
}
