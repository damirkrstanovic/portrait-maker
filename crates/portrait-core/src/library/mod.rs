mod lock;
pub(crate) mod migrations;

use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{CoreError, Result};
use lock::LibraryLock;

pub const CURRENT_FORMAT_VERSION: u32 = 1;

const MANIFEST_FILE: &str = "library.json";
const DATABASE_FILE: &str = "library.sqlite3";

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LibraryManifest {
    format_version: u32,
    library_id: Uuid,
}

pub struct Library {
    root: PathBuf,
    id: Uuid,
    name: String,
    connection: Connection,
    _lock: LibraryLock,
    pub(crate) export_registry: std::cell::RefCell<crate::export::PlanRegistry>,
}

impl fmt::Debug for Library {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Library")
            .field("root", &self.root)
            .field("id", &self.id)
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

impl Library {
    pub fn create(root: &Path) -> Result<Self> {
        prepare_empty_root(root)?;
        let id = Uuid::new_v4();
        write_new_manifest(
            root,
            &LibraryManifest {
                format_version: CURRENT_FORMAT_VERSION,
                library_id: id,
            },
        )?;
        let library_lock = LibraryLock::acquire(root)?;
        create_library_directories(root)?;

        let database_path = root.join(DATABASE_FILE);
        let connection = Connection::open_with_flags(
            &database_path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
        )?;
        configure_connection(&connection)?;
        migrations::apply(connection, &database_path, root, false)
            .and_then(|connection| Self::from_parts(root, id, connection, library_lock))
    }

    pub fn open(root: &Path) -> Result<Self> {
        if !root.is_dir() {
            return Err(CoreError::LibraryNotFound);
        }
        let manifest = read_manifest(root)?;
        validate_manifest(&manifest)?;
        let library_lock = LibraryLock::acquire(root)?;
        create_library_directories(root)?;

        let database_path = root.join(DATABASE_FILE);
        if !database_path.is_file() {
            return Err(CoreError::MissingDatabase);
        }
        let connection =
            Connection::open_with_flags(&database_path, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
        configure_connection(&connection)?;
        migrations::apply(connection, &database_path, root, true).and_then(|connection| {
            Self::from_parts(root, manifest.library_id, connection, library_lock)
        })
    }

    fn from_parts(
        root: &Path,
        id: Uuid,
        connection: Connection,
        library_lock: LibraryLock,
    ) -> Result<Self> {
        let name = root
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
            .ok_or(CoreError::InvalidLibraryName)?
            .to_owned();
        crate::recovery::recover_import_operations(root, &connection)?;
        Ok(Self {
            root: root.to_path_buf(),
            id,
            name,
            connection,
            _lock: library_lock,
            export_registry: std::cell::RefCell::new(crate::export::PlanRegistry::default()),
        })
    }

    #[must_use]
    pub const fn id(&self) -> Uuid {
        self.id
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    #[must_use]
    pub const fn connection(&self) -> &Connection {
        &self.connection
    }
}

fn prepare_empty_root(root: &Path) -> Result<()> {
    if root.exists() {
        if !root.is_dir() {
            return Err(CoreError::DestinationNotEmpty);
        }
        if fs::read_dir(root)?.next().transpose()?.is_some() {
            return Err(CoreError::DestinationNotEmpty);
        }
    } else {
        fs::create_dir_all(root)?;
    }
    Ok(())
}

fn write_new_manifest(root: &Path, manifest: &LibraryManifest) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(manifest)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(root.join(MANIFEST_FILE))?;
    file.write_all(&bytes)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    Ok(())
}

fn read_manifest(root: &Path) -> Result<LibraryManifest> {
    let path = root.join(MANIFEST_FILE);
    if !path.is_file() {
        return Err(CoreError::LibraryNotFound);
    }
    let bytes = fs::read(path)?;
    serde_json::from_slice(&bytes).map_err(|error| CoreError::InvalidManifest(error.to_string()))
}

fn validate_manifest(manifest: &LibraryManifest) -> Result<()> {
    if manifest.format_version != CURRENT_FORMAT_VERSION {
        return Err(CoreError::UnsupportedFormatVersion {
            found: manifest.format_version,
            supported: CURRENT_FORMAT_VERSION,
        });
    }
    Ok(())
}

fn create_library_directories(root: &Path) -> Result<()> {
    for directory in ["portraits", "cache", "staging"] {
        let path = root.join(directory);
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
            Ok(_) => return Err(CoreError::UnsafeManagedDirectory),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => fs::create_dir(&path)?,
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

fn configure_connection(connection: &Connection) -> Result<()> {
    connection.pragma_update(None, "foreign_keys", true)?;
    connection.busy_timeout(Duration::from_secs(5))?;
    connection.pragma_update(None, "journal_mode", "DELETE")?;
    Ok(())
}
