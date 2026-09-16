use std::fs::{File, OpenOptions};
use std::io;
use std::path::Path;

use fs2::FileExt;

use crate::error::{CoreError, Result};

pub(super) struct LibraryLock {
    file: File,
}

impl LibraryLock {
    pub(super) fn acquire(root: &Path) -> Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(root.join("library.lock"))?;
        match FileExt::try_lock_exclusive(&file) {
            Ok(()) => Ok(Self { file }),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                Err(CoreError::LibraryLocked)
            }
            Err(error) => Err(CoreError::Io(error)),
        }
    }
}

impl Drop for LibraryLock {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.file);
    }
}
