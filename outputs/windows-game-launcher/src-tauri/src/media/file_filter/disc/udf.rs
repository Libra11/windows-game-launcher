//! ECMA-167 根目录所需的最小只读子集：物理与 UDF metadata 分区。
use super::{u16, u32, u64, video_directory, Reader, BLOCK, INVALID, MAX_DIRECTORY};
use std::collections::HashMap;

#[derive(Clone)]
struct Span {
    offset: u64,
    length: u64,
}
#[derive(Clone)]
struct Partition {
    spans: Vec<Span>,
    length: u64,
}
impl Partition {
    fn read(&self, reader: &mut Reader, position: u64, length: usize) -> Result<Vec<u8>, String> {
        if position
            .checked_add(length as u64)
            .is_none_or(|end| end > self.length)
            || length > reader.remaining
        {
            return Err(INVALID.into());
        }
        let mut result = Vec::with_capacity(length);
        let mut skip = position;
        for span in &self.spans {
            if skip >= span.length {
                skip -= span.length;
                continue;
            }
            let take = (span.length - skip).min((length - result.len()) as u64) as usize;
            result.extend(reader.read(span.offset.checked_add(skip).ok_or(INVALID)?, take)?);
            skip = 0;
            if result.len() == length {
                return Ok(result);
            }
        }
        if result.len() == length {
            Ok(result)
        } else {
            Err(INVALID.into())
        }
    }
    fn spans(&self, position: u64, length: u64) -> Result<Vec<Span>, String> {
        if position
            .checked_add(length)
            .is_none_or(|end| end > self.length)
        {
            return Err(INVALID.into());
        }
        let mut result = Vec::new();
        let mut skip = position;
        let mut remaining = length;
        for span in &self.spans {
            if skip >= span.length {
                skip -= span.length;
                continue;
            }
            let take = (span.length - skip).min(remaining);
            result.push(Span {
                offset: span.offset.checked_add(skip).ok_or(INVALID)?,
                length: take,
            });
            remaining -= take;
            skip = 0;
            if remaining == 0 {
                return Ok(result);
            }
        }
        Err(INVALID.into())
    }
}
#[derive(Clone)]
struct Address {
    length: u32,
    block: u32,
    partition: usize,
}
fn address(data: &[u8], offset: usize) -> Result<Address, String> {
    let length = u32(data, offset)?;
    if length >> 30 != 0 {
        return Err(INVALID.into());
    }
    Ok(Address {
        length,
        block: u32(data, offset + 4)?,
        partition: u16(data, offset + 8)? as usize,
    })
}
fn tag(data: &[u8]) -> Result<u16, String> {
    if data.len() < 16 || !matches!(u16(data, 2)?, 2 | 3) {
        return Err(INVALID.into());
    }
    let sum = data[..16]
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != 4)
        .fold(0u8, |sum, (_, byte)| sum.wrapping_add(*byte));
    if sum != data[4] {
        return Err(INVALID.into());
    }
    let length = u16(data, 10)? as usize;
    let content = data.get(16..16 + length).ok_or(INVALID)?;
    let mut crc = 0u16;
    for byte in content {
        crc ^= (*byte as u16) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    if crc != u16(data, 8)? {
        return Err(INVALID.into());
    }
    u16(data, 0)
}
struct Entry {
    kind: u8,
    length: u64,
    inline: Option<Vec<u8>>,
    addresses: Vec<Address>,
}
fn entry(data: &[u8], partition: usize) -> Result<Entry, String> {
    let id = tag(data)?;
    let base = match id {
        261 => 176,
        266 => 216,
        _ => return Err(INVALID.into()),
    };
    if u16(data, 20)? != 4 {
        return Err(INVALID.into());
    }
    let attributes = u32(data, base - 8)? as usize;
    let allocation = u32(data, base - 4)? as usize;
    let start = base.checked_add(attributes).ok_or(INVALID)?;
    let end = start.checked_add(allocation).ok_or(INVALID)?;
    let allocation_data = data.get(start..end).ok_or(INVALID)?;
    let length = u64(data, 56)?;
    let flags = u16(data, 34)? & 7;
    let mut result = Entry {
        kind: *data.get(27).ok_or(INVALID)?,
        length,
        inline: None,
        addresses: Vec::new(),
    };
    if flags == 3 {
        if length > allocation_data.len() as u64 {
            return Err(INVALID.into());
        }
        result.inline = Some(allocation_data[..length as usize].to_vec());
        return Ok(result);
    }
    let step = match flags {
        0 => 8,
        1 => 16,
        _ => return Err(INVALID.into()),
    };
    if allocation % step != 0 || allocation / step > 32 {
        return Err(INVALID.into());
    }
    for offset in (0..allocation).step_by(step) {
        let ad = if flags == 0 {
            let length = u32(allocation_data, offset)?;
            if length >> 30 != 0 {
                return Err(INVALID.into());
            }
            Address {
                length,
                block: u32(allocation_data, offset + 4)?,
                partition,
            }
        } else {
            address(allocation_data, offset)?
        };
        if ad.length != 0 {
            result.addresses.push(ad);
        }
    }
    Ok(result)
}
fn read_address(
    reader: &mut Reader,
    partitions: &[Option<Partition>],
    ad: &Address,
    length: usize,
) -> Result<Vec<u8>, String> {
    if ad.length == 0 {
        return Err(INVALID.into());
    }
    let partition = partitions
        .get(ad.partition)
        .and_then(Option::as_ref)
        .ok_or(INVALID)?;
    partition.read(
        reader,
        (ad.block as u64).checked_mul(BLOCK).ok_or(INVALID)?,
        length,
    )
}
fn directory(data: &[u8]) -> Result<bool, String> {
    let mut offset = 0usize;
    while offset < data.len() {
        if data[offset] == 0 {
            let next = ((offset / (BLOCK as usize) + 1) * (BLOCK as usize)).min(data.len());
            if data[offset..next].iter().any(|byte| *byte != 0) {
                return Err(INVALID.into());
            }
            offset = next;
            continue;
        }
        let fid = data.get(offset..).ok_or(INVALID)?;
        let flags = *fid.get(18).ok_or(INVALID)?;
        let length = *fid.get(19).ok_or(INVALID)? as usize;
        let implementation = u16(fid, 36)? as usize;
        let start = 38usize.checked_add(implementation).ok_or(INVALID)?;
        let end = start.checked_add(length).ok_or(INVALID)?;
        let raw = fid.get(start..end).ok_or(INVALID)?;
        let size = end.checked_add(3).ok_or(INVALID)? & !3;
        if size < 38 || size > fid.len() || tag(&fid[..size])? != 257 {
            return Err(INVALID.into());
        }
        if flags & 2 != 0 && flags & (4 | 8) == 0 && !raw.is_empty() {
            let name = match raw[0] {
                8 => raw[1..].iter().map(|b| *b as char).collect::<String>(),
                16 if (raw.len() - 1) % 2 == 0 => {
                    let chars = raw[1..]
                        .chunks_exact(2)
                        .map(|p| u16::from_be_bytes([p[0], p[1]]))
                        .collect::<Vec<_>>();
                    String::from_utf16(&chars).map_err(|_| INVALID)?
                }
                _ => return Err(INVALID.into()),
            };
            if video_directory(&name) {
                return Ok(true);
            }
        }
        offset = offset.checked_add(size).ok_or(INVALID)?;
    }
    Ok(false)
}

pub(super) fn has_video_tree(reader: &mut Reader) -> Result<bool, String> {
    // Anchor、Main VDS、FSD、根 ICB 与根目录；没有沿文件树递归或读取影片。
    let mut anchor = None;
    let sectors = reader.length / BLOCK;
    for sector in [Some(256), sectors.checked_sub(1), sectors.checked_sub(257)]
        .into_iter()
        .flatten()
    {
        if sector >= sectors {
            continue;
        }
        let block = reader.block(sector)?;
        if tag(&block).ok() == Some(2) {
            anchor = Some(block);
            break;
        }
    }
    let anchor = anchor.ok_or(INVALID)?;
    let length = u32(&anchor, 16)? as usize;
    let start = u32(&anchor, 20)? as u64;
    if length == 0 || length > 128 * 1024 || length % BLOCK as usize != 0 {
        return Err(INVALID.into());
    }
    let descriptors = reader.read(start.checked_mul(BLOCK).ok_or(INVALID)?, length)?;
    let mut physical = HashMap::new();
    let mut logical = None;
    for block in descriptors.chunks_exact(BLOCK as usize) {
        match tag(block)? {
            5 => {
                let start = (u32(block, 188)? as u64)
                    .checked_mul(BLOCK)
                    .ok_or(INVALID)?;
                let length = (u32(block, 192)? as u64)
                    .checked_mul(BLOCK)
                    .ok_or(INVALID)?;
                if length == 0
                    || start
                        .checked_add(length)
                        .is_none_or(|end| end > reader.length)
                    || physical.len() >= 8
                {
                    return Err(INVALID.into());
                }
                physical.insert(
                    u16(block, 22)?,
                    Partition {
                        spans: vec![Span {
                            offset: start,
                            length,
                        }],
                        length,
                    },
                );
            }
            6 => logical = Some(block.to_vec()),
            8 => break,
            1 | 4 | 7 => {}
            _ => return Err(INVALID.into()),
        }
    }
    let logical = logical.ok_or(INVALID)?;
    if u32(&logical, 212)? != BLOCK as u32 || !logical[217..240].starts_with(b"*OSTA UDF Compliant")
    {
        return Err(INVALID.into());
    }
    let count = u32(&logical, 268)? as usize;
    let length = u32(&logical, 264)? as usize;
    if count == 0 || count > 8 {
        return Err(INVALID.into());
    }
    let maps = logical
        .get(440..440usize.checked_add(length).ok_or(INVALID)?)
        .ok_or(INVALID)?;
    let mut partitions = vec![None; count];
    let mut metadata = Vec::new();
    let mut offset = 0usize;
    for (index, slot) in partitions.iter_mut().enumerate() {
        let length = *maps.get(offset + 1).ok_or(INVALID)? as usize;
        if length < 2 {
            return Err(INVALID.into());
        }
        let map = maps
            .get(offset..offset.checked_add(length).ok_or(INVALID)?)
            .ok_or(INVALID)?;
        match (map[0], length) {
            (1, 6) => *slot = Some(physical.get(&u16(map, 4)?).ok_or(INVALID)?.clone()),
            (2, 64) if map[5..28].starts_with(b"*UDF Metadata Partition") => {
                metadata.push((index, u16(map, 38)?, u32(map, 40)?))
            }
            _ => return Err(INVALID.into()),
        }
        offset += length;
    }
    if offset != maps.len() {
        return Err(INVALID.into());
    }
    for (index, number, lba) in metadata {
        let direct = physical.get(&number).ok_or(INVALID)?;
        let direct_index = partitions
            .iter()
            .position(|part| {
                part.as_ref().is_some_and(|part| {
                    part.spans.len() == 1 && part.spans[0].offset == direct.spans[0].offset
                })
            })
            .ok_or(INVALID)?;
        let file = entry(
            &direct.read(reader, lba as u64 * BLOCK, BLOCK as usize)?,
            direct_index,
        )?;
        if file.kind != 250 || file.inline.is_some() {
            return Err(INVALID.into());
        }
        let mut spans = Vec::new();
        let mut total = 0u64;
        for ad in file.addresses {
            let part = partitions
                .get(ad.partition)
                .and_then(Option::as_ref)
                .ok_or(INVALID)?;
            spans.extend(part.spans(ad.block as u64 * BLOCK, ad.length as u64)?);
            total = total.checked_add(ad.length as u64).ok_or(INVALID)?;
        }
        if file.length == 0 || file.length > total || spans.len() > 32 {
            return Err(INVALID.into());
        }
        partitions[index] = Some(Partition {
            spans,
            length: file.length,
        });
    }
    let fsd = read_address(
        reader,
        &partitions,
        &address(&logical, 248)?,
        BLOCK as usize,
    )?;
    if tag(&fsd)? != 256 {
        return Err(INVALID.into());
    }
    let root = address(&fsd, 400)?;
    let entry = entry(
        &read_address(reader, &partitions, &root, BLOCK as usize)?,
        root.partition,
    )?;
    if entry.kind != 4 || entry.length > MAX_DIRECTORY as u64 {
        return Err(INVALID.into());
    }
    if let Some(inline) = entry.inline {
        return directory(&inline);
    }
    let mut data = Vec::with_capacity(entry.length as usize);
    for ad in entry.addresses {
        let length = (entry.length as usize - data.len()).min(ad.length as usize);
        if length == 0 {
            break;
        }
        data.extend(read_address(reader, &partitions, &ad, length)?);
    }
    if data.len() != entry.length as usize {
        return Err(INVALID.into());
    }
    directory(&data)
}
