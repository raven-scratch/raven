//! Reading the ZIP container a `.sb3` is.
//!
//! `raven-scratch` writes these archives and deliberately never reads one, so
//! this is the reader: the end-of-central-directory record, the central
//! directory, and the two storage methods a `.sb3` ever holds — `stored`, which
//! is what raven-asm writes, and `deflate`, which is what the Scratch editor
//! writes. Anything else is reported rather than guessed at.

use crate::error::{Error, Result};
use std::io::Read;

const EOCD: u32 = 0x0605_4B50;
const CENTRAL: u32 = 0x0201_4B50;
/// The signature plus the fixed part of an end-of-central-directory record.
const EOCD_LEN: usize = 22;
/// The longest archive comment the format allows, so the longest distance back
/// from the end of a file the record can start.
const MAX_COMMENT: usize = 0xFFFF;

/// One member of the archive.
pub struct Entry {
    pub name: String,
    pub data: Vec<u8>,
}

/// The whole archive, in the order the central directory lists it.
pub struct Archive {
    pub entries: Vec<Entry>,
}

impl Archive {
    pub fn get(&self, name: &str) -> Option<&Entry> {
        self.entries.iter().find(|entry| entry.name == name)
    }
}

pub fn read(data: &[u8]) -> Result<Archive> {
    let eocd = find_eocd(data)?;
    let count = u16_at(data, eocd + 10)? as usize;
    let mut cursor = u32_at(data, eocd + 16)? as usize;

    let mut entries = Vec::with_capacity(count);
    for index in 0..count {
        if u32_at(data, cursor).ok() != Some(CENTRAL) {
            return Err(Error::msg(format!(
                "the archive's central directory is damaged: entry {index} does not start with a header"
            ))
            .note("the file is not a ZIP archive a Scratch editor wrote"));
        }
        let method = u16_at(data, cursor + 10)?;
        let compressed = u32_at(data, cursor + 20)? as usize;
        let name_len = u16_at(data, cursor + 28)? as usize;
        let extra_len = u16_at(data, cursor + 30)? as usize;
        let comment_len = u16_at(data, cursor + 32)? as usize;
        let local = u32_at(data, cursor + 42)? as usize;
        let name = string_at(data, cursor + 46, name_len)?;

        // The local header repeats the name and extra fields before the data,
        // and their lengths need not match the central directory's.
        let local_name = u16_at(data, local + 26)? as usize;
        let local_extra = u16_at(data, local + 28)? as usize;
        let start = local + 30 + local_name + local_extra;
        let raw = slice(data, start, compressed)?;

        let body = match method {
            0 => raw.to_vec(),
            8 => inflate(raw, &name)?,
            other => {
                return Err(Error::msg(format!(
                    "`{name}` uses compression method {other}, which raven-re cannot read"
                ))
                .note("vanilla Scratch writes `stored` and `deflate` entries only"))
            }
        };

        entries.push(Entry { name, data: body });
        cursor += 46 + name_len + extra_len + comment_len;
    }

    Ok(Archive { entries })
}

fn find_eocd(data: &[u8]) -> Result<usize> {
    if data.len() < EOCD_LEN {
        return Err(Error::msg("the file is too short to be a `.sb3` archive"));
    }
    let floor = data.len().saturating_sub(EOCD_LEN + MAX_COMMENT);
    let mut at = data.len() - EOCD_LEN;
    loop {
        if u32_at(data, at).ok() == Some(EOCD) {
            return Ok(at);
        }
        if at == floor {
            return Err(
                Error::msg("the file has no ZIP end-of-central-directory record")
                    .note("a `.sb3` is a ZIP archive holding `project.json` and its assets"),
            );
        }
        at -= 1;
    }
}

fn inflate(raw: &[u8], name: &str) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut decoder = flate2::read::DeflateDecoder::new(raw);
    decoder.read_to_end(&mut out).map_err(|e| {
        Error::msg(format!("`{name}` does not decompress: {e}"))
            .note("the archive is damaged, so the project cannot be read")
    })?;
    Ok(out)
}

fn u16_at(data: &[u8], at: usize) -> Result<u16> {
    let bytes = slice(data, at, 2)?;
    Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn u32_at(data: &[u8], at: usize) -> Result<u32> {
    let bytes = slice(data, at, 4)?;
    Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn string_at(data: &[u8], at: usize, len: usize) -> Result<String> {
    Ok(String::from_utf8_lossy(slice(data, at, len)?).into_owned())
}

fn slice(data: &[u8], at: usize, len: usize) -> Result<&[u8]> {
    data.get(at..at + len).ok_or_else(|| {
        Error::msg("the archive ends in the middle of an entry")
            .note("the file is truncated or damaged")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use raven_scratch::zipw::ZipWriter;
    use std::io::Write;

    /// One entry of a hand-built archive, so both storage methods can be tested
    /// without a ZIP writer that only ever stores.
    struct Raw {
        name: String,
        method: u16,
        compressed: Vec<u8>,
        size: u32,
        crc: u32,
    }

    fn deflate(body: &[u8]) -> Vec<u8> {
        let mut encoder =
            flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(body).expect("deflate");
        encoder.finish().expect("finish")
    }

    /// Assemble a ZIP archive from `(name, body, deflated?)` entries.
    fn archive(entries: &[(&str, &[u8], bool)]) -> Vec<u8> {
        let raws: Vec<Raw> = entries
            .iter()
            .map(|(name, body, compress)| Raw {
                name: (*name).to_string(),
                method: if *compress { 8 } else { 0 },
                compressed: if *compress {
                    deflate(body)
                } else {
                    body.to_vec()
                },
                size: body.len() as u32,
                crc: crc32(body),
            })
            .collect();

        let mut out = Vec::new();
        let mut offsets = Vec::new();
        for raw in &raws {
            offsets.push(out.len() as u32);
            out.extend_from_slice(&0x0403_4B50u32.to_le_bytes());
            out.extend_from_slice(&20u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&raw.method.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&raw.crc.to_le_bytes());
            out.extend_from_slice(&(raw.compressed.len() as u32).to_le_bytes());
            out.extend_from_slice(&raw.size.to_le_bytes());
            out.extend_from_slice(&(raw.name.len() as u16).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(raw.name.as_bytes());
            out.extend_from_slice(&raw.compressed);
        }

        let central_at = out.len() as u32;
        for (raw, offset) in raws.iter().zip(&offsets) {
            out.extend_from_slice(&0x0201_4B50u32.to_le_bytes());
            out.extend_from_slice(&20u16.to_le_bytes());
            out.extend_from_slice(&20u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&raw.method.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&raw.crc.to_le_bytes());
            out.extend_from_slice(&(raw.compressed.len() as u32).to_le_bytes());
            out.extend_from_slice(&raw.size.to_le_bytes());
            out.extend_from_slice(&(raw.name.len() as u16).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0u32.to_le_bytes());
            out.extend_from_slice(&offset.to_le_bytes());
            out.extend_from_slice(raw.name.as_bytes());
        }
        let central_size = out.len() as u32 - central_at;

        let count = raws.len() as u16;
        out.extend_from_slice(&0x0605_4B50u32.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&count.to_le_bytes());
        out.extend_from_slice(&count.to_le_bytes());
        out.extend_from_slice(&central_size.to_le_bytes());
        out.extend_from_slice(&central_at.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out
    }

    fn crc32(data: &[u8]) -> u32 {
        let mut crc = 0xFFFF_FFFFu32;
        for byte in data {
            crc ^= u32::from(*byte);
            for _ in 0..8 {
                let mask = (crc & 1).wrapping_neg();
                crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
            }
        }
        !crc
    }

    #[test]
    fn reads_what_raven_asm_writes() {
        let mut zip = ZipWriter::new();
        zip.add("project.json", b"{\"a\":1}".to_vec());
        let archive = read(&zip.finish()).expect("read");
        assert_eq!(archive.entries.len(), 1);
        assert_eq!(
            archive.get("project.json").expect("entry").data,
            b"{\"a\":1}"
        );
    }

    #[test]
    fn reads_stored_and_deflated_entries() {
        let bytes = archive(&[
            ("project.json", b"{\"targets\":[]}", true),
            ("asset.svg", b"<svg/>", false),
        ]);
        let archive = read(&bytes).expect("read");
        assert_eq!(
            archive.get("project.json").expect("json").data,
            b"{\"targets\":[]}"
        );
        assert_eq!(archive.get("asset.svg").expect("asset").data, b"<svg/>");
    }

    #[test]
    fn rejects_a_file_that_is_not_an_archive() {
        let error = match read(b"not a zip at all, not even close") {
            Err(error) => error,
            Ok(_) => panic!("a text file is not an archive"),
        };
        assert!(error.render().contains("end-of-central-directory"));
    }
}
