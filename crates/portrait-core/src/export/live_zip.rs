//! In-memory proof of the bytes written by a live, seekable ZIP writer.
//! Only completed stamps are durable: a crash still retains ambiguous partial files.
use super::{journal::Stamp, plan::identity};
use sha2::{Digest, Sha256};
use std::{
    cell::RefCell,
    fs::File,
    io::{self, Read, Seek, SeekFrom, Write},
    rc::Rc,
};

const BLOCK: u64 = 4096;
const WRITE_CHUNK: usize = 65536;

#[derive(Clone)]
pub(super) struct LiveZip(Rc<RefCell<State>>);
struct State {
    file: File,
    identity: Vec<u8>,
    blocks: Vec<[u8; 32]>,
    len: u64,
    position: u64,
    frozen: bool,
}
fn changed() -> io::Error {
    io::Error::other("The partial ZIP changed outside the export writer")
}
impl LiveZip {
    pub fn new(file: File) -> io::Result<Self> {
        let metadata = file.metadata()?;
        if metadata.len() != 0 {
            return Err(changed());
        }
        Ok(Self(Rc::new(RefCell::new(State {
            identity: identity(&metadata),
            file,
            blocks: Vec::new(),
            len: 0,
            position: 0,
            frozen: false,
        }))))
    }
    /// ZipWriter finalizes on Drop. Discard those writes after an error so neither
    /// cancellation nor an observed external edit can modify the partial output.
    pub fn freeze(&self) {
        self.0.borrow_mut().frozen = true;
    }
    pub fn sync_all(&self) -> io::Result<()> {
        self.0.borrow().file.sync_all()
    }
    /// Verify every block against the successful writes, never bless arbitrary
    /// bytes read from disk. Hash storage is 32 bytes per 4 KiB of archive data.
    pub fn verified_stamp(&self) -> io::Result<Stamp> {
        let mut s = self.0.borrow_mut();
        s.check_len()?;
        let mut full = Sha256::new();
        s.file.seek(SeekFrom::Start(0))?;
        let mut buffer = [0; WRITE_CHUNK];
        let mut remaining = s.len;
        let mut index = 0;
        while remaining != 0 {
            let n = remaining.min(WRITE_CHUNK as u64) as usize;
            s.file.read_exact(&mut buffer[..n])?;
            for block in buffer[..n].chunks(BLOCK as usize) {
                let actual: [u8; 32] = Sha256::digest(block).into();
                if s.blocks.get(index) != Some(&actual) {
                    return Err(changed());
                }
                index += 1;
            }
            full.update(&buffer[..n]);
            remaining -= n as u64;
        }
        s.check_len()?;
        Ok(Stamp {
            identity: s.identity.clone(),
            digest: format!("{:x}", full.finalize()),
        })
    }
}
impl State {
    fn check_len(&self) -> io::Result<()> {
        let m = self.file.metadata()?;
        if m.len() != self.len || identity(&m) != self.identity {
            return Err(changed());
        }
        Ok(())
    }
    fn read_verified(&mut self, index: usize) -> io::Result<Vec<u8>> {
        let start = index as u64 * BLOCK;
        let len = self.len.saturating_sub(start).min(BLOCK) as usize;
        let mut bytes = vec![0; len];
        if len != 0 {
            self.file.seek(SeekFrom::Start(start))?;
            self.file.read_exact(&mut bytes)?;
            let actual: [u8; 32] = Sha256::digest(&bytes).into();
            if self.blocks.get(index) != Some(&actual) {
                return Err(changed());
            }
        }
        Ok(bytes)
    }
}
impl Write for LiveZip {
    fn write(&mut self, input: &[u8]) -> io::Result<usize> {
        let mut s = self.0.borrow_mut();
        if s.frozen {
            s.position = s.position.saturating_add(input.len() as u64);
            return Ok(input.len());
        }
        if input.is_empty() {
            return Ok(0);
        }
        s.check_len()?;
        // Bound temporary memory even when ZIP metadata supplies a larger slice.
        let input = &input[..input.len().min(WRITE_CHUNK)];
        let start = s.position;
        let end = start.checked_add(input.len() as u64).ok_or_else(changed)?;
        let first = (start / BLOCK) as usize;
        let last = ((end - 1) / BLOCK) as usize;
        let mut expected = Vec::with_capacity(last - first + 1);
        for index in first..=last {
            expected.push(s.read_verified(index)?);
        }
        s.file.seek(SeekFrom::Start(start))?;
        let n = s.file.write(input)?;
        let end = start + n as u64;
        for (offset, mut bytes) in expected.into_iter().enumerate() {
            let index = first + offset;
            let block_start = index as u64 * BLOCK;
            let from = start.max(block_start);
            let to = end.min(block_start + BLOCK);
            if from >= to {
                continue;
            }
            bytes.resize(bytes.len().max((to - block_start) as usize), 0);
            bytes[(from - block_start) as usize..(to - block_start) as usize]
                .copy_from_slice(&input[(from - start) as usize..(to - start) as usize]);
            let hash = Sha256::digest(bytes).into();
            if index == s.blocks.len() {
                s.blocks.push(hash);
            } else {
                s.blocks[index] = hash;
            }
        }
        s.position = end;
        s.len = s.len.max(end);
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.0.borrow_mut().file.flush()
    }
}
impl Seek for LiveZip {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        let mut s = self.0.borrow_mut();
        let position = match from {
            SeekFrom::Start(n) => i128::from(n),
            SeekFrom::End(n) => i128::from(s.len) + i128::from(n),
            SeekFrom::Current(n) => i128::from(s.position) + i128::from(n),
        };
        if position < 0
            || position > i128::from(u64::MAX)
            || (!s.frozen && position > i128::from(s.len))
        {
            return Err(io::Error::other(
                "ZIP writer seek outside its recorded content",
            ));
        }
        s.position = position as u64;
        Ok(s.position)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buffered_zip_roundtrip_with_1100_entries_and_large_blocks() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let live = LiveZip::new(file.reopen().unwrap()).unwrap();
        let mut zip =
            zip::ZipWriter::new(std::io::BufWriter::with_capacity(WRITE_CHUNK, live.clone()));
        let began = std::time::Instant::now();
        for index in 0..1100 {
            zip.start_file(
                index.to_string(),
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Stored),
            )
            .unwrap();
            zip.write_all(&vec![
                index as u8;
                if index < 3 { WRITE_CHUNK * 3 + 17 } else { 128 }
            ])
            .unwrap();
            zip.flush().unwrap();
        }
        zip.finish().unwrap().flush().unwrap();
        live.sync_all().unwrap();
        let bytes = std::fs::read(file.path()).unwrap();
        assert_eq!(
            live.verified_stamp().unwrap().digest,
            format!("{:x}", Sha256::digest(&bytes))
        );
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        assert_eq!(archive.len(), 1100);
        for index in 0..1100 {
            let mut bytes = Vec::new();
            archive
                .by_name(&index.to_string())
                .unwrap()
                .read_to_end(&mut bytes)
                .unwrap();
            assert_eq!(
                bytes,
                vec![index as u8; if index < 3 { WRITE_CHUNK * 3 + 17 } else { 128 }]
            );
        }
        eprintln!(
            "LIVE ZIP 1100 entries + cross-block payloads: {:.3}s",
            began.elapsed().as_secs_f64()
        );
    }

    #[test]
    fn header_rewrites_and_cross_block_writes_produce_exact_proof() {
        let file = tempfile::tempfile().unwrap();
        let mut writer = LiveZip::new(file).unwrap();
        let mut expected = vec![42; BLOCK as usize * 3 + 23];
        writer.write_all(&expected).unwrap();
        writer.seek(SeekFrom::Start(BLOCK - 7)).unwrap();
        writer.write_all(&[17; 29]).unwrap();
        expected[BLOCK as usize - 7..BLOCK as usize + 22].fill(17);
        assert_eq!(
            writer.verified_stamp().unwrap().digest,
            format!("{:x}", Sha256::digest(expected))
        );
    }

    #[test]
    fn same_length_external_edit_is_never_blessed_or_overwritten() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let mut writer = LiveZip::new(file.reopen().unwrap()).unwrap();
        writer.write_all(&vec![42; BLOCK as usize * 2]).unwrap();
        let mut external = file.reopen().unwrap();
        external.seek(SeekFrom::Start(3)).unwrap();
        external.write_all(b"external").unwrap();
        // Writing later, unaffected bytes cannot grant ownership of this edit.
        writer.write_all(b"tail").unwrap();
        assert!(writer.verified_stamp().is_err());
        writer.seek(SeekFrom::Start(0)).unwrap();
        assert!(writer.write_all(b"header").is_err());
        assert_eq!(&std::fs::read(file.path()).unwrap()[3..11], b"external");
        writer.freeze();
        writer
            .write_all(b"Drop finalization must be discarded")
            .unwrap();
        assert_eq!(&std::fs::read(file.path()).unwrap()[3..11], b"external");
    }
}
