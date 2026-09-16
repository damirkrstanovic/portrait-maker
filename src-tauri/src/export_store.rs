//! Machine-local target identities are independent of manifests and saved destination records.
use portrait_core::{
    Library,
    export::{
        ExportHistory, ExportReceipt, acknowledge_export_receipt, pending_export_receipts,
        target_identity,
    },
    types::AppError,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use uuid::Uuid;
#[derive(Clone)]
pub(crate) struct ExportStore {
    path: PathBuf,
    lock: Arc<Mutex<()>>,
}
#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Store {
    targets: Vec<Target>,
    histories: BTreeMap<String, ExportHistory>,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Target {
    id: Uuid,
    path: PathBuf,
    identity: String,
    saved: bool,
}
impl ExportStore {
    pub fn new(config: &Path) -> Self {
        Self {
            path: config.join("export-history.json"),
            lock: Arc::new(Mutex::new(())),
        }
    }
    pub fn route(
        &self,
        path: &Path,
        library: Uuid,
        saved: Option<Uuid>,
    ) -> Result<(Uuid, ExportHistory), AppError> {
        let _guard = self.lock.lock().map_err(error)?;
        let mut store = self.read()?;
        let identity = target_identity(path).map_err(error)?;
        let existing = store.targets.iter().position(|t| {
            t.path == path && t.saved == saved.is_some() && saved.is_none_or(|id| t.id == id)
        });
        let id = if let Some(n) = existing {
            if store.targets[n].identity == identity {
                store.targets[n].id
            } else {
                let old = store.targets.remove(n);
                store
                    .histories
                    .retain(|_, h| h.destination_id != Some(old.id));
                let id = saved.unwrap_or_else(Uuid::new_v4);
                store.targets.push(Target {
                    id,
                    path: path.into(),
                    identity,
                    saved: saved.is_some(),
                });
                id
            }
        } else {
            let id = saved.unwrap_or_else(Uuid::new_v4);
            store.targets.push(Target {
                id,
                path: path.into(),
                identity,
                saved: saved.is_some(),
            });
            id
        };
        let history = store
            .histories
            .get(&format!("{id}:{library}"))
            .cloned()
            .unwrap_or_default();
        self.write(&store)?;
        Ok((id, history))
    }
    pub fn flush(&self, library: &Library) -> Result<(), AppError> {
        for receipt in pending_export_receipts(library).map_err(error)? {
            self.persist(&receipt)?;
            acknowledge_export_receipt(library, receipt.id).map_err(error)?;
        }
        Ok(())
    }
    fn persist(&self, receipt: &ExportReceipt) -> Result<(), AppError> {
        let _guard = self.lock.lock().map_err(error)?;
        let mut store = self.read()?;
        let h = &receipt.history;
        let (Some(id), Some(library), Some(path)) =
            (h.destination_id, h.library_id, h.destination.as_ref())
        else {
            return Ok(());
        };
        // A deleted/reset target record cannot be recreated from a historical receipt.
        if let Some(target) = store
            .targets
            .iter_mut()
            .find(|t| t.id == id && &t.path == path)
        {
            let current = target_identity(path).map_err(error)?;
            if current != receipt.target_identity {
                return Ok(());
            }
            target.identity = current;
            store.histories.insert(format!("{id}:{library}"), h.clone());
            self.write(&store)?;
        }
        Ok(())
    }
    fn read(&self) -> Result<Store, AppError> {
        match fs::read(&self.path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(error),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Store::default()),
            Err(e) => Err(error(e)),
        }
    }
    fn write(&self, store: &Store) -> Result<(), AppError> {
        let parent = self.path.parent().unwrap();
        fs::create_dir_all(parent).map_err(error)?;
        let staging = parent.join(format!(".export-history-{}.tmp", Uuid::new_v4()));
        let result = (|| {
            let mut f = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&staging)?;
            f.write_all(&serde_json::to_vec_pretty(store)?)?;
            f.sync_all()?;
            fs::rename(&staging, &self.path)?;
            #[cfg(unix)]
            fs::File::open(parent)?.sync_all()?;
            Ok::<_, Box<dyn std::error::Error>>(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(staging);
        }
        result.map_err(error)
    }
}
fn error(e: impl std::fmt::Display) -> AppError {
    AppError {
        code: "EXPORT_HISTORY_ERROR".into(),
        message: format!("Export history could not be saved or read: {e}"),
        recoverable: true,
    }
}
