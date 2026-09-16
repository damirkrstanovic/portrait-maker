import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type { CatalogFacets, CatalogPage, Destination, DiscoveryReport, ExportReport, ExportPlan, ExportRequest, Game, ImportReport, ImportRequest, Job, MetadataPatch, Page, Query, SelectionAction, SelectionTarget } from "./contracts";

export type LibraryInfo = {
  id: string;
  name: string;
  path: string;
};

export interface LibraryApi {
  startBackup?(path: string, confirmedPath: string | null): Promise<Job>;
  startRestore?(archive: string, path: string): Promise<Job>;
  getRestoreResult?(id: string): Promise<LibraryInfo | null>;
  chooseBackupFolder?(): Promise<string | null>;
  chooseBackupArchive?(): Promise<string | null>;
  chooseCreateLocation(): Promise<string | null>;
  chooseOpenLocation(): Promise<string | null>;
  createLibrary(path: string): Promise<LibraryInfo>;
  openLibrary(path: string): Promise<LibraryInfo>;
  closeLibrary(): Promise<void>;
  getLastLibraryPath(): Promise<string | null>;
  chooseImportFolder(): Promise<string | null>;
  chooseImportArchive(): Promise<string | null>;
  startImport(request: ImportRequest): Promise<Job>;
  getJob(id: string): Promise<Job | null>;
  cancelJob(id: string): Promise<void>;
  getImportReport(id: string): Promise<ImportReport | null>;
  queryCatalog(query: Query, page: Page): Promise<CatalogPage>;
  getCatalogFacets?(): Promise<CatalogFacets>;
  getDisplayRole?(): Promise<"small" | "medium" | "large">;
  setDisplayRole?(role: "small" | "medium" | "large"): Promise<void>;
  editMetadata?(ids: string[], patch: MetadataPatch): Promise<void>;
  renameSource?(id: string, name: string): Promise<void>;
  changeSelection?(target: SelectionTarget, action: SelectionAction): Promise<number>;
  trashPortraits?(ids: string[]): Promise<void>;
  restorePortraits?(ids: string[]): Promise<void>;
  startPurge?(ids: string[]): Promise<Job>;
  discoverDestinations?(): Promise<DiscoveryReport>;
  getSavedDestinations?(): Promise<Destination[]>;
  saveDestination?(destination: Destination): Promise<void>;
  chooseDestinationFolder?(): Promise<string | null>;
  chooseCompatibilityPrefix?(): Promise<string | null>;
  validateDestination?(path: string, game: Game): Promise<Destination>;
  planExport?(request: ExportRequest): Promise<ExportPlan>;
  designateExportTarget?(path: string): Promise<string>;
  validateExportPlan?(id: string): Promise<ExportPlan>;
  discardExportPlan?(id: string): Promise<void>;
  getExportReport?(id: string): Promise<ExportReport | null>;
  applyExport?(id: string, confirmed: boolean): Promise<Job>;
  resolvePrefix?(path: string, game: Game): Promise<Destination[]>;
}

async function chooseDirectory(title: string): Promise<string | null> {
  const selection = await open({ directory: true, multiple: false, title });
  return typeof selection === "string" ? selection : null;
}

export const desktopApi: LibraryApi = {
  chooseBackupFolder: () => chooseDirectory("Choose a backup folder"),
  startBackup: (path, confirmedPath) => invoke<Job>("start_backup", { path, confirmedPath }),
  startRestore: (archive, path) => invoke<Job>("start_restore", { archive, path }),
  getRestoreResult: (id) => invoke<LibraryInfo | null>("get_restore_result", { id }),
  chooseBackupArchive: async () => { const selected = await open({ directory: false, multiple: false, title: "Choose a library backup", filters: [{name: "Library backup", extensions: ["zip"]}] }); return typeof selected === "string" ? selected : null; },
  chooseCreateLocation: () => chooseDirectory("Choose an empty library folder"),
  chooseOpenLocation: () => chooseDirectory("Open a portrait library"),
  createLibrary: (path) => invoke<LibraryInfo>("create_library", { path }),
  openLibrary: (path) => invoke<LibraryInfo>("open_library", { path }),
  closeLibrary: () => invoke<void>("close_library"),
  getLastLibraryPath: () => invoke<string | null>("get_last_library_path"),
  chooseImportFolder: () => chooseDirectory("Choose a portrait folder"),
  chooseImportArchive: async () => {
    const selection = await open({
      directory: false,
      multiple: false,
      title: "Choose a portrait archive",
      filters: [{ name: "Archives", extensions: ["zip", "rar", "7z"] }],
    });
    return typeof selection === "string" ? selection : null;
  },
  startImport: (request) => invoke<Job>("start_import", { request }),
  getJob: (id) => invoke<Job | null>("get_job", { id }),
  cancelJob: (id) => invoke<void>("cancel_job", { id }),
  getImportReport: (id) => invoke<ImportReport | null>("get_import_report", { id }),
  queryCatalog: (query, page) => invoke<CatalogPage>("query_catalog", { query, page }),
  getCatalogFacets: () => invoke<CatalogFacets>("get_catalog_facets"),
  getDisplayRole: () => invoke<"small" | "medium" | "large">("get_display_role"),
  setDisplayRole: (role) => invoke<void>("set_display_role", { role }),
  editMetadata: (ids, patch) => invoke<void>("edit_metadata", { ids, patch }),
  renameSource: (id, name) => invoke<void>("rename_source", { id, name }),
  changeSelection: (target, action) => invoke<number>("change_selection", { target, action }),
  trashPortraits: (ids) => invoke<void>("trash_portraits", { ids }),
  restorePortraits: (ids) => invoke<void>("restore_portraits", { ids }),
  startPurge: (ids) => invoke<Job>("start_purge", { ids }),
  discoverDestinations: () => invoke<DiscoveryReport>("discover_destinations"),
  getSavedDestinations: () => invoke<Destination[]>("get_saved_destinations"),
  saveDestination: (destination) => invoke<void>("save_destination", { destination }),
  chooseDestinationFolder: () => chooseDirectory("Choose a game Portraits folder"),
  chooseCompatibilityPrefix: () => chooseDirectory("Choose a Wine or Proton prefix"),
  validateDestination: (path, game) => invoke<Destination>("validate_destination", { path, game }),
  applyExport: (id, confirmed) => invoke<Job>("apply_export", { id, confirmed }),
  getExportReport: (id) => invoke<ExportReport | null>("get_export_report", { id }),
  planExport: (request) => invoke<ExportPlan>("plan_export", { request }),
  designateExportTarget: (path) => invoke<string>("designate_export_target", { path }),
  validateExportPlan: (id) => invoke<ExportPlan>("validate_export_plan", { id }),
  discardExportPlan: (id) => invoke<void>("discard_export_plan", { id }),
  resolvePrefix: (path, game) => invoke<Destination[]>("resolve_prefix", { path, game }),
};
