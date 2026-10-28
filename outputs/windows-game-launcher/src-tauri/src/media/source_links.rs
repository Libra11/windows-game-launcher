use super::store;
use serde_json::Value;

fn url(input: &Value) -> Result<String, String> {
    let id = store::text(input, "externalId");
    let numbers: Vec<_> = id.split(':').collect();
    let numeric = |value: &&str| {
        !value.is_empty() && value.len() <= 20 && value.bytes().all(|c| c.is_ascii_digit())
    };
    let valid = numbers.iter().all(numeric);
    let path = match (
        store::text(input, "provider"),
        store::text(input, "providerType"),
    ) {
        ("douban", "subject") if valid && numbers.len() == 1 => {
            format!("https://movie.douban.com/subject/{id}/")
        }
        ("tmdb", "movie") if valid && numbers.len() == 1 => {
            format!("https://www.themoviedb.org/movie/{id}")
        }
        ("tmdb", "tv") if valid && numbers.len() == 1 => {
            format!("https://www.themoviedb.org/tv/{id}")
        }
        ("tmdb", "season") if valid && numbers.len() == 2 => format!(
            "https://www.themoviedb.org/tv/{}/season/{}",
            numbers[0], numbers[1]
        ),
        ("tmdb", "episode") if valid && numbers.len() == 3 => format!(
            "https://www.themoviedb.org/tv/{}/season/{}/episode/{}",
            numbers[0], numbers[1], numbers[2]
        ),
        ("imdb", _) if store::douban_source::valid_imdb_id(id) => {
            format!("https://www.imdb.com/title/{id}/")
        }
        _ => return Err("作品来源链接无效".into()),
    };
    Ok(path)
}

pub async fn open(_app: tauri::AppHandle, input: Value) -> Result<Value, String> {
    crate::epic_auth::open_browser(&url(&input)?)?;
    Ok(serde_json::json!({"opened":true}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn links_only_allow_known_provider_id_routes() {
        assert_eq!(
            url(&json!({"provider":"tmdb","providerType":"episode","externalId":"100:0:2"}))
                .unwrap(),
            "https://www.themoviedb.org/tv/100/season/0/episode/2"
        );
        assert_eq!(
            url(&json!({"provider":"douban","providerType":"subject","externalId":"123"})).unwrap(),
            "https://movie.douban.com/subject/123/"
        );
        for id in ["", "1/../../evil", "123?url=evil", "1:2:3:4"] {
            assert!(
                url(&json!({"provider":"douban","providerType":"subject","externalId":id}))
                    .is_err()
            );
        }
    }
}
