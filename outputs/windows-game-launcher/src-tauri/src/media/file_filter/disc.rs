//! 只读取受限的光盘目录结构，不挂载镜像或扫描影片数据。
#[path = "disc/udf.rs"]
mod udf;
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
};

const BLOCK: u64 = 2048;
const MAX_DIRECTORY: usize = 64 * 1024;
const MAX_READ: usize = 1024 * 1024;
const INVALID: &str = "媒体镜像目录结构无效或超出读取预算";

struct Reader {
    file: File,
    length: u64,
    remaining: usize,
}
impl Reader {
    fn read(&mut self, offset: u64, length: usize) -> Result<Vec<u8>, String> {
        if length > self.remaining
            || offset
                .checked_add(length as u64)
                .is_none_or(|end| end > self.length)
        {
            return Err(INVALID.into());
        }
        self.remaining -= length;
        let mut data = vec![0; length];
        self.file
            .seek(SeekFrom::Start(offset))
            .and_then(|_| self.file.read_exact(&mut data))
            .map_err(|_| "无法读取媒体镜像")?;
        Ok(data)
    }
    fn block(&mut self, sector: u64) -> Result<Vec<u8>, String> {
        self.read(sector.checked_mul(BLOCK).ok_or(INVALID)?, BLOCK as usize)
    }
}
fn u16(data: &[u8], offset: usize) -> Result<u16, String> {
    let bytes: [u8; 2] = data
        .get(offset..offset.checked_add(2).ok_or(INVALID)?)
        .ok_or(INVALID)?
        .try_into()
        .map_err(|_| INVALID)?;
    Ok(u16::from_le_bytes(bytes))
}
fn u32(data: &[u8], offset: usize) -> Result<u32, String> {
    let bytes: [u8; 4] = data
        .get(offset..offset.checked_add(4).ok_or(INVALID)?)
        .ok_or(INVALID)?
        .try_into()
        .map_err(|_| INVALID)?;
    Ok(u32::from_le_bytes(bytes))
}
fn u64(data: &[u8], offset: usize) -> Result<u64, String> {
    let bytes: [u8; 8] = data
        .get(offset..offset.checked_add(8).ok_or(INVALID)?)
        .ok_or(INVALID)?
        .try_into()
        .map_err(|_| INVALID)?;
    Ok(u64::from_le_bytes(bytes))
}
fn video_directory(name: &str) -> bool {
    ["BDMV", "VIDEO_TS", "MPEGAV"]
        .iter()
        .any(|expected| name.eq_ignore_ascii_case(expected))
}
fn iso9660(reader: &mut Reader, pvd: &[u8]) -> Result<bool, String> {
    if u16(pvd, 128)? != BLOCK as u16 || pvd.get(130..132) != Some(&[8, 0]) {
        return Err(INVALID.into());
    }
    let volume_blocks = u32(pvd, 80)? as u64;
    if pvd.get(84..88) != Some(&(volume_blocks as u32).to_be_bytes())
        || volume_blocks == 0
        || volume_blocks
            .checked_mul(BLOCK)
            .is_none_or(|end| end > reader.length)
    {
        return Err(INVALID.into());
    }
    let root = pvd.get(156..190).ok_or(INVALID)?;
    if root[0] < 34 || root[25] & 2 == 0 {
        return Err(INVALID.into());
    }
    let length = u32(root, 10)? as usize;
    if length > MAX_DIRECTORY {
        return Err(INVALID.into());
    }
    let sector = (u32(root, 2)? as u64)
        .checked_add(root[1] as u64)
        .ok_or(INVALID)?;
    if root.get(6..10) != Some(&u32(root, 2)?.to_be_bytes())
        || root.get(14..18) != Some(&(length as u32).to_be_bytes())
        || sector
            .checked_mul(BLOCK)
            .and_then(|offset| offset.checked_add(length as u64))
            .is_none_or(|end| end > volume_blocks * BLOCK)
    {
        return Err(INVALID.into());
    }
    let data = reader.read(sector.checked_mul(BLOCK).ok_or(INVALID)?, length)?;
    let mut offset = 0;
    while offset < data.len() {
        let size = data[offset] as usize;
        if size == 0 {
            offset = ((offset / (BLOCK as usize)) + 1) * (BLOCK as usize);
            continue;
        }
        if size < 34
            || offset + size > data.len()
            || offset % (BLOCK as usize) + size > BLOCK as usize
        {
            return Err(INVALID.into());
        }
        let record = &data[offset..offset + size];
        let name_length = record[32] as usize;
        let name = record.get(33..33 + name_length).ok_or(INVALID)?;
        if record[25] & 2 != 0 {
            let name = std::str::from_utf8(name).unwrap_or("");
            if video_directory(name.strip_suffix(";1").unwrap_or(name)) {
                return Ok(true);
            }
        }
        offset += size;
    }
    Ok(false)
}

pub(super) fn has_video_tree(path: &Path) -> Result<bool, String> {
    let file = File::open(path).map_err(|_| "无法读取媒体镜像")?;
    let length = file.metadata().map_err(|_| "无法读取媒体镜像信息")?.len();
    if length < 17 * BLOCK {
        return Ok(false);
    }
    let mut reader = Reader {
        file,
        length,
        remaining: MAX_READ,
    };
    let mut primary = None;
    let mut has_udf = false;
    let mut bea = false;
    // VRS 自 LBA 16 开始，最多到锚点前；未知普通二进制立即结束。
    for sector in 16..256.min(length / BLOCK) {
        let data = reader.block(sector)?;
        let identifier = &data[1..6];
        match identifier {
            b"CD001" => {
                if data[6] != 1 {
                    return Err(INVALID.into());
                }
                if data[0] == 1 && primary.is_none() {
                    primary = Some(data);
                }
            }
            b"BEA01" => {
                if data[0] != 0 || data[6] != 1 {
                    return Err(INVALID.into());
                }
                bea = true;
            }
            b"NSR02" | b"NSR03" if bea => {
                if data[0] != 0 || data[6] != 1 {
                    return Err(INVALID.into());
                }
                has_udf = true;
                break;
            }
            b"TEA01" => break,
            b"BOOT2" => {}
            _ => break,
        }
    }
    if let Some(primary) = primary {
        if iso9660(&mut reader, &primary)? {
            return Ok(true);
        }
    }
    if has_udf {
        udf::has_video_tree(&mut reader)
    } else {
        Ok(false)
    }
}

#[cfg(test)]
#[path = "disc/tests.rs"]
mod tests;
