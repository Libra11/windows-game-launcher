//! 发布信息清理只处理明确的分组／技术标签，保留作品名和集号。
pub(super) fn technical(value: &str) -> bool {
    value
        .to_ascii_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .any(|token| {
            matches!(
                token,
                "2160p"
                    | "1080p"
                    | "720p"
                    | "480p"
                    | "4k"
                    | "web"
                    | "webdl"
                    | "bluray"
                    | "bdrip"
                    | "webrip"
                    | "hdtv"
                    | "dvdrip"
                    | "h264"
                    | "h265"
                    | "x264"
                    | "x265"
                    | "hevc"
                    | "avc"
                    | "av1"
                    | "flac"
                    | "hi10p"
                    | "ma10p"
                    | "hdr"
                    | "sdr"
                    | "remux"
            )
        })
}

fn group(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.contains("字幕组")
        || lower.contains("dmhy")
        || lower.split('&').map(str::trim).any(|part| {
            ["-raws", "-raw", "-studio", "-labs"]
                .iter()
                .any(|suffix| part.ends_with(suffix))
                || !part.contains(' ') && part.ends_with("sub")
                || part.ends_with("-rip")
        })
}

fn rating(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_uppercase().as_str(),
        "PG12指定" | "PG12" | "PG-13" | "PG13" | "R15+" | "R18+" | "R15指定" | "R18指定"
    )
}

fn finished(value: &str) -> bool {
    let lower: String = value
        .to_ascii_lowercase()
        .chars()
        .filter(|c| !c.is_ascii_whitespace())
        .collect();
    if matches!(lower.as_str(), "fin" | "complete") {
        return true;
    }
    let first_end = lower.bytes().take_while(u8::is_ascii_digit).count();
    if first_end == 0 {
        return false;
    }
    let Some(tail) = lower[first_end..]
        .strip_prefix('-')
        .or_else(|| lower[first_end..].strip_prefix('~'))
    else {
        return false;
    };
    let last_end = tail.bytes().take_while(u8::is_ascii_digit).count();
    if last_end == 0 {
        return false;
    }
    let Some(first) = lower[..first_end].parse::<u32>().ok() else {
        return false;
    };
    let Some(last) = tail[..last_end].parse::<u32>().ok() else {
        return false;
    };
    if first > last {
        return false;
    }
    let Some(extra) = tail[last_end..].strip_prefix("fin") else {
        return false;
    };
    extra.is_empty()
        || extra.strip_prefix('+').is_some_and(|parts| {
            parts
                .split('+')
                .all(|part| matches!(part, "sp" | "ova" | "oad"))
        })
}

fn trim_technical_parentheses(value: &str) -> String {
    for (index, c) in value.char_indices() {
        let close = match c {
            '(' => ')',
            '（' => '）',
            _ => continue,
        };
        if value[..index].trim().is_empty() {
            continue;
        }
        let tail = &value[index + c.len_utf8()..];
        let content = tail.split(close).next().unwrap_or(tail);
        if technical(content) {
            return value[..index].trim().to_string();
        }
    }
    value.trim().to_string()
}

pub(super) fn prepare(raw: &str) -> (String, Option<u16>) {
    let mut output = String::new();
    let mut rest = raw;
    let mut year = None;
    while let Some(index) = rest.find('[') {
        output.push_str(&rest[..index]);
        let Some(end) = rest[index + 1..].find(']') else {
            output.push_str(&rest[index..]);
            return (output, year);
        };
        let end = index + 1 + end;
        let value = &rest[index + 1..end];
        let leading = output
            .trim_matches(|c: char| !c.is_alphanumeric())
            .is_empty();
        let bracket_year = value
            .parse::<u16>()
            .ok()
            .filter(|v| (1900..=2100).contains(v));
        if leading && bracket_year.is_some() {
            year = bracket_year;
        } else if !leading && rating(value) {
            output.push(' ');
        } else if !leading && finished(value) {
            return (output, year);
        } else if group(value) || technical(value) {
            if !leading && technical(value) {
                return (output, year);
            }
        } else if value.len() <= 3 && !value.is_empty() && value.bytes().all(|c| c.is_ascii_digit())
        {
            output.push('[');
            output.push_str(value);
            output.push(']');
        } else {
            output.push(' ');
            output.push_str(value);
            output.push(' ');
        }
        rest = &rest[end + 1..];
    }
    output.push_str(rest);
    (trim_technical_parentheses(&output), year)
}

/// 独立 E/EP、#NN 和 [NN] 都有明确边界，不把片名中普通数字当作集号。
pub(super) fn episode_marker(stem: &str) -> Option<(usize, u32)> {
    let lower = stem.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    for (index, _) in stem.char_indices() {
        let boundary = index == 0 || !bytes[index - 1].is_ascii_alphanumeric();
        if !boundary {
            continue;
        }
        let (start, close) = match bytes[index] {
            b'e' => (
                index + 1 + usize::from(bytes.get(index + 1) == Some(&b'p')),
                None,
            ),
            b'#' => (index + 1, None),
            b'[' => (index + 1, Some(b']')),
            _ => continue,
        };
        let mut end = start;
        while end < bytes.len() && bytes[end].is_ascii_digit() {
            end += 1;
        }
        if end == start || end - start > 3 {
            continue;
        }
        if close.is_some_and(|value| bytes.get(end) != Some(&value)) {
            continue;
        }
        if close.is_none() && bytes.get(end).is_some_and(u8::is_ascii_alphanumeric) {
            continue;
        }
        if let Ok(number) = lower[start..end].parse() {
            return Some((index, number));
        }
    }
    None
}

pub(super) fn storage_folder(value: &str) -> bool {
    matches!(
        super::normalize_title(value).as_str(),
        "media"
            | "movie"
            | "movies"
            | "video"
            | "videos"
            | "tv"
            | "series"
            | "bdmv"
            | "stream"
            | "影视"
            | "电影"
            | "电视剧"
    )
}

pub(super) fn season_collection(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let mut numbers = std::collections::HashSet::new();
    for index in 0..bytes.len() {
        if bytes[index] != b's' || index > 0 && bytes[index - 1].is_ascii_alphanumeric() {
            continue;
        }
        let mut end = index + 1;
        while bytes.get(end).is_some_and(u8::is_ascii_digit) {
            end += 1;
        }
        if end == index + 1 || bytes.get(end).is_some_and(u8::is_ascii_alphanumeric) {
            continue;
        }
        if let Ok(number) = lower[index + 1..end].parse::<u32>() {
            numbers.insert(number);
        }
    }
    if numbers.len() > 1 {
        return true;
    }
    for marker in ["season ", "season.", "season_", "season-"] {
        for (index, _) in lower.match_indices(marker) {
            let tail = &lower[index + marker.len()..];
            let length = tail.bytes().take_while(u8::is_ascii_digit).count();
            let Some(first) = tail.get(..length).and_then(|v| v.parse::<u32>().ok()) else {
                continue;
            };
            let Some(tail) = tail[length..]
                .strip_prefix('-')
                .or_else(|| tail[length..].strip_prefix('~'))
            else {
                continue;
            };
            let length = tail.bytes().take_while(u8::is_ascii_digit).count();
            if tail
                .get(..length)
                .and_then(|v| v.parse::<u32>().ok())
                .is_some_and(|last| last != first)
            {
                return true;
            }
        }
    }
    value.contains('第') && value.matches('季').count() > 1
}

pub(super) fn year_range(value: &str) -> bool {
    for (index, c) in value.char_indices() {
        if !c.is_ascii_digit() {
            continue;
        }
        let Some(first) = value
            .get(index..index + 4)
            .and_then(|v| v.parse::<u16>().ok())
            .filter(|v| (1900..=2100).contains(v))
        else {
            continue;
        };
        let tail = &value[index + 4..];
        let Some(tail) = tail
            .strip_prefix('-')
            .or_else(|| tail.strip_prefix('~'))
            .or_else(|| tail.strip_prefix('至'))
            .or_else(|| tail.strip_prefix('到'))
        else {
            continue;
        };
        if tail
            .get(..4)
            .and_then(|v| v.parse::<u16>().ok())
            .is_some_and(|last| last > first && last <= 2100)
        {
            return true;
        }
    }
    false
}
