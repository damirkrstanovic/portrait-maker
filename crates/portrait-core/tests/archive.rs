use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use portrait_core::CoreError;
use portrait_core::import::archive::{
    ExtractionLimits, JobContext, extract_archive, validate_entry_path,
};
use tempfile::TempDir;

fn write_test_archive(temp: &TempDir, name: &str, bytes: &[u8]) -> PathBuf {
    let input = temp.path().join(name);
    std::fs::write(&input, bytes).unwrap();
    input
}

fn safe_zip_bytes() -> Vec<u8> {
    std::fs::read(fixture("safe.zip")).unwrap()
}

fn find_eocd(bytes: &[u8]) -> usize {
    bytes
        .windows(4)
        .rposition(|window| window == b"PK\x05\x06")
        .unwrap()
}

fn zip_with_comment(comment: &[u8]) -> Vec<u8> {
    let mut bytes = safe_zip_bytes();
    let eocd = find_eocd(&bytes);
    bytes[eocd + 20..eocd + 22].copy_from_slice(&(comment.len() as u16).to_le_bytes());
    bytes.extend_from_slice(comment);
    bytes
}

fn zip64_fixture(disk: u32, total_disks: u32) -> Vec<u8> {
    let mut bytes = safe_zip_bytes();
    let eocd = find_eocd(&bytes);
    let entries = u16::from_le_bytes(bytes[eocd + 10..eocd + 12].try_into().unwrap());
    let central_size = u32::from_le_bytes(bytes[eocd + 12..eocd + 16].try_into().unwrap());
    let central_offset = u32::from_le_bytes(bytes[eocd + 16..eocd + 20].try_into().unwrap());

    let mut records = Vec::new();
    records.extend_from_slice(b"PK\x06\x06");
    records.extend_from_slice(&44_u64.to_le_bytes());
    records.extend_from_slice(&45_u16.to_le_bytes());
    records.extend_from_slice(&45_u16.to_le_bytes());
    records.extend_from_slice(&disk.to_le_bytes());
    records.extend_from_slice(&disk.to_le_bytes());
    records.extend_from_slice(&u64::from(entries).to_le_bytes());
    records.extend_from_slice(&u64::from(entries).to_le_bytes());
    records.extend_from_slice(&u64::from(central_size).to_le_bytes());
    records.extend_from_slice(&u64::from(central_offset).to_le_bytes());
    records.extend_from_slice(b"PK\x06\x07");
    records.extend_from_slice(&disk.to_le_bytes());
    records.extend_from_slice(&(eocd as u64).to_le_bytes());
    records.extend_from_slice(&total_disks.to_le_bytes());
    bytes.splice(eocd..eocd, records);

    let shifted_eocd = eocd + 76;
    bytes[shifted_eocd + 8..shifted_eocd + 12].fill(0xff);
    bytes[shifted_eocd + 12..shifted_eocd + 20].fill(0xff);
    bytes
}

fn encode_vint(mut value: u64) -> Vec<u8> {
    let mut bytes = Vec::new();
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        bytes.push(byte);
        if value == 0 {
            return bytes;
        }
    }
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn extract_fixture(name: &str) -> (TempDir, PathBuf) {
    let temp = TempDir::new().unwrap();
    let staging = temp.path().join("staging");
    extract_archive(
        &fixture(name),
        &staging,
        &ExtractionLimits::default(),
        &JobContext::default(),
    )
    .unwrap();
    (temp, staging)
}

#[test]
fn archive_paths_cannot_escape_on_any_platform() {
    for name in [
        "../x",
        "/tmp/x",
        "C:\\x",
        "a/../../x",
        "a\\..\\..\\x",
        "//server/share",
        "file:///tmp/x",
        "safe\0unsafe",
    ] {
        assert!(validate_entry_path(name).is_err(), "{name}");
    }
    assert_eq!(
        validate_entry_path("pack/elf/Small.png").unwrap(),
        PathBuf::from("pack/elf/Small.png")
    );
    assert_eq!(
        validate_entry_path("pack\\elf//./Small.png").unwrap(),
        PathBuf::from("pack/elf/Small.png")
    );
}

#[test]
fn archive_paths_reject_windows_reserved_names_and_ambiguous_components() {
    for name in [
        "",
        "CON",
        "con.png",
        "pack/AUX/file",
        "Lpt9.txt",
        "name.",
        "name ",
        "a:b",
        "bad?.png",
        "bad|name.png",
        "CONIN$",
        "CONOUT$.txt",
    ] {
        assert!(validate_entry_path(name).is_err(), "{name}");
    }
}

#[test]
fn zip_7z_rar4_and_rar5_fixtures_extract_without_external_tools() {
    for (archive, expected_path, expected) in [
        (
            "safe.zip",
            "pack/elf/Small.png",
            b"zip fixture\n".as_slice(),
        ),
        (
            "safe.7z",
            "file1",
            b"                          file 1 contents\nhello\nhello\nhello\n".as_slice(),
        ),
        (
            "safe-rar4.rar",
            "testdir/test.txt",
            b"test text file\r\n".as_slice(),
        ),
        (
            "safe-rar5.rar",
            "helloworld.txt",
            b"hello libarchive test suite!\n".as_slice(),
        ),
    ] {
        let (_temp, staging) = extract_fixture(archive);
        assert_eq!(
            std::fs::read(staging.join(expected_path)).unwrap(),
            expected
        );
    }
}

#[test]
fn utf8_archive_names_are_preserved() {
    let (_temp, staging) = extract_fixture("unicode-name.zip");
    assert_eq!(std::fs::read(staging.join("λ.txt")).unwrap(), b"lambda");
}

#[test]
fn extraction_rejects_links_collisions_truncation_and_unsupported_formats() {
    for (archive, code) in [
        ("symlink.zip", "ARCHIVE_UNSAFE_ENTRY_TYPE"),
        ("case-collision.zip", "ARCHIVE_PATH_COLLISION"),
        ("nested-case-collision.zip", "ARCHIVE_PATH_COLLISION"),
        ("unicode-case-collision.zip", "ARCHIVE_PATH_COLLISION"),
        ("truncated.zip", "ARCHIVE_INVALID"),
        ("multidisk.zip", "ARCHIVE_MULTIPART"),
        ("not-an-archive.txt", "ARCHIVE_UNSUPPORTED_FORMAT"),
    ] {
        let temp = TempDir::new().unwrap();
        let staging = temp.path().join("staging");
        let error = extract_archive(
            &fixture(archive),
            &staging,
            &ExtractionLimits::default(),
            &JobContext::default(),
        )
        .unwrap_err();
        assert_eq!(error.code(), code, "{archive}: {error}");
        assert!(!staging.exists(), "failed extraction leaked {archive}");
    }
}

#[test]
fn malformed_rar_offsets_return_invalid_archive_without_panicking_or_looping() {
    let temp = TempDir::new().unwrap();
    let mut rar4 = b"Rar!\x1a\x07\x00".to_vec();
    rar4.extend_from_slice(&[0, 0, 0x74, 0, 0x80, 7, 0, 0, 0, 0, 0]);

    let mut rar5 = b"Rar!\x1a\x07\x01\x00".to_vec();
    rar5.extend_from_slice(&[0, 0, 0, 0]);
    rar5.extend_from_slice(&encode_vint(12));
    rar5.extend_from_slice(&encode_vint(2));
    rar5.extend_from_slice(&encode_vint(2));
    rar5.extend_from_slice(&encode_vint(i64::MAX as u64 + 1));

    for (name, bytes) in [("bad.rar", rar4), ("bad-rar5.rar", rar5)] {
        let input = write_test_archive(&temp, name, &bytes);
        let error = extract_archive(
            &input,
            &temp.path().join(format!("staging-{name}")),
            &ExtractionLimits::default(),
            &JobContext::default(),
        )
        .unwrap_err();
        assert_eq!(error.code(), "ARCHIVE_INVALID", "{name}: {error}");
    }
}

#[test]
fn rar_preflight_header_walk_has_an_independent_safety_budget() {
    let temp = TempDir::new().unwrap();
    let mut rar = b"Rar!\x1a\x07\x00".to_vec();
    for _ in 0..200_001 {
        rar.extend_from_slice(&[0, 0, 0x72, 0, 0, 7, 0]);
    }
    let input = write_test_archive(&temp, "too-many-headers.rar", &rar);

    let error = extract_archive(
        &input,
        &temp.path().join("staging"),
        &ExtractionLimits::default(),
        &JobContext::default(),
    )
    .unwrap_err();

    assert_eq!(error.code(), "ARCHIVE_INVALID");
}

#[test]
fn one_file_rar_is_allowed_when_entry_limit_is_one() {
    let temp = TempDir::new().unwrap();
    let staging = temp.path().join("staging");
    let limits = ExtractionLimits {
        max_entries: 1,
        ..ExtractionLimits::default()
    };

    extract_archive(
        &fixture("safe-rar5.rar"),
        &staging,
        &limits,
        &JobContext::default(),
    )
    .unwrap();

    assert_eq!(
        std::fs::read(staging.join("helloworld.txt")).unwrap(),
        b"hello libarchive test suite!\n"
    );
}

#[test]
fn zip_eocd_validation_accepts_signature_in_comment_and_single_disk_zip64() {
    let temp = TempDir::new().unwrap();
    for (name, bytes) in [
        ("comment.zip", zip_with_comment(b"comment PK\x05\x06 tail")),
        ("single-disk-zip64.zip", zip64_fixture(0, 1)),
    ] {
        let input = write_test_archive(&temp, name, &bytes);
        let staging = temp.path().join(format!("staging-{name}"));
        extract_archive(
            &input,
            &staging,
            &ExtractionLimits::default(),
            &JobContext::default(),
        )
        .unwrap();
        assert_eq!(
            std::fs::read(staging.join("pack/elf/Small.png")).unwrap(),
            b"zip fixture\n"
        );
    }
}

#[test]
fn zip64_multidisk_metadata_is_rejected_before_extraction() {
    let temp = TempDir::new().unwrap();
    let input = write_test_archive(&temp, "zip64-multidisk.zip", &zip64_fixture(1, 2));
    let staging = temp.path().join("staging");

    let error = extract_archive(
        &input,
        &staging,
        &ExtractionLimits::default(),
        &JobContext::default(),
    )
    .unwrap_err();

    assert_eq!(error.code(), "ARCHIVE_MULTIPART", "{error}");
    assert!(!staging.exists());
}

#[test]
fn extraction_rejects_encrypted_and_multipart_archives() {
    let temp = TempDir::new().unwrap();
    let encrypted_error = extract_archive(
        &fixture("encrypted.7z"),
        &temp.path().join("encrypted"),
        &ExtractionLimits::default(),
        &JobContext::default(),
    )
    .unwrap_err();
    assert_eq!(encrypted_error.code(), "ARCHIVE_ENCRYPTED");

    for (source, target_name) in [
        ("multipart.part01.rar", "multipart.part01.rar"),
        ("multipart.part01.rar", "renamed.rar"),
    ] {
        let input = temp.path().join(target_name);
        std::fs::copy(fixture(source), &input).unwrap();
        let error = extract_archive(
            &input,
            &temp.path().join(format!("staging-{target_name}")),
            &ExtractionLimits::default(),
            &JobContext::default(),
        )
        .unwrap_err();
        assert_eq!(error.code(), "ARCHIVE_MULTIPART", "{target_name}: {error}");
    }
}

#[test]
fn extraction_reports_streamed_progress() {
    let completed = Arc::new(AtomicU64::new(0));
    let observed = Arc::clone(&completed);
    let job = JobContext::with_progress(move |bytes, _total| {
        observed.store(bytes, Ordering::Release);
    });
    let temp = TempDir::new().unwrap();

    extract_archive(
        &fixture("safe.zip"),
        &temp.path().join("staging"),
        &ExtractionLimits::default(),
        &job,
    )
    .unwrap();

    assert_eq!(completed.load(Ordering::Acquire), 12);
}

#[test]
fn extraction_enforces_each_resource_limit_using_streamed_bytes() {
    for (limits, code) in [
        (
            ExtractionLimits {
                max_entries: 1,
                ..ExtractionLimits::default()
            },
            "ARCHIVE_ENTRY_LIMIT",
        ),
        (
            ExtractionLimits {
                max_file_bytes: 3,
                ..ExtractionLimits::default()
            },
            "ARCHIVE_FILE_SIZE_LIMIT",
        ),
        (
            ExtractionLimits {
                max_total_bytes: 4,
                ..ExtractionLimits::default()
            },
            "ARCHIVE_TOTAL_SIZE_LIMIT",
        ),
    ] {
        let temp = TempDir::new().unwrap();
        let error = extract_archive(
            &fixture("two-files.zip"),
            &temp.path().join("staging"),
            &limits,
            &JobContext::default(),
        )
        .unwrap_err();
        assert_eq!(error.code(), code, "{error}");
    }
}

#[test]
fn extraction_honors_cancellation_before_publishing_any_file() {
    let temp = TempDir::new().unwrap();
    let staging = temp.path().join("staging");
    let job = JobContext::default();
    job.cancel();

    let error = extract_archive(
        &fixture("safe.zip"),
        &staging,
        &ExtractionLimits::default(),
        &job,
    )
    .unwrap_err();

    assert!(matches!(error, CoreError::Cancelled));
    assert!(!staging.exists());
}

#[test]
fn extraction_requires_a_fresh_staging_directory() {
    let temp = TempDir::new().unwrap();
    let staging = temp.path().join("staging");
    std::fs::create_dir(&staging).unwrap();

    let error = extract_archive(
        &fixture("safe.zip"),
        &staging,
        &ExtractionLimits::default(),
        &JobContext::default(),
    )
    .unwrap_err();

    assert_eq!(error.code(), "ARCHIVE_STAGING_NOT_FRESH");
}

#[test]
#[ignore = "reads a private local archive named by PORTRAIT_PRIVATE_ARCHIVE"]
fn private_archive_extracts_only_into_a_temporary_directory() {
    let input = std::env::var_os("PORTRAIT_PRIVATE_ARCHIVE")
        .map(PathBuf::from)
        .expect("set PORTRAIT_PRIVATE_ARCHIVE");
    let temp = TempDir::new().unwrap();
    let staging = temp.path().join("staging");

    extract_archive(
        &input,
        &staging,
        &ExtractionLimits::default(),
        &JobContext::default(),
    )
    .unwrap();

    assert!(std::fs::read_dir(staging).unwrap().next().is_some());
}
