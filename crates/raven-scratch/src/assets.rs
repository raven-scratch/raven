//! Asset ingestion: hashing files the way Scratch does and recovering the
//! metadata a costume or sound needs in `project.json`.

use crate::diag::{Pos, Result, Source};
use md5::{Digest, Md5};
use std::path::Path;

/// A costume or sound file that will be packed into the `.sb3`.
#[derive(Debug, Clone)]
pub struct Asset {
    pub data: Vec<u8>,
    /// Lowercase hex MD5, which Scratch uses as the asset id.
    pub md5: String,
    /// Scratch's `dataFormat` (`svg`, `png`, `wav`, ...).
    pub data_format: String,
}

impl Asset {
    /// The archive member name Scratch expects: `<md5>.<dataFormat>`.
    pub fn filename(&self) -> String {
        format!("{}.{}", self.md5, self.data_format)
    }

    pub fn is_image(&self) -> bool {
        matches!(
            self.data_format.as_str(),
            "svg" | "png" | "jpg" | "bmp" | "gif"
        )
    }

    pub fn is_sound(&self) -> bool {
        matches!(self.data_format.as_str(), "wav" | "mp3")
    }
}

/// Read and hash an asset.
pub fn load(path: &Path, declared: &Source, pos: Pos) -> Result<Asset> {
    let data = std::fs::read(path).map_err(|e| {
        let d = declared
            .error(pos, format!("cannot read asset `{}`", path.display()))
            .note(e.to_string())
            .note("asset paths are relative to the project root, the directory holding raven-asm.toml");
        crate::diag::Error::new(d)
    })?;

    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    let data_format = normalize_format(&ext).ok_or_else(|| {
        let d = declared
            .error(pos, format!("unsupported asset type `.{ext}`"))
            .note("costumes may be .svg, .png, .jpg, .bmp or .gif; sounds may be .wav or .mp3");
        crate::diag::Error::new(d)
    })?;

    // The extension claims a format; the leading bytes know better. A file
    // that is really something else builds fine but fails in the editor, so
    // catch it here. Files whose bytes match no known signature are left alone:
    // raven-asm has no format authority over arbitrary content (SVG, for one,
    // has no magic number).
    if let Some(actual) = sniff_format(&data) {
        if actual != data_format {
            let d = declared
                .error(
                    pos,
                    format!(
                        "`{}` is {} but its extension says it is {}",
                        path.display(),
                        format_label(actual),
                        format_label(&data_format)
                    ),
                )
                .note("the file's contents must match its extension")
                .note(format!("rename the file to `.{actual}` or replace it"));
            return Err(crate::diag::Error::new(d));
        }
    }

    let mut hasher = Md5::new();
    hasher.update(&data);
    let md5 = hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();

    Ok(Asset {
        data,
        md5,
        data_format,
    })
}

fn normalize_format(ext: &str) -> Option<String> {
    let f = match ext {
        "svg" => "svg",
        "png" => "png",
        "jpg" | "jpeg" => "jpg",
        "bmp" => "bmp",
        "gif" => "gif",
        "wav" => "wav",
        "mp3" => "mp3",
        _ => return None,
    };
    Some(f.to_string())
}

/// The format a byte buffer really looks like, from its signature. Returns
/// `None` when the bytes match nothing raven-asm recognises.
fn sniff_format(data: &[u8]) -> Option<&'static str> {
    const PNG: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    if data.starts_with(PNG) {
        return Some("png");
    }
    if data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a") {
        return Some("gif");
    }
    if data.len() >= 12 && data.starts_with(b"RIFF") && &data[8..12] == b"WAVE" {
        return Some("wav");
    }
    if data.starts_with(b"BM") {
        return Some("bmp");
    }
    if data.starts_with(&[0xFF, 0xD8]) {
        return Some("jpg");
    }
    // An MP3 either carries an ID3 tag or starts on an MPEG frame sync
    // (`11111111 111xxxxx`).
    if data.starts_with(b"ID3") || (data.len() >= 2 && data[0] == 0xFF && data[1] & 0xE0 == 0xE0) {
        return Some("mp3");
    }
    None
}

fn format_label(format: &str) -> &'static str {
    match format {
        "svg" => "an SVG image",
        "png" => "a PNG image",
        "jpg" => "a JPEG image",
        "bmp" => "a BMP image",
        "gif" => "a GIF image",
        "wav" => "a WAV sound",
        "mp3" => "an MP3 sound",
        _ => "an unknown file",
    }
}

/// Pixel dimensions of an image file, when they can be determined.
pub fn image_size(asset: &Asset) -> Option<(f64, f64)> {
    match asset.data_format.as_str() {
        "svg" => svg_size(&asset.data),
        "png" => png_size(&asset.data),
        "gif" => gif_size(&asset.data),
        "bmp" => bmp_size(&asset.data),
        "jpg" => jpeg_size(&asset.data),
        _ => None,
    }
}

fn png_size(data: &[u8]) -> Option<(f64, f64)> {
    const SIG: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    if data.len() < 24 || &data[0..8] != SIG || &data[12..16] != b"IHDR" {
        return None;
    }
    let w = u32::from_be_bytes(data[16..20].try_into().ok()?);
    let h = u32::from_be_bytes(data[20..24].try_into().ok()?);
    Some((f64::from(w), f64::from(h)))
}

fn gif_size(data: &[u8]) -> Option<(f64, f64)> {
    if data.len() < 10 || (&data[0..6] != b"GIF87a" && &data[0..6] != b"GIF89a") {
        return None;
    }
    let w = u16::from_le_bytes(data[6..8].try_into().ok()?);
    let h = u16::from_le_bytes(data[8..10].try_into().ok()?);
    Some((f64::from(w), f64::from(h)))
}

fn bmp_size(data: &[u8]) -> Option<(f64, f64)> {
    if data.len() < 26 || &data[0..2] != b"BM" {
        return None;
    }
    let w = i32::from_le_bytes(data[18..22].try_into().ok()?);
    let h = i32::from_le_bytes(data[22..26].try_into().ok()?).abs();
    Some((f64::from(w), f64::from(h)))
}

fn jpeg_size(data: &[u8]) -> Option<(f64, f64)> {
    if data.len() < 4 || data[0] != 0xFF || data[1] != 0xD8 {
        return None;
    }
    let mut i = 2usize;
    while i + 1 < data.len() {
        if data[i] != 0xFF {
            i += 1;
            continue;
        }
        // A marker may be preceded by any number of 0xFF fill bytes.
        let mut marker_at = i + 1;
        while marker_at < data.len() && data[marker_at] == 0xFF {
            marker_at += 1;
        }
        if marker_at >= data.len() {
            return None;
        }
        let marker = data[marker_at];
        match marker {
            // Start of scan: entropy-coded data follows, so stop. End of image
            // means there was no frame header.
            0xDA | 0xD9 => return None,
            // Standalone markers carry no length field.
            0x01 | 0xD0..=0xD7 => {
                i = marker_at + 1;
                continue;
            }
            _ => {}
        }
        // Start-of-frame markers carry the dimensions: two length bytes, one
        // precision byte, then height and width.
        if (0xC0..=0xCF).contains(&marker) && marker != 0xC4 && marker != 0xC8 && marker != 0xCC {
            if marker_at + 8 > data.len() {
                return None;
            }
            let h = u16::from_be_bytes(data[marker_at + 4..marker_at + 6].try_into().ok()?);
            let w = u16::from_be_bytes(data[marker_at + 6..marker_at + 8].try_into().ok()?);
            return Some((f64::from(w), f64::from(h)));
        }
        if marker_at + 3 > data.len() {
            return None;
        }
        let len = u16::from_be_bytes(data[marker_at + 1..marker_at + 3].try_into().ok()?) as usize;
        if len < 2 {
            return None;
        }
        i = marker_at + 1 + len;
    }
    None
}

/// Recover an SVG's size from `width`/`height`, then `viewBox`.
fn svg_size(data: &[u8]) -> Option<(f64, f64)> {
    let text = String::from_utf8_lossy(data);
    let start = text.find("<svg")?;
    let rest = &text[start..];
    let end = rest.find('>')?;
    let tag = &rest[..end];

    let width = svg_attr(tag, "width");
    let height = svg_attr(tag, "height");
    if let (Some(w), Some(h)) = (
        width.as_deref().and_then(parse_len),
        height.as_deref().and_then(parse_len),
    ) {
        return Some((w, h));
    }

    let view_box = svg_attr(tag, "viewBox").or_else(|| svg_attr(tag, "viewbox"))?;
    let parts: Vec<f64> = view_box
        .split([' ', ',', '\t', '\n'])
        .filter(|p| !p.is_empty())
        .filter_map(|p| p.parse::<f64>().ok())
        .collect();
    if parts.len() == 4 {
        return Some((parts[2], parts[3]));
    }
    None
}

fn svg_attr(tag: &str, name: &str) -> Option<String> {
    let mut search = 0usize;
    while let Some(found) = tag[search..].find(name) {
        let idx = search + found;
        let before_ok = idx == 0
            || tag[..idx]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_whitespace());
        if !before_ok {
            search = idx + name.len();
            continue;
        }
        let after = tag[idx + name.len()..].trim_start();
        let after = after.strip_prefix('=')?;
        let after = after.trim_start();
        let quote = after.chars().next()?;
        if quote != '"' && quote != '\'' {
            return None;
        }
        let value_start = 1;
        let value_end = after[value_start..].find(quote)? + value_start;
        return Some(after[value_start..value_end].to_string());
    }
    None
}

/// Parse an SVG length. Only a plain number, optionally with a `px` suffix, is
/// a pixel length; percentages, `em`, `pt` and friends say nothing about the
/// pixel size, so they are treated as unknown and fall through to the viewBox.
fn parse_len(raw: &str) -> Option<f64> {
    let trimmed = raw.trim();
    let number = trimmed
        .strip_suffix("px")
        .or_else(|| trimmed.strip_suffix("PX"))
        .unwrap_or(trimmed)
        .trim();
    if number.is_empty()
        || !number
            .chars()
            .all(|c| c.is_ascii_digit() || matches!(c, '.' | '+' | '-' | 'e' | 'E'))
    {
        return None;
    }
    number
        .parse::<f64>()
        .ok()
        .filter(|v| *v > 0.0 && v.is_finite())
}

/// Sample rate and sample count of a WAV file, matching how the Scratch audio
/// engine reports them (frames, not bytes).
pub fn wav_info(data: &[u8]) -> Option<(u32, u32)> {
    if data.len() < 44 || &data[0..4] != b"RIFF" || &data[8..12] != b"WAVE" {
        return None;
    }
    let mut i = 12usize;
    let mut sample_rate = None;
    let mut block_align = None;
    let mut frames = None;

    while i + 8 <= data.len() {
        let id = &data[i..i + 4];
        let size = u32::from_le_bytes(data[i + 4..i + 8].try_into().ok()?) as usize;
        let body = i + 8;
        if id == b"fmt " && body + 16 <= data.len() {
            sample_rate = Some(u32::from_le_bytes(
                data[body + 4..body + 8].try_into().ok()?,
            ));
            block_align = Some(u16::from_le_bytes(
                data[body + 12..body + 14].try_into().ok()?,
            ));
        } else if id == b"data" {
            frames = Some(size);
        }
        i = body + size + (size & 1);
    }

    let rate = sample_rate?;
    let align = block_align.unwrap_or(2).max(1) as usize;
    let sample_count = frames.map(|bytes| (bytes / align) as u32);
    Some((rate, sample_count.unwrap_or(0)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(data_format: &str, data: &[u8]) -> Asset {
        Asset {
            data: data.to_vec(),
            md5: String::new(),
            data_format: data_format.to_string(),
        }
    }

    #[test]
    fn parses_svg_width_height() {
        let svg = br#"<svg version="1.1" width="480" height="360" xmlns="http://www.w3.org/2000/svg"></svg>"#;
        assert_eq!(svg_size(svg), Some((480.0, 360.0)));
    }

    #[test]
    fn parses_svg_viewbox() {
        let svg = br#"<svg viewBox="0 0 240 180"></svg>"#;
        assert_eq!(svg_size(svg), Some((240.0, 180.0)));
    }

    #[test]
    fn parses_png_dimensions() {
        let mut png = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        png.extend_from_slice(&13u32.to_be_bytes());
        png.extend_from_slice(b"IHDR");
        png.extend_from_slice(&64u32.to_be_bytes());
        png.extend_from_slice(&32u32.to_be_bytes());
        assert_eq!(png_size(&png), Some((64.0, 32.0)));
    }

    #[test]
    fn parses_gif_dimensions() {
        let mut gif = b"GIF89a".to_vec();
        gif.extend_from_slice(&120u16.to_le_bytes());
        gif.extend_from_slice(&90u16.to_le_bytes());
        assert_eq!(gif_size(&gif), Some((120.0, 90.0)));
    }

    #[test]
    fn parses_wav_header() {
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&0u32.to_le_bytes());
        wav.extend_from_slice(b"WAVE");
        wav.extend_from_slice(b"fmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
        wav.extend_from_slice(&1u16.to_le_bytes()); // mono
        wav.extend_from_slice(&44100u32.to_le_bytes());
        wav.extend_from_slice(&88200u32.to_le_bytes());
        wav.extend_from_slice(&2u16.to_le_bytes()); // block align
        wav.extend_from_slice(&16u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&2064u32.to_le_bytes());
        assert_eq!(wav_info(&wav), Some((44100, 1032)));
    }

    #[test]
    fn unknown_extension_is_rejected() {
        assert!(normalize_format("exe").is_none());
        assert_eq!(normalize_format("jpeg").as_deref(), Some("jpg"));
    }

    #[test]
    fn image_size_dispatches_by_format() {
        let png = {
            let mut p = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
            p.extend_from_slice(&13u32.to_be_bytes());
            p.extend_from_slice(b"IHDR");
            p.extend_from_slice(&10u32.to_be_bytes());
            p.extend_from_slice(&20u32.to_be_bytes());
            p
        };
        assert_eq!(image_size(&asset("png", &png)), Some((10.0, 20.0)));
        assert_eq!(image_size(&asset("wav", &png)), None);
    }

    #[test]
    fn svg_percentage_and_font_units_are_unknown_lengths() {
        assert_eq!(parse_len("480"), Some(480.0));
        assert_eq!(parse_len("12px"), Some(12.0));
        assert_eq!(parse_len(" 12 px "), Some(12.0));
        assert_eq!(parse_len("100%"), None);
        assert_eq!(parse_len("12em"), None);
        assert_eq!(parse_len("8pt"), None);
        assert_eq!(parse_len("inf"), None);
        assert_eq!(parse_len("nan"), None);
        assert_eq!(parse_len(""), None);

        // A percentage falls through to the viewBox.
        let svg = br#"<svg width="100%" height="100%" viewBox="0 0 240 180"></svg>"#;
        assert_eq!(svg_size(svg), Some((240.0, 180.0)));
        // With no viewBox there is nothing left, so the size stays unknown.
        assert_eq!(svg_size(br#"<svg width="12em" height="12em"></svg>"#), None);
    }

    #[test]
    fn sniffs_known_asset_signatures() {
        assert_eq!(
            sniff_format(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]),
            Some("png")
        );
        assert_eq!(sniff_format(b"GIF87a"), Some("gif"));
        assert_eq!(sniff_format(b"GIF89a"), Some("gif"));
        assert_eq!(sniff_format(b"BM\x00\x00"), Some("bmp"));
        assert_eq!(sniff_format(&[0xFF, 0xD8, 0xFF, 0xE0]), Some("jpg"));
        assert_eq!(sniff_format(b"RIFF\x00\x00\x00\x00WAVEfmt "), Some("wav"));
        assert_eq!(sniff_format(b"ID3\x04\x00"), Some("mp3"));
        assert_eq!(sniff_format(&[0xFF, 0xFB, 0x90, 0x00]), Some("mp3"));
        // SVG has no magic number, and unknown bytes are left alone.
        assert_eq!(sniff_format(b"<svg width=\"1\"></svg>"), None);
        assert_eq!(sniff_format(b""), None);
    }

    fn temp_asset(name: &str, ext: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("raven-assets-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir.join(format!("asset.{ext}"))
    }

    #[test]
    fn load_rejects_content_that_does_not_match_the_extension() {
        let path = temp_asset("mismatch", "png");
        std::fs::write(&path, b"GIF89a\x01\x00\x01\x00").expect("write asset");
        let src = Source::new(path.clone(), String::new());

        let err = load(&path, &src, Pos::new(1, 1)).expect_err("a GIF named .png must not load");
        let rendered = err.render();
        assert!(rendered.contains("GIF image"), "{rendered}");
        assert!(rendered.contains("PNG image"), "{rendered}");
        assert!(rendered.contains("extension says"), "{rendered}");
        let _ = std::fs::remove_dir_all(path.parent().expect("parent"));
    }

    #[test]
    fn load_accepts_content_that_matches_or_is_unrecognised() {
        let path = temp_asset("match", "png");
        let mut png = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        png.extend_from_slice(&[0u8; 16]);
        std::fs::write(&path, &png).expect("write asset");
        let src = Source::new(path.clone(), String::new());
        let asset = load(&path, &src, Pos::new(1, 1)).expect("a PNG named .png must load");
        assert_eq!(asset.data_format, "png");
        let _ = std::fs::remove_dir_all(path.parent().expect("parent"));

        // An empty WAV has no signature, so nothing can contradict its name.
        let path = temp_asset("empty", "wav");
        std::fs::write(&path, b"").expect("write asset");
        let src = Source::new(path.clone(), String::new());
        let asset = load(&path, &src, Pos::new(1, 1)).expect("unrecognised bytes must load");
        assert_eq!(asset.data_format, "wav");
        let _ = std::fs::remove_dir_all(path.parent().expect("parent"));
    }

    #[test]
    fn jpeg_size_skips_fill_bytes_and_stops_at_sos() {
        // SOI, an APP0 segment whose marker is preceded by a fill 0xFF, then SOF0.
        let jpeg = [
            0xFF, 0xD8, // SOI
            0xFF, 0xFF, 0xE0, 0x00, 0x04, 0x00, 0x00, // fill + APP0 (len 4)
            0xFF, 0xC0, 0x00, 0x11, 0x08, 0x00, 0x20, 0x00, 0x30, // SOF0: 32x48
        ];
        assert_eq!(jpeg_size(&jpeg), Some((48.0, 32.0)));

        // Reaching start-of-scan without a frame header must not read the
        // entropy-coded data as if it were a segment.
        let sos = [
            0xFF, 0xD8, 0xFF, 0xDA, 0x00, 0x02, 0xFF, 0xC0, 0x00, 0x11, 0x08, 0x00, 0x20, 0x00,
            0x30,
        ];
        assert_eq!(jpeg_size(&sos), None);
    }
}
