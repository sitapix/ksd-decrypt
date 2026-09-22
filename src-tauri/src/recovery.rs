use aes::Aes256;
use ctr::cipher::{KeyIvInit, StreamCipher};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{self, BufRead, BufReader, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

const HEADER: usize = 3722;
const CHUNK: usize = 1024 * 1024;
type LegacyCipher = ctr::Ctr128BE<Aes256>;
const KEY: &[u8; 32] = b"keepsafekeepsafekeepsafekeepsafe";

#[derive(Clone, Serialize, Debug)]
pub struct InputFile {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    pub bytes: u64,
    pub format: String,
    pub kind: String,
    pub issue: Option<String>,
}

impl InputFile {
    pub fn payload_bytes(&self) -> u64 {
        self.bytes.saturating_sub(HEADER as u64)
    }
}

#[derive(Clone, Serialize, Debug)]
pub struct RecoveredFile {
    pub id: String,
    pub name: String,
    pub source: PathBuf,
    pub output: Option<PathBuf>,
    pub bytes: u64,
    pub kind: String,
    pub format: String,
    pub status: String,
    pub detail: String,
    pub source_sha256: Option<String>,
    pub recovered_sha256: Option<String>,
}

impl RecoveredFile {
    pub fn failed(input: &InputFile, error: io::Error) -> Self {
        Self {
            id: input.id.clone(),
            name: input.name.clone(),
            source: input.path.clone(),
            output: None,
            bytes: 0,
            kind: input.kind.clone(),
            format: input.format.clone(),
            status: "failed".into(),
            detail: error.to_string(),
            source_sha256: None,
            recovered_sha256: None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Format {
    pub extension: &'static str,
    pub mime: &'static str,
}

fn err(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}
pub fn check_cancel(cancel: &AtomicBool) -> io::Result<()> {
    if cancel.load(Ordering::Relaxed) {
        Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "Recovery stopped. Finished files have been kept.",
        ))
    } else {
        Ok(())
    }
}

fn read_header(reader: &mut impl Read) -> io::Result<[u8; HEADER]> {
    let mut header = [0u8; HEADER];
    reader
        .read_exact(&mut header)
        .map_err(|_| err("Incomplete or unsupported file."))?;
    if &header[..8] != b"\x89PNG\r\n\x1a\n" {
        return Err(err("Unsupported format."));
    }
    let mut offset = 8usize;
    let mut data_seen = false;
    while offset + 12 <= 3705 {
        let len = u32::from_be_bytes(header[offset..offset + 4].try_into().unwrap()) as usize;
        let end = offset
            .checked_add(12)
            .and_then(|v| v.checked_add(len))
            .ok_or_else(|| err("Damaged file header."))?;
        if end > 3705 {
            return Err(err("Unsupported or damaged header."));
        }
        let kind = &header[offset + 4..offset + 8];
        if offset == 8 && (kind != b"IHDR" || len != 13) {
            return Err(err("Damaged header."));
        }
        let crc = u32::from_be_bytes(header[end - 4..end].try_into().unwrap());
        if crc32fast::hash(&header[offset + 4..end - 4]) != crc {
            return Err(err("Damaged header (checksum mismatch)."));
        }
        if kind == b"IDAT" {
            data_seen = true;
        }
        if kind == b"IEND" {
            return if end == 3705 && len == 0 && data_seen {
                Ok(header)
            } else {
                Err(err("Unsupported vault version."))
            };
        }
        offset = end;
    }
    Err(err("Incomplete header."))
}

fn identify(bytes: &[u8]) -> io::Result<Format> {
    // Old phone videos commonly use 3GP brands that infer does not recognize.
    if bytes.len() >= 16 && &bytes[4..8] == b"ftyp" {
        let brand = &bytes[8..12];
        let custom = if &brand[..3] == b"3gp" {
            Some(("3gp", "video/3gpp"))
        } else if &brand[..3] == b"3g2" {
            Some(("3g2", "video/3gpp2"))
        } else if brand == b"crx " {
            Some(("cr3", "image/x-canon-cr3"))
        } else {
            None
        };
        if let Some((extension, mime)) = custom {
            return Ok(Format { extension, mime });
        }
    }
    if bytes.len() >= 32 {
        let raw = if bytes.starts_with(b"FUJIFILMCCD-RAW ") {
            Some(("raf", "image/x-fuji-raf"))
        } else if bytes.starts_with(b"IIRO")
            || bytes.starts_with(b"IIRS")
            || bytes.starts_with(b"MMOR")
        {
            Some(("orf", "image/x-olympus-orf"))
        } else if bytes.starts_with(b"IIU\0") {
            Some(("rw2", "image/x-panasonic-rw2"))
        } else {
            None
        };
        if let Some((extension, mime)) = raw {
            return Ok(Format { extension, mime });
        }
    }
    if bytes.len() >= 4 * 192 {
        if (0..4).all(|i| bytes[i * 188] == 0x47) {
            return Ok(Format {
                extension: "ts",
                mime: "video/mp2t",
            });
        }
        if (0..4).all(|i| bytes[4 + i * 192] == 0x47) {
            return Ok(Format {
                extension: "mts",
                mime: "video/mp2t",
            });
        }
    }
    let kind = infer::get(bytes).ok_or_else(|| err("Unsupported format or key."))?;
    if !kind.mime_type().starts_with("image/") && !kind.mime_type().starts_with("video/") {
        return Err(err("Unsupported media type."));
    }
    Ok(Format {
        extension: kind.extension(),
        mime: kind.mime_type(),
    })
}

pub fn probe(path: &Path) -> io::Result<Format> {
    let mut file = File::open(path)?;
    let header = read_header(&mut file)?;
    let mut sample = vec![0; 65536];
    let count = file.read(&mut sample)?;
    sample.truncate(count);
    let mut cipher = LegacyCipher::new(KEY.into(), (&header[3705..3721]).into());
    cipher.apply_keystream(&mut sample);
    identify(&sample)
}

pub fn inspect(path: PathBuf, id: String) -> InputFile {
    let name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let bytes = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    match probe(&path) {
        Ok(format) => InputFile {
            id,
            name,
            path,
            bytes,
            format: format.extension.to_uppercase(),
            kind: if format.mime.starts_with("video/") {
                "video"
            } else {
                "image"
            }
            .into(),
            issue: None,
        },
        Err(e) => InputFile {
            id,
            name,
            path,
            bytes,
            format: "Unknown".into(),
            kind: "unknown".into(),
            issue: Some(e.to_string()),
        },
    }
}

pub fn safe_filename(input: &Path, format: &Format) -> String {
    let mut name = input
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    if name.to_ascii_lowercase().ends_with(".ksd") {
        name.truncate(name.len() - 4);
    }
    if name.as_bytes().get(13) == Some(&b'.')
        && name.as_bytes()[..13].iter().all(u8::is_ascii_digit)
    {
        name = name[14..].to_owned();
    }
    let original_ext = Path::new(&name)
        .extension()
        .unwrap_or_default()
        .to_string_lossy()
        .to_ascii_lowercase();
    let extension = match (format.extension, original_ext.as_str()) {
        ("jpg", "jpeg" | "jpe" | "jfif")
        | ("tif", "tiff" | "dng" | "nef" | "arw" | "cr2" | "pef" | "srw" | "nrw")
        | ("mp4", "m4v" | "3gp" | "3g2")
        | ("mts", "m2ts") => original_ext.as_str(),
        _ => format.extension,
    };
    let stem = Path::new(&name)
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy();
    let cleaned: String = stem
        .chars()
        .map(|c| {
            if c.is_control() || "<>:\"/\\|?*".contains(c) {
                '_'
            } else {
                c
            }
        })
        .take(120)
        .collect();
    let cleaned = cleaned.trim_matches(|c| c == '.' || c == ' ');
    let mut cleaned = if cleaned.is_empty() {
        "Recovered file".to_owned()
    } else {
        cleaned.to_owned()
    };
    if [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ]
    .contains(&cleaned.to_ascii_uppercase().as_str())
    {
        cleaned.insert_str(0, "Recovered-");
    }
    format!("{cleaned}.{extension}")
}

fn read_byte(reader: &mut impl Read) -> io::Result<u8> {
    let mut b = [0];
    reader.read_exact(&mut b)?;
    Ok(b[0])
}
fn validate_jpeg(path: &Path, cancel: &AtomicBool) -> io::Result<()> {
    let mut reader = BufReader::with_capacity(65536, File::open(path)?);
    let size = reader.get_ref().metadata()?.len();
    if read_byte(&mut reader)? != 255 || read_byte(&mut reader)? != 216 {
        return Err(err("Invalid JPEG header."));
    }
    let mut in_scan = false;
    let mut saw_frame = false;
    let mut saw_scan = false;
    loop {
        check_cancel(cancel)?;
        if in_scan {
            loop {
                let buffer = reader.fill_buf()?;
                if buffer.is_empty() {
                    return Err(err("The JPEG is truncated. Its original has been kept."));
                }
                let found = memchr::memchr(255, buffer);
                let count = found.unwrap_or(buffer.len());
                reader.consume(count);
                if found.is_some() {
                    break;
                }
                check_cancel(cancel)?;
            }
        }
        if read_byte(&mut reader)? != 255 {
            return Err(err("Damaged JPEG structure."));
        }
        let mut marker = read_byte(&mut reader)?;
        while marker == 255 {
            marker = read_byte(&mut reader)?;
        }
        if in_scan && (marker == 0 || (0xd0..=0xd7).contains(&marker)) {
            continue;
        }
        in_scan = false;
        if marker == 0xd9 {
            return if saw_frame && saw_scan {
                Ok(())
            } else {
                Err(err("Incomplete JPEG image."))
            };
        }
        if marker == 1 {
            continue;
        }
        let len = u16::from_be_bytes([read_byte(&mut reader)?, read_byte(&mut reader)?]) as u64;
        if len < 2 || reader.stream_position()? + len - 2 > size {
            return Err(err("The JPEG has an incomplete segment."));
        }
        if [
            0xc0, 0xc1, 0xc2, 0xc3, 0xc5, 0xc6, 0xc7, 0xc9, 0xca, 0xcb, 0xcd, 0xce, 0xcf,
        ]
        .contains(&marker)
        {
            saw_frame = true;
        }
        if marker == 0xda {
            saw_scan = true;
            in_scan = true;
        }
        reader.seek_relative((len - 2) as i64)?;
    }
}

fn validate_png(path: &Path, cancel: &AtomicBool) -> io::Result<()> {
    let mut reader = BufReader::new(File::open(path)?);
    reader.seek(SeekFrom::Start(8))?;
    let mut buffer = vec![0; 65536];
    let mut data_seen = false;
    loop {
        check_cancel(cancel)?;
        let mut chunk = [0; 8];
        reader.read_exact(&mut chunk)?;
        let mut remaining = u32::from_be_bytes(chunk[..4].try_into().unwrap()) as u64;
        let kind = &chunk[4..];
        let mut crc = crc32fast::Hasher::new();
        crc.update(kind);
        if kind == b"IDAT" {
            data_seen = true;
        }
        if kind == b"IEND" && remaining != 0 {
            return Err(err("Invalid PNG end marker."));
        }
        while remaining > 0 {
            check_cancel(cancel)?;
            let count = remaining.min(buffer.len() as u64) as usize;
            reader.read_exact(&mut buffer[..count])?;
            crc.update(&buffer[..count]);
            remaining -= count as u64;
        }
        let mut expected = [0; 4];
        reader.read_exact(&mut expected)?;
        if crc.finalize() != u32::from_be_bytes(expected) {
            return Err(err("The recovered PNG is damaged (checksum mismatch)."));
        }
        if kind == b"IEND" {
            return if data_seen {
                Ok(())
            } else {
                Err(err("The PNG is missing image data."))
            };
        }
    }
}

fn validate_boxes(path: &Path, cancel: &AtomicBool) -> io::Result<()> {
    let mut file = File::open(path)?;
    let total = file.metadata()?.len();
    let mut position = 0;
    while position < total {
        check_cancel(cancel)?;
        file.seek(SeekFrom::Start(position))?;
        let mut header = [0; 8];
        file.read_exact(&mut header)?;
        let mut size = u32::from_be_bytes(header[..4].try_into().unwrap()) as u64;
        let mut minimum = 8;
        if size == 1 {
            file.read_exact(&mut header)?;
            size = u64::from_be_bytes(header);
            minimum = 16;
        }
        if size == 0 {
            size = total - position;
        }
        if size < minimum || size > total - position {
            return Err(err(
                "The media container is incomplete. Keep the original for further recovery.",
            ));
        }
        position += size;
    }
    Ok(())
}

pub fn recover_one(
    input: &InputFile,
    root: &Path,
    cancel: &AtomicBool,
    progress: impl FnMut(u64, &str),
) -> io::Result<RecoveredFile> {
    recover_to(input, root, true, cancel, progress)
}

pub fn recover_beside_source(
    input: &InputFile,
    cancel: &AtomicBool,
    progress: impl FnMut(u64, &str),
) -> io::Result<RecoveredFile> {
    let folder = input
        .path
        .parent()
        .ok_or_else(|| err("Cannot find the source folder."))?;
    recover_to(input, folder, false, cancel, progress)
}

fn recover_to(
    input: &InputFile,
    root: &Path,
    group_by_type: bool,
    cancel: &AtomicBool,
    mut progress: impl FnMut(u64, &str),
) -> io::Result<RecoveredFile> {
    check_cancel(cancel)?;
    let mut source = File::open(&input.path)?;
    let before = source.metadata()?;
    let header = read_header(&mut source)?;
    let mut cipher = LegacyCipher::new(KEY.into(), (&header[3705..3721]).into());
    let mut original_hash = Sha256::new();
    original_hash.update(header);
    let mut recovered_hash = Sha256::new();
    // Small photos need only their payload size; large files stay bounded at
    // one MiB. Read and decrypt the first chunk once, including identification.
    let buffer_len = usize::try_from(before.len().saturating_sub(HEADER as u64))
        .unwrap_or(CHUNK)
        .clamp(1, CHUNK);
    let mut buffer = vec![0; buffer_len];
    let mut count = decrypt_chunk(&mut source, &mut buffer, &mut cipher, &mut original_hash)?;
    let format = identify(&buffer[..count.min(65536)])?;
    check_cancel(cancel)?;
    let group = if input.path.components().any(|p| p.as_os_str() == ".thumbs") {
        "Thumbnails"
    } else if input.path.components().any(|p| p.as_os_str() == ".breakin") {
        "Additional images"
    } else if format.mime.starts_with("video/") {
        "Videos"
    } else {
        "Photos"
    };
    let folder = if group_by_type {
        root.join(group)
    } else {
        root.to_path_buf()
    };
    fs::create_dir_all(&folder)?;
    let mut temporary = tempfile::NamedTempFile::new_in(&folder).map_err(|e| {
        io::Error::new(
            e.kind(),
            format!("Cannot save here: {e}. Choose another output folder."),
        )
    })?;
    let mut written = 0u64;
    while count != 0 {
        check_cancel(cancel)?;
        recovered_hash.update(&buffer[..count]);
        temporary.write_all(&buffer[..count])?;
        written += count as u64;
        progress(written, "recovering");
        check_cancel(cancel)?;
        count = decrypt_chunk(&mut source, &mut buffer, &mut cipher, &mut original_hash)?;
    }
    if written + HEADER as u64 != before.len()
        || source.metadata()?.modified()? != before.modified()?
    {
        return Err(err(
            "The source changed during recovery. Please try again once it is no longer being modified.",
        ));
    }
    temporary.flush()?;
    progress(written, "checking");
    let detail = match format.extension {
        "jpg" => {
            validate_jpeg(temporary.path(), cancel)?;
            "JPEG structure checked; original bytes preserved."
        }
        "png" => {
            validate_png(temporary.path(), cancel)?;
            "PNG checksums verified; original bytes preserved."
        }
        "mp4" | "mov" | "m4v" | "3gp" | "3g2" | "heic" | "heif" | "avif" | "cr3" => {
            validate_boxes(temporary.path(), cancel)?;
            "Media container checked; original bytes preserved."
        }
        _ => {
            "Media signature recognized; original bytes preserved. Full media integrity has not been verified."
        }
    };
    check_cancel(cancel)?;
    temporary.as_file().sync_all()?;
    let name = safe_filename(&input.path, &format);
    let output = persist_unique(temporary, &folder, &name)?;
    Ok(RecoveredFile {
        id: input.id.clone(),
        name: output.file_name().unwrap().to_string_lossy().into_owned(),
        source: input.path.clone(),
        output: Some(output),
        bytes: written,
        kind: input.kind.clone(),
        format: format.extension.to_uppercase(),
        status: "recovered".into(),
        detail: detail.into(),
        source_sha256: Some(format!("{:x}", original_hash.finalize())),
        recovered_sha256: Some(format!("{:x}", recovered_hash.finalize())),
    })
}

fn decrypt_chunk(
    source: &mut File,
    buffer: &mut [u8],
    cipher: &mut LegacyCipher,
    original_hash: &mut Sha256,
) -> io::Result<usize> {
    let count = source.read(buffer)?;
    original_hash.update(&buffer[..count]);
    cipher.apply_keystream(&mut buffer[..count]);
    Ok(count)
}

fn persist_unique(
    mut temporary: tempfile::NamedTempFile,
    folder: &Path,
    name: &str,
) -> io::Result<PathBuf> {
    let stem = Path::new(name)
        .file_stem()
        .unwrap()
        .to_string_lossy()
        .to_string();
    let extension = Path::new(name)
        .extension()
        .unwrap()
        .to_string_lossy()
        .to_string();
    let mut output = folder.join(name);
    let mut suffix = 2;
    loop {
        match temporary.persist_noclobber(&output) {
            Ok(_) => break,
            Err(e) if e.error.kind() == io::ErrorKind::AlreadyExists => {
                temporary = e.file;
                output = folder.join(format!("{stem} ({suffix}).{extension}"));
                suffix += 1;
            }
            Err(e) => return Err(e.error),
        }
    }
    Ok(output)
}

pub fn write_report(folder: &Path, report: &serde_json::Value) -> io::Result<PathBuf> {
    let mut temporary = tempfile::NamedTempFile::new_in(folder)?;
    serde_json::to_writer_pretty(&mut temporary, report)?;
    temporary.flush()?;
    temporary.as_file().sync_all()?;
    persist_unique(temporary, folder, "KSD Decrypt report.json")
}
