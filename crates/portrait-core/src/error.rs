use std::io;

#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("Invalid portable backup: {0}")]
    InvalidBackup(String),
    #[error("Confirm the exact backup path before overwriting this file.")]
    BackupConfirmationRequired,
    #[error("Another export is using this destination. Wait for it to finish, then preview again.")]
    ExportDestinationBusy,
    #[error("Select at least one active portrait before exporting the selection.")]
    EmptySelection,
    #[error(
        "The export target is unsafe. Choose a dedicated portrait collection outside the library, home root, or game installation root."
    )]
    ExportTargetUnsafe,
    #[error("Explicitly designate this directory as a portrait collection before replacing it.")]
    ExportTargetNotDesignated,
    #[error("This export preview is unavailable or has already been used. Preview again.")]
    ExportPlanNotFound,
    #[error("The library or destination changed. Preview again and confirm the new plan.")]
    ExportPlanStale,
    #[error("Confirm this exact export preview before replacing or overwriting files.")]
    ExportConfirmationRequired,
    #[error("The export destination exceeds the safe preview size limit.")]
    ExportPreviewLimit,

    #[error("The selected folder is not empty. Choose an empty folder or create a new one.")]
    DestinationNotEmpty,
    #[error("No portrait library was found at the selected location.")]
    LibraryNotFound,
    #[error("This portrait library is open in another window. Close it there and try again.")]
    LibraryLocked,
    #[error("The library manifest is invalid: {0}")]
    InvalidManifest(String),
    #[error(
        "This library uses format version {found}, but this app supports up to version {supported}. Update the app before opening it."
    )]
    UnsupportedFormatVersion { found: u32, supported: u32 },
    #[error(
        "This library database uses schema version {found}, but this app supports up to version {supported}. Update the app before opening it."
    )]
    UnsupportedSchemaVersion { found: u32, supported: u32 },
    #[error("The library database is missing.")]
    MissingDatabase,
    #[error("The selected library path has no usable folder name.")]
    InvalidLibraryName,
    #[error("A managed library directory is unsafe or has been replaced with a link.")]
    UnsafeManagedDirectory,
    #[error("Archive entry path is unsafe: {0}")]
    UnsafeArchivePath(String),
    #[error("Archive entry is not a regular file or directory: {0}")]
    UnsafeArchiveEntryType(String),
    #[error("Archive entries collide after portable path normalization: {0}")]
    ArchivePathCollision(String),
    #[error("The archive format is not supported. Select a ZIP, RAR, RAR5, or 7z archive.")]
    UnsupportedArchiveFormat,
    #[error("The archive is invalid or truncated: {0}")]
    InvalidArchive(String),
    #[error("Encrypted archives are not supported.")]
    EncryptedArchive,
    #[error("Multipart archives are not supported.")]
    MultipartArchive,
    #[error("The archive contains too many entries.")]
    ArchiveEntryLimit,
    #[error("An archive entry exceeds the per-file extraction limit.")]
    ArchiveFileSizeLimit,
    #[error("The archive exceeds the total extraction limit.")]
    ArchiveTotalSizeLimit,
    #[error("Archive extraction requires a new staging directory.")]
    ArchiveStagingNotFresh,
    #[error("The operation was cancelled.")]
    Cancelled,
    #[error("The export was cancelled before commit. Recovery data was retained: {0}")]
    CancelledRecoveryPending(String),
    #[error("The import folder overlaps the managed library.")]
    ImportOverlapsLibrary,
    #[error("The portrait set is incomplete or contains unsupported files.")]
    InvalidPortraitSet,
    #[error("The portrait set is missing one or more required image roles.")]
    IncompletePortraitSet,
    #[error("The portrait set has ambiguous role filenames.")]
    AmbiguousPortraitSet,
    #[error("A portrait PNG could not be decoded safely: {0}")]
    InvalidPng(String),
    #[error("A library migration could not be completed: {0}")]
    Migration(String),
    #[error("Library recovery requires attention before this library can be opened: {0}")]
    Recovery(String),
    #[error("Catalog pages must request between 1 and 200 portraits.")]
    InvalidPagination,
    #[error("The requested portrait is not in the open library.")]
    PortraitNotFound,
    #[error("Thumbnail edge must be between 1 and 1024 pixels.")]
    InvalidThumbnailEdge,
    #[error("A portrait name must contain visible text.")]
    InvalidMetadataName,
    #[error("Rename one portrait at a time.")]
    MetadataNameRequiresSinglePortrait,
    #[error("A label category and value must contain visible text.")]
    InvalidMetadataLabel,
    #[error("The requested source is not in the open library.")]
    SourceNotFound,
    #[error(
        "The duplicate review is stale or contains portraits that are no longer exact active duplicates."
    )]
    InvalidDuplicateConsolidation,
    #[error("Filesystem operation failed: {0}")]
    Io(#[from] io::Error),
    #[error("Database operation failed: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("Manifest serialization failed: {0}")]
    Json(#[from] serde_json::Error),
}

impl CoreError {
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::InvalidBackup(_) => "BACKUP_INVALID",
            Self::BackupConfirmationRequired => "BACKUP_CONFIRMATION_REQUIRED",
            Self::ExportDestinationBusy => "EXPORT_DESTINATION_BUSY",
            Self::EmptySelection => "EMPTY_SELECTION",
            Self::ExportTargetUnsafe => "EXPORT_TARGET_UNSAFE",
            Self::ExportTargetNotDesignated => "EXPORT_TARGET_NOT_DESIGNATED",
            Self::ExportPlanNotFound => "EXPORT_PLAN_NOT_FOUND",
            Self::ExportPlanStale => "EXPORT_PLAN_STALE",
            Self::ExportConfirmationRequired => "EXPORT_CONFIRMATION_REQUIRED",
            Self::ExportPreviewLimit => "EXPORT_PREVIEW_LIMIT",

            Self::DestinationNotEmpty => "LIBRARY_DESTINATION_NOT_EMPTY",
            Self::LibraryNotFound => "LIBRARY_NOT_FOUND",
            Self::LibraryLocked => "LIBRARY_LOCKED",
            Self::InvalidManifest(_) => "LIBRARY_MANIFEST_INVALID",
            Self::UnsupportedFormatVersion { .. } => "LIBRARY_FORMAT_TOO_NEW",
            Self::UnsupportedSchemaVersion { .. } => "LIBRARY_SCHEMA_TOO_NEW",
            Self::MissingDatabase => "LIBRARY_DATABASE_MISSING",
            Self::InvalidLibraryName => "LIBRARY_NAME_INVALID",
            Self::UnsafeManagedDirectory => "LIBRARY_MANAGED_PATH_UNSAFE",
            Self::UnsafeArchivePath(_) => "ARCHIVE_UNSAFE_PATH",
            Self::UnsafeArchiveEntryType(_) => "ARCHIVE_UNSAFE_ENTRY_TYPE",
            Self::ArchivePathCollision(_) => "ARCHIVE_PATH_COLLISION",
            Self::UnsupportedArchiveFormat => "ARCHIVE_UNSUPPORTED_FORMAT",
            Self::InvalidArchive(_) => "ARCHIVE_INVALID",
            Self::EncryptedArchive => "ARCHIVE_ENCRYPTED",
            Self::MultipartArchive => "ARCHIVE_MULTIPART",
            Self::ArchiveEntryLimit => "ARCHIVE_ENTRY_LIMIT",
            Self::ArchiveFileSizeLimit => "ARCHIVE_FILE_SIZE_LIMIT",
            Self::ArchiveTotalSizeLimit => "ARCHIVE_TOTAL_SIZE_LIMIT",
            Self::ArchiveStagingNotFresh => "ARCHIVE_STAGING_NOT_FRESH",
            Self::Cancelled | Self::CancelledRecoveryPending(_) => "CANCELLED",
            Self::ImportOverlapsLibrary => "IMPORT_OVERLAPS_LIBRARY",
            Self::InvalidPortraitSet => "INVALID_PORTRAIT_SET",
            Self::IncompletePortraitSet => "INCOMPLETE_SET",
            Self::AmbiguousPortraitSet => "AMBIGUOUS_SET",
            Self::InvalidPng(_) => "INVALID_PNG",
            Self::Migration(_) => "LIBRARY_MIGRATION_FAILED",
            Self::Recovery(_) => "RECOVERY_FAILED",
            Self::InvalidPagination => "CATALOG_PAGE_INVALID",
            Self::PortraitNotFound => "PORTRAIT_NOT_FOUND",
            Self::InvalidThumbnailEdge => "THUMBNAIL_EDGE_INVALID",
            Self::InvalidMetadataName => "METADATA_NAME_INVALID",
            Self::MetadataNameRequiresSinglePortrait => "METADATA_NAME_REQUIRES_SINGLE_PORTRAIT",
            Self::InvalidMetadataLabel => "METADATA_LABEL_INVALID",
            Self::SourceNotFound => "SOURCE_NOT_FOUND",
            Self::InvalidDuplicateConsolidation => "DUPLICATE_CONSOLIDATION_INVALID",
            Self::Io(_) => "FILESYSTEM_ERROR",
            Self::Database(_) => "DATABASE_ERROR",
            Self::Json(_) => "SERIALIZATION_ERROR",
        }
    }

    #[must_use]
    pub const fn recoverable(&self) -> bool {
        !matches!(
            self,
            Self::UnsupportedFormatVersion { .. }
                | Self::UnsupportedSchemaVersion { .. }
                | Self::InvalidManifest(_)
        )
    }
}

pub type Result<T> = std::result::Result<T, CoreError>;
