#[path = "model/filename.rs"]
mod filename;

#[derive(Clone, Debug, PartialEq)]
pub struct ParsedName {
    pub title: String,
    pub year: Option<u16>,
    pub season: Option<u32>,
    pub episodes: Vec<u32>,
    pub explicit_tmdb: Option<(String, String)>,
    pub ambiguous_season: bool,
}
pub fn is_video(path: &std::path::Path) -> bool {
    path.extension().and_then(|s| s.to_str()).is_some_and(|s| {
        matches!(
            s.to_ascii_lowercase().as_str(),
            "mkv"
                | "mp4"
                | "m4v"
                | "avi"
                | "mov"
                | "wmv"
                | "ts"
                | "m2ts"
                | "webm"
                | "mpg"
                | "mpeg"
                | "flv"
                | "rmvb"
                | "iso"
                | "img"
        )
    })
}
pub fn parse_name(raw: &str) -> ParsedName {
    let stem = if is_video(std::path::Path::new(raw)) {
        std::path::Path::new(raw)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or(raw.into())
    } else {
        raw.into()
    };
    let (stem, prefix_year) = filename::prepare(&stem);
    let lower = stem.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let mut season = None;
    let mut episodes = Vec::new();
    let mut cut = stem.len();
    if let Some(digits) = lower
        .strip_prefix('s')
        .filter(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()))
    {
        season = digits.parse().ok();
        cut = 0;
    }
    for marker in ["season ", "season.", "season_", "season-"] {
        if let Some(index) = lower.find(marker) {
            let digits: String = lower[index + marker.len()..]
                .chars()
                .take_while(char::is_ascii_digit)
                .collect();
            if let Ok(number) = digits.parse::<u32>() {
                season = Some(number);
                cut = cut.min(index);
            }
        }
    }
    for i in 0..bytes.len() {
        if bytes[i] == b's' && (i == 0 || !bytes[i - 1].is_ascii_alphanumeric()) {
            let mut j = i + 1;
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
            }
            if j > i + 1 && (j == bytes.len() || !bytes[j].is_ascii_alphanumeric()) {
                season = lower[i + 1..j].parse().ok();
                cut = cut.min(i);
            }
            if j > i + 1 && j < bytes.len() && bytes[j] == b'e' {
                let start = j + 1 + usize::from(bytes.get(j + 1) == Some(&b'p'));
                let mut end = start;
                while end < bytes.len() && bytes[end].is_ascii_digit() {
                    end += 1;
                }
                if end > start
                    && (end == bytes.len()
                        || !bytes[end].is_ascii_alphanumeric()
                        || bytes[end] == b'e')
                {
                    season = lower[i + 1..j].parse().ok();
                    if let Ok(n) = lower[start..end].parse() {
                        episodes.push(n);
                    }
                    if end < bytes.len() && bytes[end] == b'e' {
                        let start = end + 1;
                        end = start;
                        while end < bytes.len() && bytes[end].is_ascii_digit() {
                            end += 1;
                        }
                        if let Ok(n) = lower[start..end].parse() {
                            episodes.push(n);
                        }
                    }
                    cut = cut.min(i);
                    break;
                }
            }
        }
    }
    if episodes.is_empty() {
        if let Some((index, episode)) = filename::episode_marker(&stem) {
            episodes.push(episode);
            cut = cut.min(index);
        }
    }
    // 中文／日文的季集号只接受完整的 第…季/集/話 边界。
    for (index, _) in stem.match_indices('第') {
        let tail = &stem[index + '第'.len_utf8()..];
        if let Some(end) = tail.find(['季', '集', '話']) {
            let number = &tail[..end];
            let marker = tail[end..].chars().next().unwrap();
            if let Some(number) = chinese_number(number) {
                if marker == '季' {
                    season.get_or_insert(number);
                } else if episodes.is_empty() {
                    episodes.push(number);
                }
                cut = cut.min(index);
            }
        }
    }
    let tokens: Vec<(usize, &str)> = stem
        .char_indices()
        .filter_map(|(i, c)| {
            if i == 0
                || stem[..i]
                    .chars()
                    .next_back()
                    .is_some_and(|v: char| !v.is_alphanumeric())
            {
                if c.is_alphanumeric() {
                    let end = stem[i..]
                        .find(|v: char| !v.is_alphanumeric())
                        .map(|n| i + n)
                        .unwrap_or(stem.len());
                    Some((i, &stem[i..end]))
                } else {
                    None
                }
            } else {
                None
            }
        })
        .collect();
    let mut year = prefix_year;
    for (i, t) in &tokens {
        if t.len() == 4 {
            if let Ok(y) = t.parse::<u16>() {
                if (1900..=2100).contains(&y) {
                    // 《1917》《1984》《2001 太空漫游》等数字片名后仍可带真正发行年。
                    if *i == 0
                        && tokens.iter().any(|(index, token)| {
                            *index > 0
                                && token
                                    .parse::<u16>()
                                    .is_ok_and(|year| (1900..=2100).contains(&year))
                        })
                    {
                        continue;
                    }
                    year = Some(y);
                    cut = cut.min(*i);
                    break;
                }
            }
        }
    }
    for (i, t) in &tokens {
        let t = t.to_ascii_lowercase();
        if matches!(
            t.as_str(),
            "2160p"
                | "1080p"
                | "720p"
                | "480p"
                | "4k"
                | "web"
                | "bluray"
                | "bdrip"
                | "webrip"
                | "h264"
                | "h265"
                | "x264"
                | "x265"
                | "hevc"
                | "avc"
                | "hdr"
                | "sdr"
                | "remux"
                | "hdtv"
                | "webdl"
                | "dvdrip"
                | "av1"
        ) {
            cut = cut.min(*i);
        }
    }
    let mut title = stem[..cut]
        .trim_matches(|c: char| !c.is_alphanumeric())
        .replace(['.', '_'], " ");
    if title
        .chars()
        .any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c))
    {
        if let Some((i, _)) = title.char_indices().find(|(_, c)| c.is_ascii_alphabetic()) {
            if !title[i..]
                .chars()
                .any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c))
            {
                title = title[..i].to_string();
            }
        }
        for mark in [
            "第一季",
            "第二季",
            "第三季",
            "第四季",
            "第五季",
            "第六季",
            "第七季",
            "第八季",
            "第九季",
            "第十季",
        ] {
            if let Some(i) = title.find(mark) {
                if season.is_none() {
                    season = Some(match mark {
                        "第一季" => 1,
                        "第二季" => 2,
                        "第三季" => 3,
                        "第四季" => 4,
                        "第五季" => 5,
                        "第六季" => 6,
                        "第七季" => 7,
                        "第八季" => 8,
                        "第九季" => 9,
                        _ => 10,
                    });
                }
                title.truncate(i);
                break;
            }
        }
    }
    title = title.split_whitespace().collect::<Vec<_>>().join(" ");
    if title.is_empty() {
        title = stem;
    }
    let ambiguous_season = filename::season_collection(&lower);
    if ambiguous_season {
        season = None;
    }
    if filename::year_range(&lower) {
        year = None;
    }
    let explicit_tmdb = lower.find("{tmdb-").and_then(|start| {
        let value = &lower[start + 6..];
        let end = value.find('}')?;
        let value = &value[..end];
        if !value.is_empty() && value.chars().all(|c| c.is_ascii_digit()) {
            Some((
                if season.is_some() || !episodes.is_empty() || ambiguous_season {
                    "tv"
                } else {
                    "movie"
                }
                .into(),
                value.into(),
            ))
        } else {
            None
        }
    });
    ParsedName {
        title,
        year,
        season,
        episodes,
        explicit_tmdb,
        ambiguous_season,
    }
}
fn chinese_number(text: &str) -> Option<u32> {
    if !text.is_empty() && text.chars().all(|c| c.is_ascii_digit()) {
        return text.parse().ok();
    }
    fn digit(c: char) -> Option<u32> {
        match c {
            '零' | '〇' => Some(0),
            '一' => Some(1),
            '二' | '两' => Some(2),
            '三' => Some(3),
            '四' => Some(4),
            '五' => Some(5),
            '六' => Some(6),
            '七' => Some(7),
            '八' => Some(8),
            '九' => Some(9),
            _ => None,
        }
    }
    let chars: Vec<_> = text.chars().collect();
    match chars.as_slice() {
        [c] => {
            if *c == '十' {
                Some(10)
            } else {
                digit(*c)
            }
        }
        ['十', b] => Some(10 + digit(*b)?),
        [a, '十'] => Some(digit(*a)? * 10),
        [a, '十', b] => Some(digit(*a)? * 10 + digit(*b)?),
        _ => None,
    }
}
pub fn normalize_title(value: &str) -> String {
    value
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}
fn generic_title(title: &str) -> bool {
    let normalized = normalize_title(title);
    filename::storage_folder(title)
        || normalized
            .strip_prefix('第')
            .and_then(|rest| {
                rest.strip_suffix('集')
                    .or_else(|| rest.strip_suffix('季'))
                    .or_else(|| rest.strip_suffix('話'))
            })
            .is_some_and(|number| chinese_number(number).is_some())
        || normalized.chars().all(|c| c.is_ascii_digit())
        || ["season", "s", "episode", "ep", "e"].iter().any(|prefix| {
            normalized.strip_prefix(prefix).is_some_and(|rest| {
                !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit() || c == 'e')
            })
        })
}
pub fn meaningful_title(title: &str) -> bool {
    !generic_title(title) && !normalize_title(title).is_empty()
}
pub fn parse_path(path: &std::path::Path) -> ParsedName {
    let mut parsed = parse_name(
        &path
            .file_name()
            .map(|name| name.to_string_lossy())
            .unwrap_or_default(),
    );
    let bare = std::path::Path::new(&parsed.title)
        .file_stem()
        .and_then(|v| v.to_str())
        .unwrap_or(&parsed.title)
        .to_string();
    let explicit_episode = !parsed.episodes.is_empty();
    if parsed.episodes.is_empty() && bare.len() <= 3 && bare.chars().all(|c| c.is_ascii_digit()) {
        if let Ok(number) = bare.parse::<u32>() {
            parsed.episodes.push(number);
        }
    }
    let numeric_identity = (parsed.year.is_some() || parsed.season.is_some() || explicit_episode)
        && parsed.title.chars().all(|c| c.is_ascii_digit());
    let needs_title = generic_title(&parsed.title) && !numeric_identity;
    let own_names = path_titles(path, &parsed);
    let mut contextual_title = false;
    for ancestor in path.ancestors().skip(1).take(3) {
        let Some(name) = ancestor.file_name() else {
            continue;
        };
        let parent = parse_name(&name.to_string_lossy());
        let same_work = own_names
            .iter()
            .any(|name| normalize_title(name) == normalize_title(&parent.title));
        if parent.ambiguous_season
            && parsed.season.is_none()
            && parsed.year.is_none()
            && (needs_title || same_work)
        {
            parsed.ambiguous_season = true;
        }
        if parsed.season.is_none()
            && (needs_title || !parsed.episodes.is_empty())
            && (needs_title || generic_title(&parent.title) || same_work)
        {
            parsed.season = parent.season;
        }
        if !generic_title(&parent.title)
            && !parent.title.is_empty()
            && needs_title
            && !contextual_title
        {
            parsed.title = parent.title;
            contextual_title = true;
            if parsed.year.is_none() {
                parsed.year = parent.year;
            }
        } else if parsed.year.is_none() && same_work {
            parsed.year = parent.year;
        }
    }
    // 裸数字文件只有处于明确季目录时才作为某集解释。
    if parsed.season.is_none()
        && parsed.episodes.len() == 1
        && bare.chars().all(|c| c.is_ascii_digit())
        && !contextual_title
    {
        parsed.episodes.clear();
    }
    parsed
}

/// 补充同一文件名里的双语线索，不把另一个父目录名当作当前作品的别名。
pub fn path_titles(path: &std::path::Path, parsed: &ParsedName) -> Vec<String> {
    let mut names = vec![parsed.title.clone()];
    let name = path.file_stem().unwrap_or_default().to_string_lossy();
    let (prepared, _) = filename::prepare(&name);
    if let Some((index, _)) = prepared
        .char_indices()
        .find(|(_, c)| c.is_ascii_alphabetic())
    {
        let prefix = &prepared[..index];
        if prefix
            .chars()
            .any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c))
        {
            let before = parse_name(prefix);
            let alias = parse_name(&prepared[index..]);
            if before.year.is_none()
                && before.season.is_none()
                && before.episodes.is_empty()
                && alias.title.is_ascii()
                && meaningful_title(&alias.title)
                && !names.contains(&alias.title)
            {
                names.push(alias.title);
            }
        }
    }
    names
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_realistic_names_without_guessing_season() {
        let v = parse_name(
            "幸福伽菜子的快乐杀手生活.2025.S02E01.第1集.1080p.WEB-DL.SDR.H.264.AAC 2.0.mkv",
        );
        assert_eq!(v.title, "幸福伽菜子的快乐杀手生活");
        assert_eq!(v.year, Some(2025));
        assert_eq!(v.season, Some(2));
        assert_eq!(v.episodes, vec![1]);
        let v = parse_name("How.To.Make.Millions.Before.Grandma.Dies.2024.1080p.NF.WEB-DL.mkv");
        assert_eq!(v.title, "How To Make Millions Before Grandma Dies");
        assert_eq!(v.year, Some(2024));
    }
    #[test]
    fn raw_yuv_is_not_guessed() {
        assert!(!is_video(std::path::Path::new("akiyo_cif.yuv")));
    }
    #[test]
    fn parses_real_ep_release_and_chinese_markers() {
        let value = parse_name("法医秦明Medical.Examiner.Dr.Qin.S01EP01.mp4");
        assert_eq!(
            (value.title, value.season, value.episodes),
            ("法医秦明".into(), Some(1), vec![1])
        );
        let value = parse_name("法医秦明.第一季.第二十集.mp4");
        assert_eq!(
            (value.title, value.season, value.episodes),
            ("法医秦明".into(), Some(1), vec![20])
        );
        assert_eq!(parse_name("Show.S01.1080p.mkv").season, Some(1));
        assert!(parse_name("Show.S01EP01fake.mkv").episodes.is_empty());
    }
    #[test]
    fn numbered_episode_uses_explicit_season_and_series_folders() {
        let parsed = parse_path(std::path::Path::new("C:/Media/法医秦明/Season 1/01.mkv"));
        assert_eq!(parsed.title, "法医秦明");
        assert_eq!(parsed.season, Some(1));
        assert_eq!(parsed.episodes, vec![1]);
        let unknown = parse_path(std::path::Path::new("C:/Media/01.mkv"));
        assert!(unknown.season.is_none());
        assert!(unknown.episodes.is_empty());
    }

    #[test]
    fn standalone_episode_and_bracket_numbers_keep_clear_boundaries() {
        let p = parse_name("Soldiers.Sortie.E01.2006.1080p.mkv");
        assert_eq!(
            (p.title, p.year, p.season, p.episodes),
            ("Soldiers Sortie".into(), Some(2006), None, vec![1])
        );
        let p = parse_name("Some.Show.EP12.2024.mkv");
        assert_eq!((p.title, p.episodes), ("Some Show".into(), vec![12]));
        let p = parse_name("[AI-Raws] PSYCHO-PASS #01 (BD HEVC 1080p).mkv");
        assert_eq!((p.title, p.episodes), ("PSYCHO-PASS".into(), vec![1]));
        assert!(parse_name("Movie.Case.1.2019.mkv").episodes.is_empty());
        assert!(parse_name("Some.Show.E01fake.mkv").episodes.is_empty());
    }

    #[test]
    fn release_groups_and_parent_years_do_not_become_anime_titles() {
        let p=parse_path(std::path::Path::new("C:/Media/[2018][High Score Girl][BDRIP][1-15Fin+SP]/[VCB-Studio] High Score Girl [01][Hi10p_1080p][x264_flac].mkv"));
        assert_eq!(
            (p.title, p.year, p.season, p.episodes),
            ("High Score Girl".into(), Some(2018), None, vec![1])
        );
        let p = parse_path(std::path::Path::new(
            "C:/Media/Some.Show.2024/Season 2/EP03.mkv",
        ));
        assert_eq!(
            (p.title, p.year, p.season, p.episodes),
            ("Some Show".into(), Some(2024), Some(2), vec![3])
        );
        let p = parse_path(std::path::Path::new("C:/Media/某剧/第1集.mkv"));
        assert_eq!((p.title, p.episodes), ("某剧".into(), vec![1]));
        assert_eq!(parse_name("Movie.[Straws].2024.mkv").title, "Movie Straws");
        assert_eq!(
            parse_name("Movie.[Studio 54].2024.mkv").title,
            "Movie Studio 54"
        );
        let p = parse_name("[ LPSub & VCB-Studio ] Show [01][1080p].mkv");
        assert_eq!((p.title, p.episodes), ("Show".into(), vec![1]));
    }

    #[test]
    fn numeric_titles_keep_their_release_year_and_movie_identity() {
        let p = parse_name("1917.2019.1080p.mkv");
        assert_eq!((p.title, p.year), ("1917".into(), Some(2019)));
        let p = parse_name("2001.A.Space.Odyssey.1968.1080p.mkv");
        assert_eq!(
            (p.title, p.year),
            ("2001 A Space Odyssey".into(), Some(1968))
        );
        assert_eq!(
            parse_name("[The Submarine][2010].mkv").title,
            "The Submarine"
        );
        let p = parse_path(std::path::Path::new(
            "C:/Media/Other.Show.S01/24.S01E01.mkv",
        ));
        assert_eq!(p.title, "24");
        assert_eq!(parse_name("阿Q正传.1981.mkv").title, "阿Q正传");
    }

    #[test]
    fn filenames_keep_their_identity_even_inside_a_different_series_folder() {
        let path = std::path::Path::new("C:/Media/ShowB.S01.2024/ShowA.S01E01.mkv");
        let p = parse_path(path);
        assert_eq!(path_titles(path, &p), vec!["ShowA"]);
        let p = parse_path(std::path::Path::new(
            "C:/Media/ShowB.S02.2024/ShowA.E01.mkv",
        ));
        assert_eq!((p.title, p.season, p.year), ("ShowA".into(), None, None));
        let path = std::path::Path::new(
            "C:/Media/Other.Show.2020/法医秦明.Medical.Examiner.Dr.Qin.S01E01.2016.mkv",
        );
        let p = parse_path(path);
        assert_eq!(
            path_titles(path, &p),
            vec!["法医秦明", "Medical Examiner Dr Qin"]
        );
    }

    #[test]
    fn multi_season_and_year_ranges_never_supply_a_specific_season() {
        let p = parse_path(std::path::Path::new(
            "C:/Media/Show.S01-S07.2012-2019/Show.E01.mkv",
        ));
        assert_eq!(
            (p.title, p.season, p.year, p.ambiguous_season),
            ("Show".into(), None, None, true)
        );
        let p = parse_path(std::path::Path::new(
            "C:/Media/Show.S01-S07.2012-2019/Show.S02E01.mkv",
        ));
        assert_eq!(
            (p.title, p.season, p.year, p.ambiguous_season),
            ("Show".into(), Some(2), None, false)
        );
        let p = parse_name("Show.Season.1-7.2012-2019");
        assert!(p.ambiguous_season);
        assert_eq!((p.season, p.year), (None, None));
    }

    #[test]
    fn japanese_episode_markers_and_explicit_rip_groups_do_not_infer_seasons() {
        let p = parse_name("[U2-Rip] けいおん!! 第08話「進路!」(BD 1080p x264 AACx3).mkv");
        assert_eq!(
            (p.title, p.episodes, p.season, p.year),
            ("けいおん".into(), vec![8], None, None)
        );
        let p = parse_name("[U2-Rip] Another.Show (BD 1080p x264 AAC).mkv");
        assert_eq!(p.title, "Another Show");
        let p = parse_name("[The Rip].2025.1080p.mkv");
        assert_eq!((p.title, p.episodes), ("The Rip".into(), vec![]));
        let p = parse_name("The.Rip.2025.mkv");
        assert_eq!(p.title, "The Rip");
        assert!(parse_name("けいおん!! 第八話予告.mkv").season.is_none());
        assert!(parse_name("第08回の記録.2020.mkv").episodes.is_empty());
    }

    #[test]
    fn explicit_rating_suffixes_do_not_become_title_words() {
        let p = parse_name("喜劇　愛妻物語[PG12指定] 2019 1080p HDTV x264 AAC5.1.mkv");
        assert_eq!((p.title, p.year), ("喜劇 愛妻物語".into(), Some(2019)));
        assert_eq!(parse_name("Some.Movie[PG-13].2020.mkv").title, "Some Movie");
        assert_eq!(parse_name("[PG].2020.mkv").title, "PG");
        assert_eq!(parse_name("[R].2020.mkv").title, "R");
    }

    #[test]
    fn fin_inside_real_title_words_is_not_a_release_end_marker() {
        assert_eq!(
            parse_name("Movie.[Infinity].2024.mkv").title,
            "Movie Infinity"
        );
        assert_eq!(
            parse_name("Movie.[Defining Moments].2024.mkv").title,
            "Movie Defining Moments"
        );
        assert_eq!(parse_name("[Show][01-12Fin+SP][BDRIP].mkv").title, "Show");
        assert_eq!(parse_name("Show.[Complete].1080p.mkv").title, "Show");
        assert_eq!(
            parse_name("Movie.[Finland].2024.mkv").title,
            "Movie Finland"
        );
    }
}
