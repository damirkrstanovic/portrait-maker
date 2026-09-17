export type Id = string;
export type Role = "small" | "medium" | "large";
export type Game = "kingmaker" | "wotr";
export type Label = { category: string; value: string };
export type Query = {
  text: string;
  sourceIds: Id[];
  labels: Label[];
  selectedOnly: boolean;
  trash: boolean;
};
export type SelectionTarget = { ids: Id[] } | { matching: Query };
export type SelectionAction = "add" | "remove" | "clear";
export type MetadataPatch = {
  name?: string;
  description?: string | null;
  addLabels: Label[];
  removeLabels: Label[];
};
export type Page = { offset: number; limit: number };
export type Portrait = {
  id: Id;
  sourceId: Id;
  name: string;
  sourceName: string;
  originalFolder: string;
  description: string | null;
  labels: Label[];
  selected: boolean;
  trashedAt: string | null;
};
export type CatalogPage = { items: Portrait[]; total: number; revision: number };
export type SourceFacet = { id: Id; name: string; count: number };
export type LabelFacet = { category: string; value: string; displayValue: string; count: number };
export type CatalogFacets = { sources: SourceFacet[]; labels: LabelFacet[] };
export type Issue = {
  path: string;
  code: string;
  message: string;
  severity: "warning" | "error";
};
export type ImportRequest = {
  path: string;
  sourceName: string;
  kind: "folder" | "archive" | "game";
  resize: boolean;
  duplicatePolicy?: "keep" | "skip";
};
export type ImportReport = {
  sourceId: Id | null;
  imported: number;
  skipped: number;
  issues: Issue[];
  cancelled: boolean;
};
export type Job = {
  id: Id;
  state: "running" | "done" | "cancelled" | "failed";
  completed: number;
  total: number | null;
  message: string;
};
export type ExportRequest = {
  scope: "all" | "selected";
  target: string;
  output: "directory" | "zip";
  mode: "merge" | "replace";
};
export type ExportAction = {
  kind: "add" | "overwrite" | "remove" | "preserve";
  path: string;
  reason: string;
};
export type ExportPlan = {
  portraitCount: number;
  id: Id;
  target: string;
  actions: ExportAction[];
  requiresConfirmation: boolean;
  warnings: string[];
};
export type Destination = {
  id: Id;
  game: Game;
  name: string;
  path: string;
  origin: "steam" | "manual";
  state: "existing" | "missingPortraits" | "uninitialized";
  evidence: string[];
};
export type DiscoveryReport = { destinations: Destination[]; warnings: string[] };
export type AppError = { code: string; message: string; recoverable: boolean };

export type ExportReport = { added: number; overwritten: number; removed: number; preserved: number; issues: Issue[] };

export type DuplicateMember = { portrait: Portrait; sourceNames: string[] };
export type DuplicateGroup = { fingerprint: string; members: DuplicateMember[]; nameConflict: boolean; descriptionConflict: boolean };
export type DuplicateScanReport = { groups: DuplicateGroup[]; issues: Issue[] };
export type ImportDuplicateMatch = { folder: string; name: string; matchingPortraitId: string | null; matchingName: string | null; duplicateOfInBatch: string | null };
export type ImportDuplicateReport = { matches: ImportDuplicateMatch[]; issues: Issue[] };
export type DuplicateConsolidation = { keepId: string; removeIds: string[] };
export type DuplicateConsolidationReport = { trashed: number };
