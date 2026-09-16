mod plan;
pub(crate) use plan::PlanRegistry;
pub use plan::{
    ExportHistory, StoredExportPlan, file_digest, plan_export, plan_export_for_destination,
    stable_folder_name,
};

mod apply;
pub(crate) mod journal;
mod live_zip;
pub use apply::{ExportBoundary, apply_export, apply_export_with_observer};
pub use journal::{
    ExportReceipt, acknowledge_export_receipt, pending_export_receipts, target_identity,
};

/// Resolve and validate a target before desktop identity routing.
pub fn resolve_export_target(
    lib: &crate::Library,
    request: &crate::types::ExportRequest,
) -> crate::Result<std::path::PathBuf> {
    plan::safe_target(lib, &request.target, request.output)
}

pub use journal::recover_export_operations_with_observer;
