use super::store;
use std::{fs, path::Path};
pub fn create(
    dir: &Path,
    profile_id: &str,
    automatic: bool,
) -> Result<Option<store::Snapshot>, String> {
    let mut c = store::open(dir)?;
    let mut b = store::binding(&c, &profile_id)?;
    let mut settings = super::tasks::settings(&c, &b.id)?;
    if automatic && !settings.auto_backup {
        return Ok(None);
    }
    let current = youji_save_archive::scan(Path::new(&b.root))?;
    let current_hash = youji_save_archive::manifest_hash(&current)?;
    if automatic && current.files.is_empty() {
        return Err("需要处理：存档目录为空，请检查目录或手动确认备份".into());
    }
    if automatic && settings.baseline.as_deref() == Some(&current_hash) {
        return Ok(None);
    }
    let id = uuid::Uuid::new_v4().to_string();
    let archive = dir.join(format!("{id}.zip"));
    let manifest = youji_save_archive::create(Path::new(&b.root), &archive)?;
    if automatic && manifest.files.is_empty() {
        fs::remove_file(&archive).map_err(|e| e.to_string())?;
        return Err("需要处理：归档时存档目录变空，请检查目录".into());
    }
    let manifest_hash = youji_save_archive::manifest_hash(&manifest)?;
    if automatic && settings.baseline.as_deref() == Some(&manifest_hash) {
        fs::remove_file(&archive).map_err(|e| e.to_string())?;
        return Ok(None);
    }
    let s = store::Snapshot {
        id: id.clone(),
        profile_id: b.id.clone(),
        archive: archive.to_string_lossy().into_owned(),
        manifest_hash,
        archive_hash: youji_save_archive::file_hash(&archive)?,
        bytes: fs::metadata(&archive).map_err(|e| e.to_string())?.len(),
        parent_ids: b.parent_id.clone().into_iter().collect(),
        base_revision: b.revision,
        created_at: chrono::Utc::now().to_rfc3339(),
        uploaded: false,
        upload_started: false,
        error: None,
    };
    b.parent_id = Some(id);
    let tx = c.transaction().map_err(|e| e.to_string())?;
    store::put_snapshot(&tx, &s)?;
    store::put_binding(&tx, &b)?;
    settings.baseline = Some(s.manifest_hash.clone());
    settings.notice = Some("本地已备份".into());
    settings.last_success = Some(s.created_at.clone());
    super::tasks::put(&tx, &b.id, &settings)?;
    if settings.auto_upload {
        super::tasks::enqueue(&tx, &b.id, "upload")?;
    }
    tx.commit().map_err(|e| e.to_string())?;
    super::retention::prune(&c, &dir, &b, &settings)?;
    Ok(Some(s))
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::saves::{restore, tasks};
    fn fixture() -> (tempfile::TempDir, std::path::PathBuf, store::Binding) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("game");
        fs::create_dir(&root).unwrap();
        fs::write(root.join("progress"), b"one").unwrap();
        let data = dir.path().join("backups");
        let c = store::open(&data).unwrap();
        let b = store::Binding {
            id: "profile".into(),
            game_id: "game".into(),
            name: "save".into(),
            root: root.to_string_lossy().into(),
            cloud_id: None,
            parent_id: None,
            revision: 0,
        };
        store::put_binding(&c, &b).unwrap();
        tasks::put(
            &c,
            &b.id,
            &tasks::Settings {
                auto_backup: true,
                ..Default::default()
            },
        )
        .unwrap();
        (dir, data, b)
    }
    #[test]
    fn unchanged_is_skipped_restart_and_restore_keep_baseline() {
        let (_dir, data, b) = fixture();
        let first = create(&data, &b.id, true).unwrap().unwrap();
        assert!(create(&data, &b.id, true).unwrap().is_none());
        fs::write(Path::new(&b.root).join("progress"), b"two").unwrap();
        let second = create(&data, &b.id, true).unwrap().unwrap();
        assert_ne!(first.manifest_hash, second.manifest_hash);
        let c = store::open(&data).unwrap();
        let job = restore::prepare(&c, &data, &b, &first).unwrap();
        restore::apply(&c, &data, job).unwrap();
        drop(c);
        assert!(create(&data, &b.id, true).unwrap().is_none());
        assert_eq!(
            fs::read(Path::new(&b.root).join("progress")).unwrap(),
            b"one"
        );
    }
    #[test]
    fn empty_requires_manual_confirmation_and_disabled_never_runs() {
        let (_dir, data, b) = fixture();
        fs::remove_file(Path::new(&b.root).join("progress")).unwrap();
        assert!(create(&data, &b.id, true).is_err());
        assert!(create(&data, &b.id, false).unwrap().is_some());
        tasks::put(&store::open(&data).unwrap(), &b.id, &Default::default()).unwrap();
        assert!(create(&data, &b.id, true).unwrap().is_none());
    }
    #[test]
    fn backup_atomically_enqueues_upload_and_retention_keeps_pending() {
        let (_dir, data, b) = fixture();
        let c = store::open(&data).unwrap();
        let mut s = tasks::settings(&c, &b.id).unwrap();
        s.auto_upload = true;
        tasks::put(&c, &b.id, &s).unwrap();
        for i in 0..13 {
            fs::write(Path::new(&b.root).join("progress"), i.to_string()).unwrap();
            create(&data, &b.id, true).unwrap();
        }
        assert_eq!(tasks::list(&c).unwrap().len(), 1);
        assert_eq!(
            store::snapshots(&c, &b.id)
                .unwrap()
                .iter()
                .filter(|s| Path::new(&s.archive).exists())
                .count(),
            13
        );
    }
    #[test]
    fn local_retention_preserves_metadata_and_latest_ten() {
        let (_dir, data, b) = fixture();
        for i in 0..13 {
            fs::write(Path::new(&b.root).join("progress"), i.to_string()).unwrap();
            create(&data, &b.id, true).unwrap();
        }
        let c = store::open(&data).unwrap();
        let all = store::snapshots(&c, &b.id).unwrap();
        assert_eq!(all.len(), 13);
        assert_eq!(
            all.iter()
                .filter(|s| Path::new(&s.archive).exists())
                .count(),
            10
        );
        assert_eq!(
            super::super::retention::flags(&c)
                .unwrap()
                .values()
                .filter(|v| v["available"] == false)
                .count(),
            3
        );
    }
}
