//! Local file eligibility is separate from matching a name to an online work.
mod disc;

use super::{model, store};
use serde_json::Value;
use std::{fs::File, io::Read, path::Path};

fn extension(path: &Path) -> String {
    path.extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
}

fn sidecar(path: &Path) -> bool {
    path.file_name()
        .and_then(|value| value.to_str())
        .is_some_and(|name| name.starts_with("._"))
}

fn transport_stream(path: &Path) -> Result<bool, String> {
    let mut bytes = Vec::with_capacity(4096);
    File::open(path)
        .and_then(|file| file.take(4096).read_to_end(&mut bytes))
        .map_err(|_| "无法验证 TS 视频封装")?;
    // MPEG-TS, M2TS and DVB packets; require four consecutive valid headers.
    Ok([188, 192, 204].into_iter().any(|width| {
        (0..width.min(bytes.len())).any(|offset| {
            (0..4).all(|packet| {
                let start = offset + packet * width;
                bytes.get(start) == Some(&0x47)
                    && bytes.get(start + 3).is_some_and(|value| value & 0x30 != 0)
            })
        })
    }))
}

pub(super) fn scan_candidate(path: &Path) -> Result<bool, String> {
    if sidecar(path) {
        return Ok(false);
    }
    match extension(path).as_str() {
        "ts" => transport_stream(path),
        "iso" | "img" => disc::has_video_tree(path),
        "yuv" => Ok(true),
        _ => Ok(model::is_video(path)),
    }
}

pub(super) fn extra(path: &Path) -> bool {
    let name = path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_ascii_lowercase();
    if [
        "[menu",
        "[ncop",
        "[nced",
        "[cm ",
        "[pv ",
        "[game pv]",
        "[creditless",
        "【预告片】",
        "【样片】",
    ]
    .iter()
    .any(|marker| name.contains(marker))
    {
        return true;
    }
    let words: Vec<_> = name
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|s| !s.is_empty())
        .collect();
    for (index, word) in words.iter().enumerate() {
        if matches!(*word, "ncop" | "nced" | "creditless") {
            return true;
        }
        if !matches!(*word, "sample" | "trailer" | "preview" | "featurette") {
            continue;
        }
        // A title such as "Sample People" or "The Trailer" is still a title.
        // Only treat these words as extras after clear release/episode tokens.
        if words[..index].iter().any(|word| {
            word.parse::<u16>()
                .is_ok_and(|year| (1900..=2100).contains(&year))
                || matches!(
                    *word,
                    "480p" | "720p" | "1080p" | "2160p" | "bluray" | "bdrip" | "webrip"
                )
                || word.starts_with('s')
                    && word.contains('e')
                    && word.chars().skip(1).all(|c| c.is_ascii_digit() || c == 'e')
        }) {
            return true;
        }
    }
    false
}

pub(super) fn automatic_rejection(resource: &Value) -> Result<Option<String>, String> {
    let path = Path::new(store::text(resource, "path"));
    if sidecar(path) {
        return Ok(Some("系统伴随文件不作为影视资源识别".into()));
    }
    if extra(path) {
        return Ok(Some("样片、预告或附加片不自动关联为正片".into()));
    }
    match extension(path).as_str() {
        "ts" => match transport_stream(path) {
            Ok(true) => {}
            Ok(false) => return Ok(Some("文件不是有效的 TS 视频封装，不自动识别".into())),
            Err(error) => return Err(error),
        },
        "iso" | "img" => match disc::has_video_tree(path) {
            Ok(true) => {}
            Ok(false) => return Ok(Some("镜像中未确认 DVD/蓝光视频目录，不自动识别".into())),
            Err(error) => return Err(error),
        },
        "yuv" => return Ok(Some("原始 YUV 缺少可靠作品及封装信息，请手动确认".into())),
        _ => {}
    }
    if resource["technical"]["state"] == "ready"
        && resource["technical"]["video"]
            .as_array()
            .is_some_and(Vec::is_empty)
    {
        return Ok(Some("媒体探测未发现视频流，不自动识别为影视作品".into()));
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn sidecars_and_source_code_are_not_video_candidates() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("node.ts");
        std::fs::write(&source, b"export const result = 42;").unwrap();
        assert!(!scan_candidate(&source).unwrap());
        assert!(!scan_candidate(&dir.path().join("._Movie.2024.mkv")).unwrap());
        assert!(scan_candidate(&dir.path().join("Movie.2024.mkv")).unwrap());
    }
    #[test]
    fn valid_transport_packets_are_kept_and_read_errors_are_not_absence() {
        let dir = tempfile::tempdir().unwrap();
        for width in [188, 192, 204] {
            let path = dir.path().join(format!("{width}.ts"));
            let mut bytes = vec![0; width * 5];
            for n in 0..5 {
                bytes[n * width] = 0x47;
                bytes[n * width + 3] = 0x10;
            }
            std::fs::write(&path, bytes).unwrap();
            assert!(scan_candidate(&path).unwrap());
        }
        assert!(scan_candidate(&dir.path().join("missing.ts")).is_err());
        assert!(automatic_rejection(
            &json!({"path":dir.path().join("missing.ts").to_string_lossy()})
        )
        .is_err());
    }
    #[test]
    fn only_explicit_extras_are_rejected_not_short_films_or_title_words() {
        for name in [
            "Movie.2024.1080p.Sample.mkv",
            "Series.S01E01.1080p.trailer.mp4",
            "[Group] Series [NCOP][1080p].mkv",
            "[Title][Game PV][1080p].mkv",
        ] {
            assert!(
                automatic_rejection(&json!({"path":name}))
                    .unwrap()
                    .is_some(),
                "{name}"
            );
        }
        for name in [
            "Sample.People.2000.mkv",
            "The.Trailer.2016.mkv",
            "Movie.2024.mkv",
            "Show.S00E01.Special.mkv",
        ] {
            assert!(
                automatic_rejection(&json!({"path":name,"durationSeconds":35}))
                    .unwrap()
                    .is_none(),
                "{name}"
            );
        }
        assert!(automatic_rejection(
            &json!({"path":"Movie.2024.mkv","technical":{"state":"ready","video":[]}})
        )
        .unwrap()
        .is_some());
    }
}
