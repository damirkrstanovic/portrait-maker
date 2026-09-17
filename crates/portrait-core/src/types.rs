use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub type Id = Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Role {
    Small,
    Medium,
    Large,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Game {
    Kingmaker,
    Wotr,
}

impl Game {
    #[must_use]
    pub const fn steam_id(self) -> &'static str {
        match self {
            Self::Kingmaker => "640820",
            Self::Wotr => "1184370",
        }
    }

    #[must_use]
    pub const fn windows_suffix(self) -> &'static str {
        match self {
            Self::Kingmaker => "Owlcat Games/Pathfinder Kingmaker",
            Self::Wotr => "Owlcat Games/Pathfinder Wrath Of The Righteous",
        }
    }

    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Kingmaker => "Pathfinder: Kingmaker",
            Self::Wotr => "Pathfinder: Wrath of the Righteous",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Label {
    pub category: String,
    pub value: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Query {
    pub text: String,
    pub source_ids: Vec<Id>,
    pub labels: Vec<Label>,
    pub selected_only: bool,
    pub trash: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SelectionTarget {
    Ids(Vec<Id>),
    Matching(Query),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SelectionAction {
    Add,
    Remove,
    Clear,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Page {
    pub offset: u32,
    pub limit: u32,
}

impl<'de> Deserialize<'de> for Page {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Input {
            offset: u32,
            limit: u32,
        }

        let input = Input::deserialize(deserializer)?;
        if input.limit > 200 {
            return Err(serde::de::Error::custom("page limit must be at most 200"));
        }
        Ok(Self {
            offset: input.offset,
            limit: input.limit,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Portrait {
    pub id: Id,
    pub source_id: Id,
    pub name: String,
    pub source_name: String,
    pub original_folder: String,
    pub description: Option<String>,
    pub labels: Vec<Label>,
    pub selected: bool,
    pub trashed_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogPage {
    pub items: Vec<Portrait>,
    pub total: u64,
    pub revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceFacet {
    pub id: Id,
    pub name: String,
    pub count: u64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LabelFacet {
    pub category: String,
    pub value: String,
    pub display_value: String,
    pub count: u64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogFacets {
    pub sources: Vec<SourceFacet>,
    pub labels: Vec<LabelFacet>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum IssueSeverity {
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    pub path: String,
    pub code: String,
    pub message: String,
    pub severity: IssueSeverity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ImportKind {
    Folder,
    Archive,
    Game,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportRequest {
    pub path: PathBuf,
    pub source_name: String,
    pub kind: ImportKind,
    pub resize: bool,
    #[serde(default)]
    pub duplicate_policy: DuplicatePolicy,
}

/// What an import should do after its duplicate review has completed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DuplicatePolicy {
    /// Retain every complete portrait set. This preserves the historical import behaviour.
    #[default]
    Keep,
    /// Do not create a second active portrait for an exact match; retain its source and labels.
    Skip,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateMember {
    pub portrait: Portrait,
    pub source_names: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateGroup {
    pub fingerprint: String,
    pub members: Vec<DuplicateMember>,
    pub name_conflict: bool,
    pub description_conflict: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateScanReport {
    pub groups: Vec<DuplicateGroup>,
    pub issues: Vec<Issue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportDuplicateMatch {
    pub folder: String,
    pub name: String,
    pub matching_portrait_id: Option<Id>,
    pub matching_name: Option<String>,
    pub duplicate_of_in_batch: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportDuplicateReport {
    pub matches: Vec<ImportDuplicateMatch>,
    pub issues: Vec<Issue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateConsolidation {
    pub keep_id: Id,
    pub remove_ids: Vec<Id>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateConsolidationReport {
    pub trashed: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub source_id: Option<Id>,
    pub imported: u64,
    pub skipped: u64,
    pub issues: Vec<Issue>,
    pub cancelled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum JobState {
    Running,
    Done,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub id: Id,
    pub state: JobState,
    pub completed: u64,
    pub total: Option<u64>,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExportScope {
    All,
    Selected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExportOutput {
    Directory,
    Zip,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExportMode {
    Merge,
    Replace,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportRequest {
    pub scope: ExportScope,
    pub target: PathBuf,
    pub output: ExportOutput,
    pub mode: ExportMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExportActionKind {
    Add,
    Overwrite,
    Remove,
    Preserve,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportAction {
    pub kind: ExportActionKind,
    pub path: PathBuf,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportPlan {
    pub portrait_count: u64,
    pub id: Id,
    pub target: PathBuf,
    pub actions: Vec<ExportAction>,
    pub requires_confirmation: bool,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DestinationOrigin {
    Steam,
    Manual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DestinationState {
    Existing,
    MissingPortraits,
    Uninitialized,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Destination {
    pub id: Id,
    pub game: Game,
    pub name: String,
    pub path: PathBuf,
    pub origin: DestinationOrigin,
    pub state: DestinationState,
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryReport {
    pub destinations: Vec<Destination>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppError {
    pub code: String,
    pub message: String,
    pub recoverable: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportReport {
    pub added: u64,
    pub overwritten: u64,
    pub removed: u64,
    pub preserved: u64,
    pub issues: Vec<Issue>,
}
