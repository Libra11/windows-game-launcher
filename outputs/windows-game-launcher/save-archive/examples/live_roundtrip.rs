//! Live deployment smoke helper. Only operates inside a newly-created test directory.
#[path = "../../src-tauri/src/saves/capture.rs"]
mod capture;
#[path = "../../src-tauri/src/saves/restore.rs"]
pub(crate) mod restore;
#[path = "../../src-tauri/src/saves/retention.rs"]
mod retention;
#[path = "../../src-tauri/src/saves/store.rs"]
mod store;
#[path = "../../src-tauri/src/saves/tasks.rs"]
pub(crate) mod tasks;
use std::{fs, path::PathBuf};
fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args().collect();
    let action = args.get(1).ok_or("expected prepare or restore")?;
    let dir = PathBuf::from(args.get(2).ok_or("expected fresh test directory")?);
    let marker = dir.join("youji-live-test.marker");
    let data = dir.join("data");
    let root = dir.join("game");
    if action == "prepare" {
        if dir.exists() {
            return Err("test directory already exists; refusing overwrite".into());
        }
        fs::create_dir_all(root.join("slot")).map_err(|e| e.to_string())?;
        fs::write(&marker, b"youji-save-deployment-test-v1").map_err(|e| e.to_string())?;
        fs::write(root.join("slot/progress.txt"), b"level=7\n").map_err(|e| e.to_string())?;
        let c = store::open(&data)?;
        let b = store::Binding {
            id: uuid::Uuid::new_v4().to_string(),
            game_id: uuid::Uuid::new_v4().to_string(),
            name: "isolated-live-test".into(),
            root: root
                .canonicalize()
                .map_err(|e| e.to_string())?
                .to_string_lossy()
                .into(),
            cloud_id: None,
            parent_id: None,
            revision: 0,
        };
        store::put_binding(&c, &b)?;
        tasks::put(
            &c,
            &b.id,
            &tasks::Settings {
                auto_backup: true,
                ..Default::default()
            },
        )?;
        let snapshot = capture::create(&data, &b.id, true)?.ok_or("missing first backup")?;
        if capture::create(&data, &b.id, true)?.is_some() {
            return Err("unchanged backup was duplicated".into());
        }
        fs::write(
            dir.join("snapshot.json"),
            serde_json::to_vec(&snapshot).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        println!("Local automatic backup and unchanged skip: OK");
    } else if action == "restore" {
        if fs::read(&marker).map_err(|e| e.to_string())? != b"youji-save-deployment-test-v1" {
            return Err("test marker missing".into());
        }
        let mut s: store::Snapshot = serde_json::from_slice(
            &fs::read(dir.join("snapshot.json")).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        let downloaded = dir.join("downloaded.zip");
        if youji_save_archive::file_hash(&downloaded)? != s.archive_hash
            || youji_save_archive::manifest_hash(&youji_save_archive::verify(&downloaded)?)?
                != s.manifest_hash
        {
            return Err("cloud archive mismatch".into());
        }
        s.archive = downloaded.to_string_lossy().into();
        let c = store::open(&data)?;
        let b = store::binding(&c, &s.profile_id)?;
        fs::write(root.join("slot/progress.txt"), b"level=42\n").map_err(|e| e.to_string())?;
        fs::write(root.join("later-only.txt"), b"preserve in safety backup")
            .map_err(|e| e.to_string())?;
        let job = restore::prepare(&c, &data, &b, &s)?;
        let safety = job.safety.clone();
        let original = job.rollback.clone();
        restore::apply(&c, &data, job)?;
        if fs::read(root.join("slot/progress.txt")).map_err(|e| e.to_string())? != b"level=7\n"
            || root.join("later-only.txt").exists()
        {
            return Err("restored directory differs".into());
        }
        if fs::read(original.join("slot/progress.txt")).map_err(|e| e.to_string())? != b"level=42\n"
        {
            return Err("original progress was lost".into());
        }
        youji_save_archive::verify(&safety)?;
        if capture::create(&data, &b.id, true)?.is_some() {
            return Err("restored content was backed up again".into());
        }
        let job = restore::prepare(&c, &data, &b, &s)?;
        fs::write(
            root.join("slot/progress.txt"),
            b"new progress after confirmation\n",
        )
        .map_err(|e| e.to_string())?;
        if restore::apply(&c, &data, job).is_ok() {
            return Err("changed progress was overwritten".into());
        }
        restore::recover(&c, &data)?;
        if fs::read(root.join("slot/progress.txt")).map_err(|e| e.to_string())?
            != b"new progress after confirmation\n"
        {
            return Err("changed progress not preserved".into());
        }
        println!("Cloud ZIP verified; restore, safety copy, baseline, and stale confirmation protection: OK");
    } else {
        return Err("unknown action".into());
    }
    Ok(())
}

#[cfg(test)]
mod saves {
    pub(crate) use crate::{restore, tasks};
}
