use std::collections::{HashMap, HashSet};
use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Component, Path, PathBuf};
use std::ptr;

use crate::{CoreError, Result};

pub use super::JobContext;
use super::unicode_casefold;

const ARCHIVE_EOF: c_int = 1;
const ARCHIVE_OK: c_int = 0;
const ARCHIVE_WARN: c_int = -20;
const FORMAT_BASE_MASK: c_int = 0x00ff_0000;
const FORMAT_ZIP: c_int = 0x0005_0000;
const FORMAT_RAR: c_int = 0x000d_0000;
const FORMAT_7ZIP: c_int = 0x000e_0000;
const FORMAT_RAR5: c_int = 0x0010_0000;
const ENCRYPTION_UNSUPPORTED: c_int = -2;
const ENCRYPTION_DONT_KNOW: c_int = -1;
#[cfg(unix)]
type ArchiveMode = libc::mode_t;
#[cfg(windows)]
type ArchiveMode = u16;
const AE_IFREG: ArchiveMode = 0o100000;
const AE_IFDIR: ArchiveMode = 0o040000;
const COPY_BUFFER_SIZE: usize = 64 * 1024;
const MAX_PATH_BYTES: usize = 4096;
const MAX_PATH_DEPTH: usize = 64;
const MAX_PREFLIGHT_HEADERS: u64 = 200_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExtractionLimits {
    pub max_entries: u64,
    pub max_total_bytes: u64,
    pub max_file_bytes: u64,
}

impl Default for ExtractionLimits {
    fn default() -> Self {
        Self {
            max_entries: 100_000,
            max_total_bytes: 20 * 1024 * 1024 * 1024,
            max_file_bytes: 128 * 1024 * 1024,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InputFormat {
    Zip,
    Rar4,
    Rar5,
    SevenZip,
}

pub fn validate_entry_path(name: &str) -> Result<PathBuf> {
    if name.is_empty() || name.contains('\0') || name.len() > MAX_PATH_BYTES {
        return Err(CoreError::UnsafeArchivePath(name.to_owned()));
    }

    let portable = name.replace('\\', "/");
    if portable.starts_with('/') || portable.contains(':') {
        return Err(CoreError::UnsafeArchivePath(name.to_owned()));
    }

    let mut result = PathBuf::new();
    let mut depth = 0_usize;
    for component in portable.split('/') {
        if component.is_empty() || component == "." {
            continue;
        }
        if component == ".."
            || component.len() > 255
            || component.ends_with(['.', ' '])
            || component.chars().any(|character| {
                character.is_control() || matches!(character, '<' | '>' | '"' | '|' | '?' | '*')
            })
            || is_windows_reserved(component)
        {
            return Err(CoreError::UnsafeArchivePath(name.to_owned()));
        }
        depth += 1;
        if depth > MAX_PATH_DEPTH {
            return Err(CoreError::UnsafeArchivePath(name.to_owned()));
        }
        result.push(component);
    }

    if result.as_os_str().is_empty()
        || result
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(CoreError::UnsafeArchivePath(name.to_owned()));
    }
    Ok(result)
}

fn is_windows_reserved(component: &str) -> bool {
    let stem = component.split('.').next().unwrap_or(component);
    let upper = stem.to_ascii_uppercase();
    matches!(
        upper.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) || upper
        .strip_prefix("COM")
        .or_else(|| upper.strip_prefix("LPT"))
        .is_some_and(|number| matches!(number, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9"))
}

pub fn extract_archive(
    input: &Path,
    staging: &Path,
    limits: &ExtractionLimits,
    job: &JobContext,
) -> Result<()> {
    check_cancelled(job)?;
    let expected_format = preflight(input, job)?;
    fs::create_dir(staging).map_err(|error| {
        if error.kind() == std::io::ErrorKind::AlreadyExists {
            CoreError::ArchiveStagingNotFresh
        } else {
            CoreError::Io(error)
        }
    })?;

    let result = extract_into_fresh_staging(input, staging, limits, job, expected_format);
    if result.is_err() {
        let _ = fs::remove_dir_all(staging);
    }
    result
}

fn extract_into_fresh_staging(
    input: &Path,
    staging: &Path,
    limits: &ExtractionLimits,
    job: &JobContext,
    expected_format: InputFormat,
) -> Result<()> {
    let mut reader = ArchiveReader::open(input)?;
    let mut seen_entries = HashSet::new();
    let mut seen_portable_components = HashMap::new();
    let mut entries = 0_u64;
    let mut total_declared = 0_u64;
    let mut total_actual = 0_u64;
    let mut buffer = vec![0_u8; COPY_BUFFER_SIZE];

    loop {
        check_cancelled(job)?;
        let entry = match reader.next_header()? {
            Some(entry) => entry,
            None => break,
        };
        reader.require_format(expected_format)?;
        entries = entries.checked_add(1).ok_or(CoreError::ArchiveEntryLimit)?;
        if entries > limits.max_entries {
            return Err(CoreError::ArchiveEntryLimit);
        }

        let raw_name = entry.pathname()?;
        let relative = validate_entry_path(&raw_name)?;
        let portable_path = relative.to_string_lossy().replace('\\', "/");
        let collision_key = unicode_casefold::fold(&portable_path);
        if !seen_entries.insert(collision_key.clone()) {
            return Err(CoreError::ArchivePathCollision(raw_name));
        }
        let mut original_prefix = String::new();
        for component in portable_path.split('/') {
            if !original_prefix.is_empty() {
                original_prefix.push('/');
            }
            original_prefix.push_str(component);
            let folded_prefix = unicode_casefold::fold(&original_prefix);
            match seen_portable_components.get(&folded_prefix) {
                Some(existing) if existing != &original_prefix => {
                    return Err(CoreError::ArchivePathCollision(raw_name));
                }
                Some(_) => {}
                None => {
                    seen_portable_components.insert(folded_prefix, original_prefix.clone());
                }
            }
        }
        entry.require_unencrypted()?;

        let file_type = entry.file_type();
        if entry.has_link() || !matches!(file_type, AE_IFREG | AE_IFDIR) {
            return Err(CoreError::UnsafeArchiveEntryType(raw_name));
        }

        let output = staging.join(&relative);
        if file_type == AE_IFDIR {
            fs::create_dir_all(&output)?;
            continue;
        }

        if let Some(size) = entry.declared_size() {
            if size > limits.max_file_bytes {
                return Err(CoreError::ArchiveFileSizeLimit);
            }
            total_declared = total_declared
                .checked_add(size)
                .ok_or(CoreError::ArchiveTotalSizeLimit)?;
            if total_declared > limits.max_total_bytes {
                return Err(CoreError::ArchiveTotalSizeLimit);
            }
        }

        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut destination = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&output)
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::AlreadyExists {
                    CoreError::ArchivePathCollision(raw_name.clone())
                } else {
                    CoreError::Io(error)
                }
            })?;
        let mut file_actual = 0_u64;
        loop {
            check_cancelled(job)?;
            let read = reader.read_data(&mut buffer)?;
            if read == 0 {
                break;
            }
            let read = u64::try_from(read).map_err(|_| {
                CoreError::InvalidArchive("libarchive returned an invalid byte count".into())
            })?;
            file_actual = file_actual
                .checked_add(read)
                .ok_or(CoreError::ArchiveFileSizeLimit)?;
            if file_actual > limits.max_file_bytes {
                return Err(CoreError::ArchiveFileSizeLimit);
            }
            total_actual = total_actual
                .checked_add(read)
                .ok_or(CoreError::ArchiveTotalSizeLimit)?;
            if total_actual > limits.max_total_bytes {
                return Err(CoreError::ArchiveTotalSizeLimit);
            }
            destination.write_all(&buffer[..read as usize])?;
            job.report_progress(total_actual, None);
        }
    }

    match reader.encryption_state() {
        0 => Ok(()),
        1 | ENCRYPTION_DONT_KNOW | ENCRYPTION_UNSUPPORTED => Err(CoreError::EncryptedArchive),
        state => Err(CoreError::InvalidArchive(format!(
            "libarchive returned encryption state {state}"
        ))),
    }
}

fn check_cancelled(job: &JobContext) -> Result<()> {
    if job.is_cancelled() {
        Err(CoreError::Cancelled)
    } else {
        Ok(())
    }
}

fn preflight(input: &Path, job: &JobContext) -> Result<InputFormat> {
    if has_multipart_name(input) {
        return Err(CoreError::MultipartArchive);
    }

    let mut file = File::open(input)?;
    let mut signature = [0_u8; 8];
    let count = file.read(&mut signature)?;
    let bytes = &signature[..count];
    if bytes.starts_with(b"Rar!\x1a\x07\x00") {
        reject_rar4_multipart(&mut file, job)?;
        return Ok(InputFormat::Rar4);
    }
    if bytes.starts_with(b"Rar!\x1a\x07\x01\x00") {
        reject_rar5_multipart(&mut file, job)?;
        return Ok(InputFormat::Rar5);
    }
    if bytes.starts_with(b"7z\xbc\xaf'\x1c") {
        return Ok(InputFormat::SevenZip);
    }
    if bytes.starts_with(b"PK\x03\x04")
        || bytes.starts_with(b"PK\x05\x06")
        || bytes.starts_with(b"PK\x07\x08")
    {
        reject_zip_multipart_or_truncated(&mut file)?;
        return Ok(InputFormat::Zip);
    }
    Err(CoreError::UnsupportedArchiveFormat)
}

fn has_multipart_name(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
        return false;
    };
    let name = name.to_ascii_lowercase();
    let digit_suffix = |prefix: &str| {
        name.rsplit_once(prefix).is_some_and(|(_, suffix)| {
            !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit())
        })
    };
    (name.ends_with(".rar")
        && name.rfind(".part").is_some_and(|index| {
            name[index + 5..name.len() - 4]
                .bytes()
                .all(|byte| byte.is_ascii_digit())
        }))
        || name.rsplit_once('.').is_some_and(|(_, extension)| {
            extension.len() == 3
                && matches!(extension.as_bytes()[0], b'r' | b'z')
                && extension[1..].bytes().all(|byte| byte.is_ascii_digit())
        })
        || digit_suffix(".zip.")
        || digit_suffix(".7z.")
}

fn reject_zip_multipart_or_truncated(file: &mut File) -> Result<()> {
    let length = file.metadata()?.len();
    let tail_size = length.min(65_557) as usize;
    file.seek(SeekFrom::End(-(tail_size as i64)))?;
    let mut tail = vec![0_u8; tail_size];
    file.read_exact(&mut tail)?;
    let eocd = tail
        .windows(4)
        .enumerate()
        .filter(|(_, window)| *window == b"PK\x05\x06")
        .filter_map(|(offset, _)| {
            let fixed_end = offset.checked_add(22)?;
            if fixed_end > tail.len() {
                return None;
            }
            let comment_length =
                u16::from_le_bytes([tail[offset + 20], tail[offset + 21]]) as usize;
            (fixed_end.checked_add(comment_length) == Some(tail.len())).then_some(offset)
        })
        .next_back()
        .ok_or_else(|| CoreError::InvalidArchive("ZIP end record is missing".into()))?;
    let u16_at = |offset| u16::from_le_bytes([tail[eocd + offset], tail[eocd + offset + 1]]);
    let u32_at =
        |offset| u32::from_le_bytes(tail[eocd + offset..eocd + offset + 4].try_into().unwrap());
    let disk = u16_at(4);
    let central_disk = u16_at(6);
    let entries_on_disk = u16_at(8);
    let entries = u16_at(10);
    let central_size = u32_at(12);
    let central_offset = u32_at(16);
    let needs_zip64 = disk == u16::MAX
        || central_disk == u16::MAX
        || entries_on_disk == u16::MAX
        || entries == u16::MAX
        || central_size == u32::MAX
        || central_offset == u32::MAX;
    if needs_zip64 {
        validate_zip64(file, length, &tail, eocd)?;
    } else if disk != 0 || central_disk != 0 || entries_on_disk != entries {
        return Err(CoreError::MultipartArchive);
    }
    Ok(())
}

fn validate_zip64(file: &mut File, length: u64, tail: &[u8], eocd: usize) -> Result<()> {
    if eocd < 20 || &tail[eocd - 20..eocd - 16] != b"PK\x06\x07" {
        return Err(CoreError::InvalidArchive("ZIP64 locator is missing".into()));
    }
    let locator = &tail[eocd - 20..eocd];
    let locator_disk = u32::from_le_bytes(locator[4..8].try_into().unwrap());
    let record_offset = u64::from_le_bytes(locator[8..16].try_into().unwrap());
    let total_disks = u32::from_le_bytes(locator[16..20].try_into().unwrap());
    if locator_disk != 0 || total_disks != 1 {
        return Err(CoreError::MultipartArchive);
    }
    let minimum_end = record_offset
        .checked_add(56)
        .ok_or_else(|| CoreError::InvalidArchive("ZIP64 record offset overflow".into()))?;
    if minimum_end > length {
        return Err(CoreError::InvalidArchive(
            "ZIP64 record is truncated".into(),
        ));
    }
    file.seek(SeekFrom::Start(record_offset))?;
    let mut record = [0_u8; 56];
    read_exact_archive(file, &mut record, "ZIP64 record is truncated")?;
    if &record[..4] != b"PK\x06\x06" {
        return Err(CoreError::InvalidArchive("ZIP64 record is missing".into()));
    }
    let record_size = u64::from_le_bytes(record[4..12].try_into().unwrap());
    let record_end = record_offset
        .checked_add(12)
        .and_then(|offset| offset.checked_add(record_size))
        .ok_or_else(|| CoreError::InvalidArchive("ZIP64 record size overflow".into()))?;
    let tail_start = length - tail.len() as u64;
    let locator_offset = tail_start + eocd as u64 - 20;
    if record_size < 44 || record_end > locator_offset {
        return Err(CoreError::InvalidArchive(
            "ZIP64 record has an invalid size".into(),
        ));
    }
    let disk = u32::from_le_bytes(record[16..20].try_into().unwrap());
    let central_disk = u32::from_le_bytes(record[20..24].try_into().unwrap());
    let entries_on_disk = u64::from_le_bytes(record[24..32].try_into().unwrap());
    let entries = u64::from_le_bytes(record[32..40].try_into().unwrap());
    if disk != 0 || central_disk != 0 || entries_on_disk != entries {
        return Err(CoreError::MultipartArchive);
    }
    Ok(())
}

fn reject_rar4_multipart(file: &mut File, job: &JobContext) -> Result<()> {
    let length = file.metadata()?.len();
    file.seek(SeekFrom::Start(7))?;
    let mut header = [0_u8; 11];
    let mut headers = 0_u64;
    loop {
        check_cancelled(job)?;
        let header_start = file.stream_position()?;
        if header_start == length {
            return Ok(());
        }
        if header_start > length || length - header_start < 7 {
            return Err(CoreError::InvalidArchive("RAR header is truncated".into()));
        }
        headers = headers
            .checked_add(1)
            .ok_or_else(|| CoreError::InvalidArchive("too many RAR headers".into()))?;
        if headers > MAX_PREFLIGHT_HEADERS {
            return Err(CoreError::InvalidArchive("too many RAR headers".into()));
        }
        read_exact_archive(file, &mut header[..7], "RAR header is truncated")?;
        let kind = header[2];
        let flags = u16::from_le_bytes([header[3], header[4]]);
        let header_size = u16::from_le_bytes([header[5], header[6]]) as u64;
        let prefix_size = if flags & 0x8000 != 0 { 11 } else { 7 };
        if header_size < prefix_size {
            return Err(CoreError::InvalidArchive(
                "RAR header has an invalid size".into(),
            ));
        }
        if (kind == 0x73 && flags & 0x0001 != 0) || (kind == 0x74 && flags & 0x0003 != 0) {
            return Err(CoreError::MultipartArchive);
        }
        let data_size = if flags & 0x8000 != 0 {
            read_exact_archive(file, &mut header[7..11], "RAR header is truncated")?;
            u32::from_le_bytes(header[7..11].try_into().unwrap()) as u64
        } else {
            0
        };
        let header_end = header_start
            .checked_add(header_size)
            .ok_or_else(|| CoreError::InvalidArchive("RAR header offset overflow".into()))?;
        let next_header = header_end
            .checked_add(data_size)
            .ok_or_else(|| CoreError::InvalidArchive("RAR data offset overflow".into()))?;
        if header_end > length || next_header > length || next_header <= header_start {
            return Err(CoreError::InvalidArchive(
                "RAR block extends past input".into(),
            ));
        }
        file.seek(SeekFrom::Start(next_header))?;
    }
}

fn reject_rar5_multipart(file: &mut File, job: &JobContext) -> Result<()> {
    let length = file.metadata()?.len();
    file.seek(SeekFrom::Start(8))?;
    let mut headers = 0_u64;
    loop {
        check_cancelled(job)?;
        let header_start = file.stream_position()?;
        if header_start == length {
            return Ok(());
        }
        if header_start > length || length - header_start < 4 {
            return Err(CoreError::InvalidArchive("RAR5 header is truncated".into()));
        }
        headers = headers
            .checked_add(1)
            .ok_or_else(|| CoreError::InvalidArchive("too many RAR5 headers".into()))?;
        if headers > MAX_PREFLIGHT_HEADERS {
            return Err(CoreError::InvalidArchive("too many RAR5 headers".into()));
        }
        let mut crc = [0_u8; 4];
        read_exact_archive(file, &mut crc, "RAR5 header is truncated")?;
        let header_size = read_vint(file)?;
        let start = file.stream_position()?;
        let end = start
            .checked_add(header_size)
            .ok_or_else(|| CoreError::InvalidArchive("RAR5 header size overflow".into()))?;
        if header_size == 0 || end > length {
            return Err(CoreError::InvalidArchive(
                "RAR5 header has an invalid size".into(),
            ));
        }
        let kind = read_vint_before(file, end)?;
        let flags = read_vint_before(file, end)?;
        if flags & (0x0008 | 0x0010) != 0 {
            return Err(CoreError::MultipartArchive);
        }
        if flags & 0x0001 != 0 {
            let _ = read_vint_before(file, end)?;
        }
        let data_size = if flags & 0x0002 != 0 {
            read_vint_before(file, end)?
        } else {
            0
        };
        if kind == 1 && read_vint_before(file, end)? & 0x0001 != 0 {
            return Err(CoreError::MultipartArchive);
        }
        let next_header = end
            .checked_add(data_size)
            .ok_or_else(|| CoreError::InvalidArchive("RAR5 data offset overflow".into()))?;
        if next_header > length || next_header <= header_start {
            return Err(CoreError::InvalidArchive(
                "RAR5 block extends past input".into(),
            ));
        }
        file.seek(SeekFrom::Start(next_header))?;
    }
}

fn read_vint_before(file: &mut File, end: u64) -> Result<u64> {
    let start = file.stream_position()?;
    if start >= end {
        return Err(CoreError::InvalidArchive("RAR5 header is truncated".into()));
    }
    let value = read_vint(file)?;
    if file.stream_position()? > end {
        return Err(CoreError::InvalidArchive("RAR5 header is truncated".into()));
    }
    Ok(value)
}

fn read_exact_archive(file: &mut File, buffer: &mut [u8], message: &str) -> Result<()> {
    file.read_exact(buffer)
        .map_err(|_| CoreError::InvalidArchive(message.into()))
}

fn read_vint(file: &mut File) -> Result<u64> {
    let mut value = 0_u64;
    for shift in (0..=63).step_by(7) {
        let mut byte = [0_u8; 1];
        file.read_exact(&mut byte)
            .map_err(|_| CoreError::InvalidArchive("RAR5 variable integer is truncated".into()))?;
        value |= u64::from(byte[0] & 0x7f) << shift;
        if byte[0] & 0x80 == 0 {
            return Ok(value);
        }
    }
    Err(CoreError::InvalidArchive(
        "RAR5 variable integer is too large".into(),
    ))
}

struct ArchiveReader {
    raw: *mut ffi::Archive,
    #[cfg(unix)]
    _locale: ThreadLocale,
}

impl ArchiveReader {
    fn open(path: &Path) -> Result<Self> {
        #[cfg(unix)]
        let locale = ThreadLocale::utf8()?;
        let raw = unsafe { ffi::archive_read_new() };
        if raw.is_null() {
            return Err(CoreError::InvalidArchive(
                "libarchive allocation failed".into(),
            ));
        }
        let reader = Self {
            raw,
            #[cfg(unix)]
            _locale: locale,
        };
        for result in unsafe {
            [
                ffi::archive_read_support_filter_all(raw),
                ffi::archive_read_support_format_zip(raw),
                ffi::archive_read_support_format_7zip(raw),
                ffi::archive_read_support_format_rar(raw),
                ffi::archive_read_support_format_rar5(raw),
            ]
        } {
            reader.require_ok(result)?;
        }
        let module = c"zip";
        let option = c"hdrcharset";
        let value = c"UTF-8";
        reader.require_ok(unsafe {
            ffi::archive_read_set_option(raw, module.as_ptr(), option.as_ptr(), value.as_ptr())
        })?;
        reader.open_filename(path)?;
        Ok(reader)
    }

    #[cfg(unix)]
    fn open_filename(&self, path: &Path) -> Result<()> {
        use std::os::unix::ffi::OsStrExt;
        let path = CString::new(path.as_os_str().as_bytes())
            .map_err(|_| CoreError::UnsafeArchivePath(path.display().to_string()))?;
        self.require_ok(unsafe { ffi::archive_read_open_filename(self.raw, path.as_ptr(), 10_240) })
    }

    #[cfg(windows)]
    fn open_filename(&self, path: &Path) -> Result<()> {
        use std::os::windows::ffi::OsStrExt;
        let path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        self.require_ok(unsafe {
            ffi::archive_read_open_filename_w(self.raw, path.as_ptr(), 10_240)
        })
    }

    fn next_header(&mut self) -> Result<Option<ArchiveEntry>> {
        let mut entry = ptr::null_mut();
        let status = unsafe { ffi::archive_read_next_header(self.raw, &mut entry) };
        match status {
            ARCHIVE_EOF => Ok(None),
            ARCHIVE_OK => Ok(Some(ArchiveEntry { raw: entry })),
            ARCHIVE_WARN
                if self
                    .error_message()
                    .contains("cannot be converted from UTF-8") =>
            {
                Ok(Some(ArchiveEntry { raw: entry }))
            }
            _ => Err(self.error()),
        }
    }

    fn read_data(&mut self, buffer: &mut [u8]) -> Result<isize> {
        let read =
            unsafe { ffi::archive_read_data(self.raw, buffer.as_mut_ptr().cast(), buffer.len()) };
        if read < 0 {
            Err(self.error())
        } else {
            Ok(read)
        }
    }

    fn require_format(&self, expected: InputFormat) -> Result<()> {
        let actual = unsafe { ffi::archive_format(self.raw) } & FORMAT_BASE_MASK;
        let expected = match expected {
            InputFormat::Zip => FORMAT_ZIP,
            InputFormat::Rar4 => FORMAT_RAR,
            InputFormat::Rar5 => FORMAT_RAR5,
            InputFormat::SevenZip => FORMAT_7ZIP,
        };
        if actual == expected {
            Ok(())
        } else {
            Err(CoreError::UnsupportedArchiveFormat)
        }
    }

    fn encryption_state(&self) -> c_int {
        unsafe { ffi::archive_read_has_encrypted_entries(self.raw) }
    }

    fn require_ok(&self, status: c_int) -> Result<()> {
        if status == ARCHIVE_OK {
            Ok(())
        } else {
            Err(self.error())
        }
    }

    fn error(&self) -> CoreError {
        let message = self.error_message();
        let lower = message.to_ascii_lowercase();
        if lower.contains("passphrase") || lower.contains("encrypt") {
            CoreError::EncryptedArchive
        } else {
            CoreError::InvalidArchive(message)
        }
    }

    fn error_message(&self) -> String {
        let pointer = unsafe { ffi::archive_error_string(self.raw) };
        if pointer.is_null() {
            "unknown libarchive error".to_owned()
        } else {
            unsafe { CStr::from_ptr(pointer) }
                .to_string_lossy()
                .into_owned()
        }
    }
}

#[cfg(unix)]
struct ThreadLocale {
    locale: libc::locale_t,
    previous: libc::locale_t,
}

#[cfg(unix)]
impl ThreadLocale {
    fn utf8() -> Result<Self> {
        let mut locale = ptr::null_mut();
        for name in [c"C.UTF-8", c"C.UTF8", c"en_US.UTF-8", c"UTF-8"] {
            locale =
                unsafe { libc::newlocale(libc::LC_CTYPE_MASK, name.as_ptr(), ptr::null_mut()) };
            if !locale.is_null() {
                break;
            }
        }
        if locale.is_null() {
            return Err(CoreError::Io(std::io::Error::last_os_error()));
        }
        let previous = unsafe { libc::uselocale(locale) };
        if previous.is_null() {
            unsafe { libc::freelocale(locale) };
            return Err(CoreError::Io(std::io::Error::last_os_error()));
        }
        Ok(Self { locale, previous })
    }
}

#[cfg(unix)]
impl Drop for ThreadLocale {
    fn drop(&mut self) {
        unsafe {
            libc::uselocale(self.previous);
            libc::freelocale(self.locale);
        }
    }
}

impl Drop for ArchiveReader {
    fn drop(&mut self) {
        unsafe {
            ffi::archive_read_free(self.raw);
        }
    }
}

struct ArchiveEntry {
    raw: *mut ffi::ArchiveEntry,
}

impl ArchiveEntry {
    fn pathname(&self) -> Result<String> {
        let utf8_pointer = unsafe { ffi::archive_entry_pathname_utf8(self.raw) };
        #[cfg(unix)]
        if utf8_pointer.is_null() {
            let wide = unsafe { ffi::archive_entry_pathname_w(self.raw) };
            if !wide.is_null() {
                let mut result = String::new();
                for index in 0..MAX_PATH_BYTES {
                    let value = unsafe { *wide.add(index) };
                    if value == 0 {
                        return Ok(result);
                    }
                    let character = char::from_u32(value as u32).ok_or_else(|| {
                        CoreError::UnsafeArchivePath("<invalid Unicode pathname>".into())
                    })?;
                    result.push(character);
                }
                return Err(CoreError::UnsafeArchivePath(
                    "<overlong Unicode pathname>".into(),
                ));
            }
        }
        let pointer = if utf8_pointer.is_null() {
            unsafe { ffi::archive_entry_pathname(self.raw) }
        } else {
            utf8_pointer
        };
        if pointer.is_null() {
            return Err(CoreError::UnsafeArchivePath("<missing pathname>".into()));
        }
        unsafe { CStr::from_ptr(pointer) }
            .to_str()
            .map(str::to_owned)
            .map_err(|_| CoreError::UnsafeArchivePath("<non-UTF-8 pathname>".into()))
    }

    fn file_type(&self) -> ArchiveMode {
        unsafe { ffi::archive_entry_filetype(self.raw) }
    }

    fn has_link(&self) -> bool {
        unsafe {
            !ffi::archive_entry_hardlink(self.raw).is_null()
                || !ffi::archive_entry_symlink(self.raw).is_null()
        }
    }

    fn declared_size(&self) -> Option<u64> {
        let size = unsafe { ffi::archive_entry_size(self.raw) };
        u64::try_from(size).ok()
    }

    fn require_unencrypted(&self) -> Result<()> {
        let encrypted = unsafe {
            ffi::archive_entry_is_encrypted(self.raw) > 0
                || ffi::archive_entry_is_data_encrypted(self.raw) > 0
                || ffi::archive_entry_is_metadata_encrypted(self.raw) > 0
        };
        if encrypted {
            Err(CoreError::EncryptedArchive)
        } else {
            Ok(())
        }
    }
}

mod ffi {
    use super::{ArchiveMode, c_char, c_int, c_void};

    pub enum Archive {}
    pub enum ArchiveEntry {}

    unsafe extern "C" {
        pub fn archive_read_new() -> *mut Archive;
        pub fn archive_read_support_filter_all(archive: *mut Archive) -> c_int;
        pub fn archive_read_support_format_zip(archive: *mut Archive) -> c_int;
        pub fn archive_read_support_format_7zip(archive: *mut Archive) -> c_int;
        pub fn archive_read_support_format_rar(archive: *mut Archive) -> c_int;
        pub fn archive_read_support_format_rar5(archive: *mut Archive) -> c_int;
        pub fn archive_read_set_option(
            archive: *mut Archive,
            module: *const c_char,
            option: *const c_char,
            value: *const c_char,
        ) -> c_int;
        #[cfg(unix)]
        pub fn archive_read_open_filename(
            archive: *mut Archive,
            filename: *const c_char,
            block_size: usize,
        ) -> c_int;
        #[cfg(windows)]
        pub fn archive_read_open_filename_w(
            archive: *mut Archive,
            filename: *const u16,
            block_size: usize,
        ) -> c_int;
        pub fn archive_read_next_header(
            archive: *mut Archive,
            entry: *mut *mut ArchiveEntry,
        ) -> c_int;
        pub fn archive_read_data(archive: *mut Archive, buffer: *mut c_void, size: usize) -> isize;
        pub fn archive_read_has_encrypted_entries(archive: *mut Archive) -> c_int;
        pub fn archive_error_string(archive: *mut Archive) -> *const c_char;
        pub fn archive_format(archive: *mut Archive) -> c_int;
        pub fn archive_read_free(archive: *mut Archive) -> c_int;
        pub fn archive_entry_pathname(entry: *mut ArchiveEntry) -> *const c_char;
        pub fn archive_entry_pathname_utf8(entry: *mut ArchiveEntry) -> *const c_char;
        #[cfg(unix)]
        pub fn archive_entry_pathname_w(entry: *mut ArchiveEntry) -> *const libc::wchar_t;
        pub fn archive_entry_filetype(entry: *mut ArchiveEntry) -> ArchiveMode;
        pub fn archive_entry_hardlink(entry: *mut ArchiveEntry) -> *const c_char;
        pub fn archive_entry_symlink(entry: *mut ArchiveEntry) -> *const c_char;
        pub fn archive_entry_size(entry: *mut ArchiveEntry) -> i64;
        pub fn archive_entry_is_encrypted(entry: *mut ArchiveEntry) -> c_int;
        pub fn archive_entry_is_data_encrypted(entry: *mut ArchiveEntry) -> c_int;
        pub fn archive_entry_is_metadata_encrypted(entry: *mut ArchiveEntry) -> c_int;
    }
}
