use super::*;
use std::{fs, io::Write};
#[test]
fn roundtrip_and_no_overwrite() {
    let temp = tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap();
    let root = temp.path().join("source");
    fs::create_dir_all(root.join("槽位")).unwrap();
    fs::write(root.join("槽位/save.dat"), b"saved progress").unwrap();
    let archive = temp.path().join("save.zip");
    let m = create(&root, &archive).unwrap();
    assert_eq!(verify(&archive).unwrap(), m);
    let out = temp.path().join("output");
    assert_eq!(extract(&archive, &out).unwrap(), m);
    assert_eq!(fs::read(out.join("槽位/save.dat")).unwrap(), b"saved progress");
    assert!(extract(&archive, &out).is_err());
    assert!(create(&root, &archive).is_err());
    assert!(create(&root, &root.join("bad.zip")).is_err());
}
fn crafted(name: &str, hash: &str, data: &[u8]) -> tempfile::NamedTempFile {
    let mut f = tempfile::NamedTempFile::new().unwrap();
    let manifest = Manifest { version: 1, files: [(name.into(), FileRecord { bytes: data.len() as u64, sha256: hash.into() })].into() };
    let mut zip = zip::ZipWriter::new(f.as_file_mut());
    let options = zip::write::SimpleFileOptions::default();
    zip.start_file("manifest.json", options).unwrap();
    zip.write_all(&serde_json::to_vec(&manifest).unwrap()).unwrap();
    zip.start_file(format!("files/{name}"), options).unwrap();
    zip.write_all(data).unwrap();
    zip.finish().unwrap();
    f
}
#[test]
fn traversal_device_stream_and_hash_rejected() {
    for name in ["../escape", "C:/x", "save:stream", "NUL.dat", "a./save", "a\\save"] {
        let f = crafted(name, &"a".repeat(64), b"x");
        assert!(verify(f.path()).is_err(), "{name}");
    }
    let f = crafted("save.dat", &"a".repeat(64), b"x");
    assert!(verify(f.path()).is_err());
}
#[test]
fn truncated_zip_rejected_without_extracting() {
    let f = crafted("save", &"a".repeat(64), b"x");
    f.as_file().set_len(20).unwrap();
    let temp = tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap();
    let out = temp.path().join("out");
    assert!(extract(f.path(), &out).is_err());
    assert!(!out.exists());
}
#[cfg(unix)]
#[test]
fn symlink_ancestor_and_file_rejected() {
    let temp = tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap();
    let source = temp.path().join("source");
    fs::create_dir(&source).unwrap();
    std::os::unix::fs::symlink(&source, temp.path().join("link")).unwrap();
    assert!(create(&temp.path().join("link"), &temp.path().join("a.zip")).is_err());
    std::os::unix::fs::symlink("/etc/passwd", source.join("save")).unwrap();
    assert!(create(&source, &temp.path().join("b.zip")).is_err());
}

#[test]
fn manifest_hash_matches_protocol_canonical_digest() {
    let manifest=Manifest { version:1, files:[("save.dat".into(),FileRecord { bytes:0,sha256:"a".repeat(64) })].into() };
    assert_eq!(manifest_hash(&manifest).unwrap(),youji_cloud_protocol::digest(&manifest).unwrap());
    let reordered=serde_json::json!({"files":{"save.dat":{"sha256":"a".repeat(64),"bytes":0}},"version":1});
    assert_eq!(manifest_hash(&manifest).unwrap(),youji_cloud_protocol::digest(&reordered).unwrap());
}
#[test]
fn excessive_size_and_conflicting_paths_rejected() {
    let empty=FileRecord {bytes:0,sha256:"a".repeat(64)};
    for files in [
        [("a".into(),empty.clone()),("A".into(),empty.clone())].into(),
        [("a".into(),empty.clone()),("a/b".into(),empty.clone())].into(),
        [("large".into(),FileRecord {bytes:MAX_BYTES+1,sha256:"a".repeat(64)})].into(),
    ] {
        let mut file=tempfile::NamedTempFile::new().unwrap();
        let mut zip=zip::ZipWriter::new(file.as_file_mut());
        zip.start_file("manifest.json",zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(&serde_json::to_vec(&Manifest {version:1,files}).unwrap()).unwrap();
        zip.finish().unwrap();
        assert!(verify(file.path()).is_err());
    }
}
