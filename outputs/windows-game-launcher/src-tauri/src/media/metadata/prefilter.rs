//! 只使用搜索结果中明确的电影年份；不按显示名或整剧首播年排除候选。
use super::store;
use serde_json::Value;

pub(super) fn movie_year_differs(row: &Value, year: Option<u16>) -> bool {
    if row["providerType"] != "movie" {
        return false;
    }
    let actual = store::text(row, "year");
    if actual.len() != 4 || !actual.bytes().all(|c| c.is_ascii_digit()) {
        return false;
    }
    year.zip(actual.parse::<u16>().ok())
        .is_some_and(|(expected, actual)| expected != actual)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn retains_unknown_dates_aliases_and_season_years() {
        assert!(movie_year_differs(
            &json!({"providerType":"movie","year":"1999"}),
            Some(2024)
        ));
        for row in [
            json!({"providerType":"movie","year":"2024","title":"别名不在显示名中"}),
            json!({"providerType":"tv","year":"1999"}),
            json!({"providerType":"movie"}),
            json!({"providerType":"movie","year":"????"}),
        ] {
            assert!(!movie_year_differs(&row, Some(2024)));
        }
        assert!(!movie_year_differs(
            &json!({"providerType":"movie","year":"1999"}),
            None
        ));
    }
}
