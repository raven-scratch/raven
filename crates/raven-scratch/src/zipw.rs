//! A minimal ZIP writer.
//!
//! `.sb3` files are plain ZIP archives containing `project.json` plus every
//! asset named `<md5>.<ext>`. raven-asm only ever writes small archives, so entries
//! are stored uncompressed and no compression dependency is needed.
//!
//! Layout follows the PKZIP appnote:
//! local file header + data for each entry, then the central directory, then
//! the end-of-central-directory record.

/// One file to place in the archive.
pub struct Entry {
    pub name: String,
    pub data: Vec<u8>,
}

pub struct ZipWriter {
    entries: Vec<Entry>,
}

impl ZipWriter {
    pub fn new() -> Self {
        ZipWriter {
            entries: Vec::new(),
        }
    }

    /// Add a file. Names are stored as-is and must be unique.
    pub fn add(&mut self, name: impl Into<String>, data: Vec<u8>) {
        self.entries.push(Entry {
            name: name.into(),
            data,
        });
    }

    /// Serialize the archive.
    pub fn finish(self) -> Vec<u8> {
        let mut out = Vec::new();
        let mut central = Vec::new();

        for entry in &self.entries {
            let offset = out.len() as u32;
            let crc = crc32(&entry.data);
            let size = entry.data.len() as u32;
            let name = entry.name.as_bytes();

            // Local file header.
            out.extend_from_slice(&0x0403_4B50u32.to_le_bytes());
            out.extend_from_slice(&20u16.to_le_bytes()); // version needed
            out.extend_from_slice(&0x0800u16.to_le_bytes()); // UTF-8 names
            out.extend_from_slice(&0u16.to_le_bytes()); // method: stored
            out.extend_from_slice(&dos_time().to_le_bytes());
            out.extend_from_slice(&dos_date().to_le_bytes());
            out.extend_from_slice(&crc.to_le_bytes());
            out.extend_from_slice(&size.to_le_bytes());
            out.extend_from_slice(&size.to_le_bytes());
            out.extend_from_slice(&(name.len() as u16).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes()); // extra length
            out.extend_from_slice(name);
            out.extend_from_slice(&entry.data);

            // Central directory record.
            central.extend_from_slice(&0x0201_4B50u32.to_le_bytes());
            central.extend_from_slice(&20u16.to_le_bytes()); // version made by
            central.extend_from_slice(&20u16.to_le_bytes()); // version needed
            central.extend_from_slice(&0x0800u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&dos_time().to_le_bytes());
            central.extend_from_slice(&dos_date().to_le_bytes());
            central.extend_from_slice(&crc.to_le_bytes());
            central.extend_from_slice(&size.to_le_bytes());
            central.extend_from_slice(&size.to_le_bytes());
            central.extend_from_slice(&(name.len() as u16).to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes()); // extra
            central.extend_from_slice(&0u16.to_le_bytes()); // comment
            central.extend_from_slice(&0u16.to_le_bytes()); // disk number
            central.extend_from_slice(&0u16.to_le_bytes()); // internal attrs
            central.extend_from_slice(&0u32.to_le_bytes()); // external attrs
            central.extend_from_slice(&offset.to_le_bytes());
            central.extend_from_slice(name);
        }

        let central_offset = out.len() as u32;
        let central_size = central.len() as u32;
        out.extend_from_slice(&central);

        let count = self.entries.len() as u16;
        out.extend_from_slice(&0x0605_4B50u32.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // this disk
        out.extend_from_slice(&0u16.to_le_bytes()); // disk with central dir
        out.extend_from_slice(&count.to_le_bytes());
        out.extend_from_slice(&count.to_le_bytes());
        out.extend_from_slice(&central_size.to_le_bytes());
        out.extend_from_slice(&central_offset.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // comment length
        out
    }
}

impl Default for ZipWriter {
    fn default() -> Self {
        Self::new()
    }
}

/// A fixed timestamp keeps builds reproducible. 1980-01-01 00:00:00 is the
/// earliest representable DOS date.
fn dos_date() -> u16 {
    (1 << 5) | 1 // year 1980, month 1, day 1
}

fn dos_time() -> u16 {
    0
}

/// CRC-32 (IEEE 802.3), as required by the ZIP format.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32_matches_known_vectors() {
        assert_eq!(crc32(b""), 0x0000_0000);
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(
            crc32(b"The quick brown fox jumps over the lazy dog"),
            0x414F_A339
        );
    }

    #[test]
    fn archive_has_signatures_and_payload() {
        let mut zip = ZipWriter::new();
        zip.add("project.json", b"{\"a\":1}".to_vec());
        let bytes = zip.finish();
        assert_eq!(&bytes[0..4], b"PK\x03\x04");
        assert!(bytes.windows(4).any(|w| w == b"PK\x01\x02"));
        assert!(bytes.windows(4).any(|w| w == b"PK\x05\x06"));
        assert!(bytes.windows(12).any(|w| w == b"project.json"));
    }
}
