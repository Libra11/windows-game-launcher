//! Portable rules never contain a device's absolute path. Retention preserves graph metadata.
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashSet};
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DirectoryRule {
    pub base: String,
    pub relative: String,
    pub account: Option<String>,
}
impl DirectoryRule {
    pub fn validate(&self) -> Result<(), String> {
        if ![
            "localAppData",
            "roamingAppData",
            "documents",
            "savedGames",
            "gameInstall",
            "steamUserData",
        ]
        .contains(&self.base.as_str())
            || !crate::saves::valid_path(&self.relative)
        {
            return Err("存档目录规则无效".into());
        }
        if self.base == "steamUserData" {
            if self.account.as_ref().is_none_or(|s| {
                s.is_empty() || s.len() > 20 || !s.bytes().all(|b| b.is_ascii_digit())
            }) {
                return Err("Steam 账号规则无效".into());
            }
        } else if self.account.is_some() {
            return Err("目录规则包含无关账号".into());
        }
        Ok(())
    }
}
pub struct Version {
    pub id: String,
    pub unix_micros: i64,
    pub protected: bool,
}
pub fn retained(versions: &[Version], now: i64) -> HashSet<String> {
    let mut ordered: Vec<_> = versions.iter().collect();
    ordered.sort_by(|a, b| b.unix_micros.cmp(&a.unix_micros).then(a.id.cmp(&b.id)));
    let mut days = BTreeSet::new();
    let today = now.div_euclid(86_400_000_000);
    ordered
        .into_iter()
        .enumerate()
        .filter_map(|(i, v)| {
            let day = v.unix_micros.div_euclid(86_400_000_000);
            let daily = (today - 6..=today).contains(&day) && days.insert(day);
            (i < 10 || daily || v.protected).then(|| v.id.clone())
        })
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retention_unions_recent_daily_and_protected() {
        let now = 20 * 86_400_000_000;
        let mut v: Vec<_> = (0..15)
            .map(|i| Version {
                id: i.to_string(),
                unix_micros: now - i,
                protected: false,
            })
            .collect();
        v.push(Version {
            id: "daily".into(),
            unix_micros: now - 2 * 86_400_000_000,
            protected: false,
        });
        v.push(Version {
            id: "pinned".into(),
            unix_micros: 1,
            protected: true,
        });
        let kept = retained(&v, now);
        assert_eq!(kept.len(), 12);
        assert!(kept.contains("daily"));
        assert!(kept.contains("pinned"));
        assert!(!kept.contains("14"));
    }
    #[test]
    fn rules_reject_absolute_and_traversal() {
        for relative in ["C:/Users/name", "../save", "save/../../etc"] {
            assert!(DirectoryRule {
                base: "documents".into(),
                relative: relative.into(),
                account: None
            }
            .validate()
            .is_err());
        }
    }
}
