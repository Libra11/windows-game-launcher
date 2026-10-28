use super::*;

fn put16(data: &mut [u8], offset: usize, value: u16) {
    data[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}
fn put32(data: &mut [u8], offset: usize, value: u32) {
    data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}
fn put64(data: &mut [u8], offset: usize, value: u64) {
    data[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}
fn check(data: &[u8]) -> Result<bool, String> {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("disc.img");
    std::fs::write(&path, data).unwrap();
    has_video_tree(&path)
}
fn iso_record(name: &[u8], directory: bool) -> Vec<u8> {
    let size = 33 + name.len() + usize::from(name.len() % 2 == 0);
    let mut data = vec![0; size];
    data[0] = size as u8;
    data[25] = if directory { 2 } else { 0 };
    data[32] = name.len() as u8;
    data[33..33 + name.len()].copy_from_slice(name);
    data
}
fn iso(name: &str, directory: bool) -> Vec<u8> {
    let mut image = vec![0; 64 * BLOCK as usize];
    let pvd = &mut image[16 * BLOCK as usize..17 * BLOCK as usize];
    pvd[0] = 1;
    pvd[1..6].copy_from_slice(b"CD001");
    pvd[6] = 1;
    put32(pvd, 80, 64);
    pvd[84..88].copy_from_slice(&64u32.to_be_bytes());
    put16(pvd, 128, 2048);
    pvd[130..132].copy_from_slice(&2048u16.to_be_bytes());
    let mut root = iso_record(&[0], true);
    put32(&mut root, 2, 20);
    root[6..10].copy_from_slice(&20u32.to_be_bytes());
    put32(&mut root, 10, 2048);
    root[14..18].copy_from_slice(&2048u32.to_be_bytes());
    pvd[156..190].copy_from_slice(&root);
    let end = &mut image[17 * BLOCK as usize..18 * BLOCK as usize];
    end[0] = 255;
    end[1..6].copy_from_slice(b"CD001");
    end[6] = 1;
    let record = iso_record(name.as_bytes(), directory);
    image[20 * BLOCK as usize..20 * BLOCK as usize + record.len()].copy_from_slice(&record);
    image
}
fn tag(data: &mut [u8], id: u16, location: u32) {
    put16(data, 0, id);
    put16(data, 2, 3);
    put32(data, 12, location);
    put16(data, 10, (data.len() - 16) as u16);
    let mut crc = 0u16;
    for byte in &data[16..] {
        crc ^= (*byte as u16) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    put16(data, 8, crc);
    data[4] = data
        .iter()
        .take(16)
        .enumerate()
        .filter(|(i, _)| *i != 4)
        .fold(0u8, |sum, (_, b)| sum.wrapping_add(*b));
}
fn long_ad(data: &mut [u8], offset: usize, block: u32, partition: u16) {
    put32(data, offset, 2048);
    put32(data, offset + 4, block);
    put16(data, offset + 8, partition);
}
fn udf(name: &str, metadata: bool, inline: bool) -> Vec<u8> {
    let mut image = vec![0; 600 * BLOCK as usize];
    for (sector, id) in [(16, b"BEA01"), (17, b"NSR03")] {
        image[sector * BLOCK as usize + 1..sector * BLOCK as usize + 6].copy_from_slice(id);
        image[sector * BLOCK as usize + 6] = 1;
    }
    let block = &mut image[256 * BLOCK as usize..256 * BLOCK as usize + 512];
    put32(block, 16, 3 * 2048);
    put32(block, 20, 32);
    tag(block, 2, 256);
    let pd = &mut image[32 * BLOCK as usize..32 * BLOCK as usize + 512];
    put32(pd, 188, 288);
    put32(pd, 192, 256);
    tag(pd, 5, 32);
    let lvd = &mut image[33 * BLOCK as usize..33 * BLOCK as usize + 512];
    put32(lvd, 212, 2048);
    lvd[217..236].copy_from_slice(b"*OSTA UDF Compliant");
    long_ad(lvd, 248, 0, if metadata { 1 } else { 0 });
    put32(lvd, 264, if metadata { 70 } else { 6 });
    put32(lvd, 268, if metadata { 2 } else { 1 });
    lvd[440] = 1;
    lvd[441] = 6;
    if metadata {
        lvd[446] = 2;
        lvd[447] = 64;
        lvd[451..474].copy_from_slice(b"*UDF Metadata Partition");
        // physical partition number 0 and metadata file location 0 are already zero.
    }
    tag(lvd, 6, 33);
    tag(
        &mut image[34 * BLOCK as usize..34 * BLOCK as usize + 512],
        8,
        34,
    );
    let base = if metadata { 320 } else { 288 };
    if metadata {
        let entry = &mut image[288 * BLOCK as usize..288 * BLOCK as usize + 512];
        put16(entry, 20, 4);
        entry[27] = 250;
        put64(entry, 56, 128 * BLOCK);
        put32(entry, 212, 8);
        put32(entry, 216, 128 * 2048);
        put32(entry, 220, 32);
        tag(entry, 266, 0);
    }
    let fsd = &mut image[base * BLOCK as usize..base * BLOCK as usize + 512];
    long_ad(fsd, 400, 1, if metadata { 1 } else { 0 });
    tag(fsd, 256, 0);
    let raw = [vec![8], name.as_bytes().to_vec()].concat();
    let size = (38 + raw.len() + 3) & !3;
    let mut fid = vec![0; size];
    fid[18] = 2;
    fid[19] = raw.len() as u8;
    fid[38..38 + raw.len()].copy_from_slice(&raw);
    tag(&mut fid, 257, 2);
    let root = &mut image[(base + 1) * BLOCK as usize..(base + 1) * BLOCK as usize + 512];
    put16(root, 20, 4);
    root[27] = 4;
    put64(root, 56, fid.len() as u64);
    if inline {
        put16(root, 34, 3);
        put32(root, 212, fid.len() as u32);
        root[216..216 + fid.len()].copy_from_slice(&fid);
    } else {
        put32(root, 212, 8);
        put32(root, 216, fid.len() as u32);
        put32(root, 220, 2);
    }
    tag(root, 266, 1);
    if !inline {
        image[(base + 2) * BLOCK as usize..(base + 2) * BLOCK as usize + fid.len()]
            .copy_from_slice(&fid);
    }
    image
}

#[test]
fn iso9660_accepts_video_directories_but_not_files_or_software_images() {
    for name in ["BDMV", "VIDEO_TS", "MPEGAV"] {
        assert!(check(&iso(name, true)).unwrap());
    }
    assert!(!check(&iso("BDMV", false)).unwrap());
    assert!(!check(&iso("Windows", true)).unwrap());
    assert!(!check(b"QFI\xfb not an optical disc").unwrap());
}
#[test]
fn pure_udf_supports_physical_metadata_and_inline_root_directories() {
    for metadata in [false, true] {
        for inline in [false, true] {
            assert!(check(&udf("BDMV", metadata, inline)).unwrap());
            assert!(check(&udf("VIDEO_TS", metadata, inline)).unwrap());
            assert!(!check(&udf("Windows", metadata, inline)).unwrap());
        }
    }
}
#[test]
fn malformed_directory_offsets_tags_and_allocations_fail_within_budget() {
    let mut bad = iso("BDMV", true);
    let pvd = &mut bad[16 * BLOCK as usize..17 * BLOCK as usize];
    put32(pvd, 158, u32::MAX);
    pvd[162..166].copy_from_slice(&u32::MAX.to_be_bytes());
    assert!(check(&bad).is_err());
    let mut bad = iso("BDMV", true);
    put32(
        &mut bad,
        16 * BLOCK as usize + 166,
        (MAX_DIRECTORY + 1) as u32,
    );
    assert!(check(&bad).is_err());
    let mut bad = udf("BDMV", true, false);
    bad[256 * BLOCK as usize + 4] ^= 1;
    assert!(check(&bad).is_err());
    let mut bad = udf("BDMV", true, false);
    let logical = &mut bad[33 * BLOCK as usize..33 * BLOCK as usize + 512];
    put32(logical, 264, u32::MAX);
    tag(logical, 6, 33);
    assert!(check(&bad).is_err());
    let mut bad = udf("BDMV", true, false);
    let entry = &mut bad[321 * BLOCK as usize..321 * BLOCK as usize + 512];
    put32(entry, 208, u32::MAX);
    tag(entry, 266, 1);
    assert!(check(&bad).is_err());
}
