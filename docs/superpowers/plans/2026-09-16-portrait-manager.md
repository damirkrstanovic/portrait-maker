# Pathfinder Portrait Manager Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Inline execution is the default unless the user chooses delegation.

**Goal:** Build an offline desktop application for importing, searching, curating, and exporting thousands of game-ready Pathfinder portraits.

**Architecture:** A Tauri desktop shell calls a Rust core that owns SQLite, image files, archive processing, and game discovery. React renders a virtualized catalog and optional preview. The core is a separate workspace crate so persistence and destructive-operation tests run without a GUI.

**Tech Stack:** Tauri 2, React/TypeScript, Vite, SQLite FTS5 through bundled `rusqlite`, Rust `image` PNG decoder, `compress-tools`/libarchive for archive reading, Rust `zip` for writing, TanStack Virtual, Vitest/Testing Library, and Playwright for browser UI tests.

**Spec:** [Approved design](../specs/2026-09-16-portrait-manager-design.md).

## Global Constraints

- “Linux is the first directly tested platform; Windows and macOS are supported architecture and packaging targets, with native verification required before claiming release readiness.”
- “All managed asset paths are relative to the library root.”
- “Use a single-writer library lock.”
- “Each imported portrait set gets its own entry, including repeated imports.”
- “No hashes, visual matching, or automatic source merging are used to deduplicate portraits.”
- “Original input is never modified.”
- “Search and filter changes preserve selection.”
- “This is lexical search, not semantic image search.”
- “Trashed portraits are always excluded.” (Game-ready exports.)
- “Require an explicit user confirmation for that exact plan on every replacement.”
- “V1 does not migrate saved-game references.”
- “V1 excludes cloud sync, automatic duplicate detection, image editing/cropping, individual-image import, VLM execution, image generation, and automatic GOG/Heroic/Lutris discovery.”
- Never test export or deletion against the user's real game directories. Real discovery and image-header inspection are read-only; use disposable directories for mutation tests.

## Delivery and execution rules

Deliver three sequential, runnable milestones: A — local library/import/search browser (tasks 1–6); B — editing/selection/trash (tasks 7–8); C — discovery/export/backup and release validation (tasks 9–13). These share a catalog and command contract, so they stay in one ordered plan rather than independent subsystem projects.

For each task, write its meaningful regression tests first, run them to observe a relevant failure, implement the behavior, and rerun the affected checks. Record evidence in `docs/development/verification.md`. Scaffolding and documentation do not need artificial tests. Commit each completed task when an actual writable Git checkout is available. This workspace currently contains an empty read-only `.git` directory and is not a usable repository: do not delete or replace it to force commits.

Resolve dependency versions once when scaffolding, check declared compiler/Node requirements, and lock them in `Cargo.lock` and `package-lock.json`. Keep native archive packaging in the initial feasibility check, not as a surprise after UI completion. Do not report Windows/macOS verified based on Linux tests or fixture tests alone.

## Evidence and compatibility decisions

Read-only checks on 2026-09-16 found Rust/Cargo 1.95.0, Node 26.8.2, npm 12.0.2, GTK 3.24.52, WebKitGTK 2.52.6, and libarchive 3.8.9 available locally. No application scaffold exists.

Local Steam lists both games. Native Kingmaker and Proton prefixes for both games have portrait directories. Sample PNG headers in all three destinations agree:

| Role | Export filename | Width | Height |
| --- | --- | --- | --- |
| Small | `Small.png` | 185 | 242 |
| Medium | `Medium.png` | 330 | 432 |
| Large | `Fulllength.png` | 692 | 1024 |

These are observed existing files, not proof that every file has been accepted in-game. Use these as canonical resize/export-warning dimensions, permit nonstandard dimensions by user-approved warning policy, and perform game acceptance testing before release; do not copy private portrait images into repository fixtures. Generate plain-color test PNGs.

Discovery compatibility table:

| Game/platform | Candidate user-data root, with `/Portraits` appended |
| --- | --- |
| Kingmaker, Windows | Known Folder LocalAppDataLow + `Owlcat Games/Pathfinder Kingmaker` |
| Kingmaker, Linux native | `~/.config/unity3d/Owlcat Games/Pathfinder Kingmaker` (also probe XDG config location) |
| Kingmaker, macOS | `~/Library/Application Support/Owlcat Games/Pathfinder Kingmaker` and existing Unity aliases `unity.Owlcat Games.Pathfinder Kingmaker`, `unity3d/Owlcat Games/Pathfinder Kingmaker` |
| WotR, Windows | Known Folder LocalAppDataLow + `Owlcat Games/Pathfinder Wrath Of The Righteous` |
| Either, Proton/manual Wine prefix | `<prefix>/drive_c/users/<user>/AppData/LocalLow/` + Windows game suffix |

Kingmaker Steam ID is `640820`; WotR is `1184370`. Do not invent a Linux-native WotR path. For WotR on macOS retain manual destination/prefix support; automatic native paths need evidence of a supported native build and its data directory. Multiple existing candidates remain visible, with no inferred winner.

Primary references:

- [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) — platform build requirements.
- [Tauri AppImage documentation](https://tauri.app/distribute/appimage/) — choose an older compatible build base for Linux portability.
- [libarchive format support](https://github.com/libarchive/libarchive/wiki/LibarchiveFormats) — ZIP, original RAR/RAR5, and 7z reading; test actual codecs rather than assuming every archive variant works.
- [libarchive project](https://www.libarchive.org/) — cross-platform library and BSD licensing. Bundle required codecs/libraries and their licenses; do not rely on an installed extractor.
- [compress-tools API](https://docs.rs/compress-tools/latest/compress_tools/) — libarchive binding; streaming entry interface available.
- [Valve Proton FAQ](https://github.com/ValveSoftware/Proton/wiki/Proton-FAQ) — per-game prefix convention.
- [Owlcat Kingmaker technical support](https://kingmaker.owlcat.games/content/bug-report-technical) — native platform user-data locations. Portraits as a sibling of Saved Games is a resolver rule, with Linux corroborated locally.
- [Kingmaker Steam page](https://store.steampowered.com/app/640820/) and [WotR Steam page](https://store.steampowered.com/app/1184370/) — game identities.

## File structure

```text
Cargo.toml                             Rust workspace: core + desktop shell
crates/portrait-core/src/
  lib.rs, error.rs, types.rs            exported domain API and errors
  library/{mod,lock,migrations}.rs     library lifecycle and schema
  import/{mod,archive,scan,validate}.rs safe ingestion
  metadata/{mod,infer}.rs              rules and user overrides
  catalog/{mod,fts,query}.rs            search and indexing
  selection.rs, trash.rs               curation state
  thumbnails.rs                       bounded thumbnail generation
  discovery/{mod,steam,platform}.rs    destination candidates
  export/{mod,plan,apply,journal}.rs    export previews and recovery
  backup.rs, recovery.rs               snapshots and startup recovery
crates/portrait-core/migrations/001_initial.sql
crates/portrait-core/resources/labels.json
crates/portrait-core/tests/             integration tests by feature
crates/portrait-core/tests/support/mod.rs
crates/portrait-core/tests/fixtures/    generated or licensed archive fixtures
src-tauri/src/{lib,main,commands,state,settings,assets}.rs
src-tauri/{Cargo.toml,build.rs,tauri.conf.json,capabilities/default.json}
src/app/{App,LibraryChooser,AppLayout}.tsx
src/features/{catalog,import,metadata,selection,trash,destinations,export,backup}/
src/lib/{api,contracts,preferences}.ts
src/styles/{tokens,app}.css
src/test/{setup,mockApi}.ts
tests/e2e/{library,curation,export}.spec.ts
scripts/{seed-benchmark,check-bundle}.mjs
.github/workflows/check.yml
docs/development/{compatibility,verification,release-checklist}.md
README.md, THIRD_PARTY_NOTICES.md
```

Keep feature components beside their hooks and tests. Do not put database, archive, or discovery logic into Tauri command functions. UI styles should use a restrained desktop layout: readable typography, neutral surfaces, clear selection indicators, and image-focused cards. Use the frontend-design skill during UI implementation.

## Shared contracts

Define domain types in `types.rs`, serialize camelCase to matching `src/lib/contracts.ts`, and test round-trip fixtures to prevent drift. UUIDs are strings at IPC boundaries. No arbitrary SQL or general filesystem APIs are exposed to the webview.

```ts
type Id = string;
type Role = 'small' | 'medium' | 'large';
type Game = 'kingmaker' | 'wotr';
type Label = { category: string; value: string };
type Query = {
  text: string; sourceIds: Id[]; labels: Label[];
  selectedOnly: boolean; trash: boolean;
};
type Page = { offset: number; limit: number }; // limit <= 200
type Portrait = {
  id: Id; sourceId: Id; name: string; sourceName: string;
  originalFolder: string; description: string | null;
  labels: Label[]; selected: boolean; trashedAt: string | null;
};
type CatalogPage = { items: Portrait[]; total: number; revision: number };
type Issue = { path: string; code: string; message: string };
type ImportRequest = { path: string; sourceName: string; kind: 'folder'|'archive'|'game'; resize: boolean };
type ImportReport = { sourceId: Id | null; imported: number; skipped: number; issues: Issue[]; cancelled: boolean };
type Job = { id: Id; state: 'running'|'done'|'cancelled'|'failed'; completed: number; total: number | null; message: string };
type ExportRequest = { scope: 'all'|'selected'; target: string; output: 'directory'|'zip'; mode: 'merge'|'replace' };
type ExportAction = { kind: 'add'|'overwrite'|'remove'|'preserve'; path: string; reason: string };
type ExportPlan = { id: Id; target: string; actions: ExportAction[]; requiresConfirmation: boolean; warnings: string[] };
type Destination = { id: Id; game: Game; name: string; path: string; origin: 'steam'|'manual'; state: 'existing'|'missingPortraits'|'uninitialized'; evidence: string[] };
type AppError = { code: string; message: string; recoverable: boolean };
```

Core `Library` owns root, lock, SQLite connection, and catalog revision. All exported core functions use `Result<T, CoreError>`. Blocking work runs on a worker; mutations serialize through the library owner. Long jobs support `JobContext` with cancellation flag and progress callback. `Settings` and saved export manifests are owned by the desktop shell, outside the library.

Rust mirrors the contract structs using snake_case fields, `Uuid` for IDs, `PathBuf` for input paths, and `u64` for counts/revisions. String unions become enums: `Role::{Small,Medium,Large}`, `Game::{Kingmaker,Wotr}`, `ImportKind::{Folder,Archive,Game}`, `ExportScope::{All,Selected}`, `ExportOutput::{Directory,Zip}`, and `ExportMode::{Merge,Replace}`. Derive serialization and debug traits; error `code(&self) -> &str` returns stable uppercase codes. SQL fixtures use `Library::connection(&self) -> &rusqlite::Connection`, available to trusted Rust code only, never IPC. `Page` uses `u32` fields; boundary deserialization rejects negative numbers.

## Task 1 — Core workspace, library lifecycle, desktop entry

**Files:** root/frontend manifests and lockfiles; `crates/portrait-core/{Cargo.toml,src/lib.rs,src/error.rs,src/types.rs,src/library/*,migrations/001_initial.sql,tests/library.rs}`; Tauri shell files; `src/app/{App,LibraryChooser}.tsx`; `src/lib/{api,contracts}.ts`; `README.md`.

**Interfaces:** `Library::create(root: &Path) -> Result<Library>`; `Library::open(root: &Path) -> Result<Library>`; `Library::id(&self) -> Uuid`. Commands `create_library(path)` and `open_library(path)` return the library UUID/name; `close_library()` releases resources.

- [x] Scaffold the workspace and Vite React shell, add Tauri 2 dialog integration, and restrict capabilities to named commands/dialogs. Configure `npm run dev`, `npm run build`, `npm run test`, and `npm run tauri` scripts. Verify the blank desktop opens before adding features.
- [x] Add `library.rs` regression test and run `cargo test -p portrait-core --test library`; expect missing library API initially.

```rust
#[test]
fn moving_a_closed_library_preserves_identity() {
    let temp = tempfile::tempdir().unwrap();
    let old = temp.path().join("old");
    let new = temp.path().join("new");
    let lib = Library::create(&old).unwrap();
    let id = lib.id();
    assert!(Library::open(&old).is_err()); // second writer
    drop(lib);
    std::fs::rename(&old, &new).unwrap();
    assert_eq!(Library::open(&new).unwrap().id(), id);
}
```

- [x] Implement the lifecycle. Use OS file locking rather than existence-only lockfiles; create manifest with exclusive create; refuse nonempty destinations and unsupported format versions. Enable foreign keys, a busy timeout, and bundled SQLite FTS5. Start with DELETE journal mode to simplify closed-library copying. Apply migrations transactionally with an online SQLite backup before changing an existing schema.
- [x] Create tables for sources, portraits, assets, labels, portrait labels/provenance, suppressed inferred labels, selection, search documents, and operation state. Use foreign keys and asset-role uniqueness. Store only relative managed paths, UTC timestamps, and `PRAGMA user_version`.
- [x] Wire create/open/close UI, remember last library in platform settings, and show actionable lock/version errors. Run core tests, TypeScript checking, and a desktop open/close smoke test; record results.

## Task 2 — Safe archive reader and distribution feasibility

**Files:** `crates/portrait-core/src/import/{mod,archive}.rs`; `tests/{archive.rs,fixtures/README.md}` and fixtures; `docs/development/compatibility.md`; `THIRD_PARTY_NOTICES.md`; platform build configuration.

**Interfaces:** `extract_archive(input: &Path, staging: &Path, limits: &ExtractionLimits, job: &JobContext) -> Result<()>`; `validate_entry_path(name: &str) -> Result<PathBuf>`. `ExtractionLimits` contains entry count, total bytes, per-file bytes. `JobContext::default()` runs without cancellation/progress listeners.

- [x] Write path tests, then run `cargo test -p portrait-core --test archive` expecting missing validation/extraction APIs.

```rust
#[test]
fn archive_paths_cannot_escape_on_any_platform() {
    for name in ["../x", "/tmp/x", "C:\\x", "a/../../x", "a\\..\\..\\x", "//server/share"] {
        assert!(validate_entry_path(name).is_err(), "{name}");
    }
    assert_eq!(validate_entry_path("pack/elf/Small.png").unwrap(),
               std::path::PathBuf::from("pack/elf/Small.png"));
}
```

- [x] Implement streamed extraction with `compress-tools` and libarchive; allow only ZIP/RAR/7z input signatures/formats. Normalize separators before checking path components; reject reserved Windows names, drive prefixes, NULs, case-fold collisions, links, and special files. Write only app-created regular files under fresh staging using exclusive creation. Do not invoke bulk extraction APIs that create archive-controlled links.
- [x] Set defaults to 100,000 entries, 20 GiB total expanded data, and 128 MiB per file; count actual streamed bytes, not only header claims. Surface limit-specific errors. Reject encrypted/multipart input and propagate truncated/unsupported codec errors. If a wrapper cannot expose required link/encryption/format information, add a narrow audited libarchive adapter rather than weakening checks.
- [x] Add small ZIP/7z synthetic fixtures and original-RAR/RAR5 fixtures with provenance and redistribution rights documented in `fixtures/README.md`; use upstream libarchive test assets if appropriate. Exercise successful extraction, truncated files, links, duplicate normalized names, limits, and cancellation. Do not put downloaded portrait art into fixtures.
- [x] Verify libarchive linkage and required codecs in Linux packaging and Windows/macOS build configurations (vcpkg/CMake or bundled libraries). Include library licenses. Run fixture tests without `unrar`, `7z`, or `bsdtar` in PATH. A format is not marked supported until its real archive fixture passes.

## Task 3 — Game-ready directory import and label inference

**Files:** `src/import/{scan,validate,mod}.rs`, `src/metadata/{mod,infer}.rs`, `resources/labels.json`, `src/recovery.rs`, `tests/{import,inference}.rs`, `tests/support/mod.rs` under core; desktop job commands; `src/features/import/{ImportDialog,ImportProgress}.tsx`.

**Interfaces:** `import_portraits(lib: &mut Library, request: ImportRequest, job: &JobContext) -> Result<ImportReport>`; `infer_labels(path: &str) -> Vec<Label>`; `validate_portrait(dir: &Path) -> Result<Vec<AssetSpec>>`. `AssetSpec` holds role, original path, width, height. Recovery runs before normal library commands.

- [x] Add reusable test helper `write_portrait(root: &Path, folder: &str) -> PathBuf` in `tests/support/mod.rs`: create folder, generate three PNGs with `image::RgbaImage::from_pixel(width, height, image::Rgba([30,40,50,255])).save(path)`. Return created folder. Use dimensions from the compatibility table.
- [x] Write tests using that helper, then run `cargo test -p portrait-core --test import --test inference` to observe failure.

```rust
#[test]
fn repeated_imports_remain_separate_entries() {
    let temp = tempfile::tempdir().unwrap();
    let input = temp.path().join("input");
    support::write_portrait(&input, "female_elf_archer");
    let mut lib = Library::create(&temp.path().join("library")).unwrap();
    for _ in 0..2 {
        let request = ImportRequest { path: input.clone(), source_name: "Pack".into(), kind: ImportKind::Folder, resize: false };
        assert_eq!(import_portraits(&mut lib, request, &JobContext::default()).unwrap().imported, 1);
    }
    assert_eq!(lib.connection().query_row("SELECT count(*) FROM portraits", [], |r| r.get::<_, i64>(0)).unwrap(), 2);
}
```

- [x] Implement recursive directory scanning without following external symlinks; recognize names case-insensitively; decode PNGs under a 16-million-pixel and 128-MiB allocation cap and compare role dimensions for warnings. Default import preserves nonstandard images; optional explicit resize fits the whole image proportionally onto the canonical opaque-black canvas using Lanczos3 and writes new managed PNGs. Do not crop or modify originals. Record actual dimensions and warning counts. Normalize copied filenames. Reject incomplete, ambiguous, non-PNG, or corrupted sets with per-folder issues; continue valid sets.
- [x] Infer labels from folder/parent tokens using a versioned JSON vocabulary. Token boundaries must distinguish `female` from `male`, and phrases from incidental substrings. Seed woman/man, common Pathfinder races, and mage/martial/archer aliases. Keep unknown attributes empty and conflicts visible.
- [x] Stage each portrait set, create operation intent, rename it into its UUID folder, then commit source/portrait/assets/labels in one transaction. Recovery reconciles intent records and removes orphan staging/final folders if no portrait committed. Cancellation keeps committed portraits, cleans unfinished copies, and returns accurate counts; remove empty source records.
- [x] Add tests for nested mixed-case folders, partial valid packs, deletion of originals, copy failure, cancellation, restart recovery, and each supported archive flowing through import. Wire import dialog/progress with an unchecked resize option and report per-folder warnings/errors without freezing the window. Test p1.zip importing 45 nonstandard sets unchanged by default and all 45 normalized when resize is opted into; heroes.rar has 377 canonical sets. Fixtures are private symlinks, never commit or modify them.

## Task 4 — Catalog queries and FTS5

**Files:** core `src/catalog/{mod,fts,query}.rs`, `tests/search.rs`; frontend `src/lib/contracts.ts` and `src/features/catalog/useCatalog.ts`; desktop catalog commands.

**Interfaces:** `query_catalog(lib: &Library, query: &Query, page: Page) -> Result<CatalogPage>`; `refresh_search_document(tx: &Transaction, id: Uuid) -> Result<()>`; `compile_fts(text: &str) -> Option<String>`; `Query::default()` returns unfiltered active entries. Command `query_catalog(query, page)` returns items/count/revision.

- [x] Add tests and run `cargo test -p portrait-core --test search` expecting missing query behavior.

```rust
#[test]
fn user_text_is_not_raw_match_syntax() {
    assert_eq!(compile_fts(" elf   archer ").as_deref(), Some("\"elf\" AND \"archer\""));
    assert_eq!(compile_fts("\" OR * :").as_deref(), Some("\"OR\""));
    assert_eq!(compile_fts("   "), None);
}
```

- [x] Create FTS5 using `unicode61 remove_diacritics 2`; construct quoted terms from Unicode word tokens and parameterize MATCH. Use `None` when no searchable terms remain. Rank text matches by bm25 and then name/UUID; otherwise sort name/UUID. Reject negative/out-of-range pagination and cap page size at 200.
- [x] Build one shared matching-ID SQL builder for browsing and bulk operations: OR labels within category, AND category groups, OR sources, AND free text; selected-only and trash predicates are explicit. Count and page queries share one read snapshot/revision.
- [x] Update index during import and every metadata/source rename transaction. Add integration fixtures testing combined filters, punctuation, diacritics, descriptions, source rename, index rebuild, stable paging, and trash isolation. Test all user input through bound parameters.
- [x] Frontend hook debounces text, resets page windows on query changes, ignores stale responses using a request generation counter, and never clears selection. Verify out-of-order mocked responses cannot replace current results.

## Task 5 — Thumbnail service and virtualized grid

**Files:** core `src/thumbnails.rs`, `tests/thumbnails.rs`; desktop `src/assets.rs`; frontend `src/features/catalog/{PortraitGrid,PortraitCard,FilterSidebar,SearchBar}.tsx`, colocated tests, `src/styles/{tokens,app}.css`.

**Interfaces:** `thumbnail(lib: &Library, id: Uuid, role: Role, edge: u32) -> Result<PathBuf>`; frontend `assetUrl(id, role, variant: 'thumbnail'|'original') -> string` through a scoped Tauri asset route. IDs resolve inside the open library; do not accept arbitrary caller paths. `PortraitGrid` receives catalog query/page state and emits focus/toggle-selection events.

- [x] Write tests for asset ID validation, unknown portrait errors, cache regeneration, dimensions, and library switching; run `cargo test -p portrait-core --test thumbnails` before implementation.
- [x] Implement PNG-only thumbnail generation with bounded workers (2), disk cache keys by portrait/role/edge/asset metadata, maximum requested edge 1024, and a 256-MiB in-memory image/cache budget. Clear job/cache handles on close. Originals remain unchanged.
- [x] Build responsive virtualized grid rows with TanStack Virtual and a small overscan. Lazy-load only visible/nearby thumbnails; retain at most five 200-record pages. Remember role preference in machine settings. Add source and category filters and clear-filter controls.
- [x] Add Testing Library test with 10,000 mocked records and assert the rendered card count stays below 200 after scrolling, rather than asserting implementation internals. In Playwright, verify changing filters doesn't flash stale rows. Confirm loading/error/empty states and keyboard focus visibility.

## Task 6 — Optional three-size preview and runnable milestone A

**Files:** `src/features/catalog/{PortraitPreview,PreviewImage}.tsx` and tests; `src/app/AppLayout.tsx`; `src/lib/preferences.ts`; `tests/e2e/library.spec.ts`; `docs/development/verification.md`.

**Interfaces:** `PortraitPreview({ portrait: Portrait | null, visible: boolean })`; preview focus is frontend state independent of catalog `selected`. Preference storage saves visibility and grid role. `src/test/mockApi.ts` implements the same typed API as the desktop bridge; mocks are test-only.

- [x] Write UI regression first and run `npm run test -- PortraitPreview`.

```tsx
it('does not select a portrait when opening its preview', async () => {
  const user = userEvent.setup();
  const onFocus = vi.fn();
  const onToggleSelection = vi.fn();
  const portrait = { id:'p', sourceId:'s', name:'Elf', sourceName:'Pack', originalFolder:'elf', description:null, labels:[], selected:false, trashedAt:null };
  render(<PortraitCard portrait={portrait} role="medium" onFocus={onFocus} onToggleSelection={onToggleSelection} />);
  await user.click(screen.getByRole('button', { name: 'Preview Elf' }));
  expect(onFocus).toHaveBeenCalledWith('p');
  expect(onToggleSelection).not.toHaveBeenCalled();
});
```

- [x] Implement optional side pane with all three labeled images, name/source/labels, fitting previews, and a native-resolution view. Support keyboard open/close, navigation, and independent checkbox toggles. Remember visibility and avoid preloading every original.
- [x] Run milestone A desktop smoke test: create/open library, import folder and all archive formats, search/filter thousands of records, change thumbnail role, preview, close/reopen, move closed library, delete original import source, and reopen successfully. Report failures before moving on.

## Task 7 — Metadata editing and persistent selection

**Files:** core `src/metadata/mod.rs`, `src/selection.rs`, `tests/{metadata,selection}.rs`; frontend `src/features/metadata/MetadataEditor.tsx`, `src/features/selection/SelectionToolbar.tsx`; desktop commands.

**Interfaces:** `edit_metadata(lib: &mut Library, ids: &[Uuid], patch: MetadataPatch) -> Result<()>`; `rename_source(lib: &mut Library, id: Uuid, name: &str) -> Result<()>`; `change_selection(lib: &mut Library, target: SelectionTarget, action: SelectionAction) -> Result<u64>`. `SelectionTarget = Ids(Vec<Uuid>) | Matching(Query)`; `SelectionAction = Add | Remove | Clear`. `MetadataPatch` has optional name/description and added/removed label lists; multi-ID edits cannot rename.

- [x] Add selection test using generated fixture entries and run `cargo test -p portrait-core --test selection --test metadata`.

```rust
#[test]
fn select_matching_is_not_limited_to_one_page() {
    let temp = tempfile::tempdir().unwrap();
    let mut lib = Library::create(temp.path().join("library").as_path()).unwrap();
    support::seed_catalog(&mut lib, 1500); // helper inserts complete catalog rows with distinct IDs, one source, and empty labels
    let count = change_selection(&mut lib, SelectionTarget::Matching(Query::default()), SelectionAction::Add).unwrap();
    assert_eq!(count, 1500);
    assert_eq!(query_catalog(&lib, &Query::default(), Page { offset: 0, limit: 100 }).unwrap().items.len(), 100);
}
```

- [x] Implement `support::seed_catalog` in shared test support using generated role assets and one transaction; use it in scale tests too. Implement selection with `INSERT ... SELECT`/`DELETE ... WHERE id IN (matching query)` inside a transaction; use SQL grouping, not frontend page IDs. Define returned count as total selected after the operation. Snapshot current filters when action starts.
- [x] Implement metadata transactions and provenance. User removal writes a suppression row; user values win over future inferred/model writes. Validate trimmed nonempty names and label values; support custom category/value pairs. Reindex affected documents in the same transaction and bump revision.
- [x] Test source renaming, descriptions, label suppression through a repeat inference call, selected-only view, remove-matching after changing filters, restart persistence, clear selection, and single-checkbox behavior. Wire bulk label edits, counts, add/remove all matching, and clear controls.

## Task 8 — Trash, restore, permanent deletion

**Files:** core `src/trash.rs`, `src/recovery.rs`, `tests/trash.rs`; frontend `src/features/trash/{TrashView,DeleteConfirmation}.tsx`; `tests/e2e/curation.spec.ts`.

**Interfaces:** `trash_portraits(lib: &mut Library, ids: &[Uuid]) -> Result<()>`; `restore_portraits(...) -> Result<()>`; `purge_portraits(lib: &mut Library, ids: &[Uuid], job: &JobContext) -> Result<u64>`. Purge operates only on IDs still trashed at execution time.

- [x] Test trash and restore semantics first; run `cargo test -p portrait-core --test trash`.

```rust
#[test]
fn trash_removes_selection_and_restore_does_not_reselect() {
    let temp = tempfile::tempdir().unwrap();
    let mut lib = Library::create(&temp.path().join("library")).unwrap();
    support::seed_catalog(&mut lib, 1);
    let id = query_catalog(&lib, &Query::default(), Page { offset: 0, limit: 1 }).unwrap().items[0].id;
    change_selection(&mut lib, SelectionTarget::Ids(vec![id]), SelectionAction::Add).unwrap();
    trash_portraits(&mut lib, &[id]).unwrap();
    assert_eq!(query_catalog(&lib, &Query::default(), Page { offset: 0, limit: 1 }).unwrap().total, 0);
    restore_portraits(&mut lib, &[id]).unwrap();
    assert!(!query_catalog(&lib, &Query::default(), Page { offset: 0, limit: 1 }).unwrap().items[0].selected);
}
```

- [x] Implement soft-delete plus selection removal transactionally. Purge journals IDs and moves files to staging before deleting catalog records; recover by restoring files if DB deletion didn't commit, or finishing cleanup if it did. Never follow symlinks or delete original source paths.
- [x] Add trash search/preview, restore, purge-selected, and empty-trash dialogs showing counts. Test purge failure/restart and repeated import of trashed content. Run milestone B curation smoke test including restart persistence and keyboard controls.

## Task 9 — Steam discovery and manual destinations

**Files:** core `src/discovery/{mod,steam,platform}.rs`, `tests/discovery.rs`, discovery fixtures; desktop settings; `src/features/destinations/{DestinationList,DestinationEditor}.tsx`; compatibility documentation.

**Interfaces:** `discover_destinations(env: &DiscoveryEnvironment) -> Result<Vec<Destination>>`; `resolve_prefix(prefix: &Path, game: Game) -> Result<Vec<Destination>>`; `validate_destination(path: &Path, game: Game) -> Result<Destination>`. `DiscoveryEnvironment` explicitly supplies platform, home, XDG config/data roots, and Steam roots for fixture injection; production adapter obtains them from OS APIs.

- [x] Add pure tests for Windows suffixes, Linux native location, and fixture Steam roots, then run `cargo test -p portrait-core --test discovery`.

```rust
#[test]
fn steam_ids_and_windows_suffixes_are_game_specific() {
    assert_eq!(Game::Kingmaker.steam_id(), "640820");
    assert_eq!(Game::Wotr.steam_id(), "1184370");
    assert_eq!(Game::Kingmaker.windows_suffix(), "Owlcat Games/Pathfinder Kingmaker");
    assert_eq!(Game::Wotr.windows_suffix(), "Owlcat Games/Pathfinder Wrath Of The Righteous");
}
```

- [x] Parse Valve KeyValues with an appropriate maintained parser; do not regex-parse braces/quoted paths. Read both current and legacy libraryfolders forms and relevant appmanifest files. Linux roots include `~/.local/share/Steam`, `~/.steam/steam`, and `~/.var/app/com.valvesoftware.Steam/.local/share/Steam`, plus relevant XDG locations. Windows reads Steam registry location; macOS probes `~/Library/Application Support/Steam`. Canonicalize and deduplicate equivalent candidates.
- [x] Apply compatibility table; enumerate real prefix user directories except public/default template users. Use Windows Known Folder APIs for LocalAppDataLow. Probe supported macOS aliases only as candidates with existence evidence. Explain uninitialized prefix versus missing Portraits child. Never create directories during discovery.
- [x] Cover multiple libraries, missing manifests, escaped spaces/backslashes, native plus Proton Kingmaker, Flatpak, nonstandard prefix users, permissions, legacy path aliases, and manual override in fixtures. Save destination names/paths in local settings with revalidation on use.
- [x] Wire discovery result picker, manual folder/prefix browsing, refresh, and import-existing through the normal import service. Run real local discovery read-only; it should show both native and Proton Kingmaker candidates plus WotR, not silently pick one.

## Task 10 — Export planning and confirmation

**Files:** core `src/export/{mod,plan}.rs`, `tests/export_plan.rs`; desktop export commands; frontend `src/features/export/{ExportDialog,ExportPreview}.tsx` and tests.

**Interfaces:** `plan_export(lib: &Library, request: ExportRequest, history: &ExportHistory) -> Result<ExportPlan>`; plans are stored server-side with frozen portrait IDs, revision, destination identity/state fingerprint, actions, and one-use plan ID. `Library::export_plan(id: Uuid) -> Result<StoredExportPlan>` retrieves a plan from its interior-mutable plan registry; this registry is ephemeral and cleared on library close. `StoredExportPlan` contains the public `ExportPlan`, `ExportRequest`, frozen `Vec<Uuid>`, catalog revision, and destination fingerprint. `ExportHistory` contains destination/library IDs and previously exported path/file digests and implements `Default` as empty history. Digests are for overwrite detection only, never library deduplication.

- [x] Write tests for empty selected export, preserve/remove classification, and overlap rejection; run `cargo test -p portrait-core --test export_plan`.

```rust
#[test]
fn empty_selection_never_means_export_everything() {
    let temp = tempfile::tempdir().unwrap();
    let lib = Library::create(&temp.path().join("library")).unwrap();
    let request = ExportRequest { scope: ExportScope::Selected, target: temp.path().join("out"), output: ExportOutput::Directory, mode: ExportMode::Merge };
    assert_eq!(plan_export(&lib, request, &ExportHistory::default()).unwrap_err().code(), "EMPTY_SELECTION");
}
```

- [x] Generate stable folder names `pm-<library UUID>-<portrait UUID>`. Snapshot all active or selected IDs. Resolve destination symlinks before overlap checks; reject library ancestors/descendants, filesystem roots, and broad home/game installation roots. Directory replacement targets must be a verified portrait root or explicitly designated portrait collection directory.
- [x] Classify target entries: merge preserves unrelated content; replacement removes only recognized portrait set directories outside the desired set. Folders with extra nonportrait content are preserved and flagged, not recursively removed. Missing historical manifest means ownership is unknown; any collision requires confirmation. Show exact changed file paths as well as folder counts.
- [x] Require confirmation on every replacement and on overwrite collisions. UI displays absolute target, frozen count, add/overwrite/remove/preserve lists, existing-save reference warning, and an explicit Confirm replacement button. Cancel does nothing. Browser tests verify no apply call before validation and that editing the target/scope invalidates the preview.

## Task 11 — Recoverable directory/ZIP export

**Files:** core `src/export/{apply,journal}.rs`, `src/recovery.rs`, `tests/export_apply.rs`; desktop manifest storage and jobs; `tests/e2e/export.spec.ts`.

**Interfaces:** `apply_export(lib: &Library, plan: &StoredExportPlan, confirmed: bool, job: &JobContext) -> Result<ExportReport>`; `ExportReport` has added/overwritten/removed/preserved counts and issues. `StoredExportPlan` is the server-only plan defined in task 10. UI sends a plan ID and confirmation, never a caller-created action list.

- [x] Add stale-plan and unconfirmed-replacement tests; run `cargo test -p portrait-core --test export_apply`. Use injected failure points around each journal/rename/commit boundary and temporary destinations.

```rust
#[test]
fn replacement_requires_confirmation() {
    let temp = tempfile::tempdir().unwrap();
    let mut lib = Library::create(&temp.path().join("library")).unwrap();
    support::seed_catalog(&mut lib, 1);
    let request = ExportRequest { scope: ExportScope::All, target: temp.path().join("Portraits"), output: ExportOutput::Directory, mode: ExportMode::Replace };
    let plan = plan_export(&lib, request, &ExportHistory::default()).unwrap();
    let stored = lib.export_plan(plan.id).unwrap();
    assert_eq!(apply_export(&lib, &stored, false, &JobContext::default()).unwrap_err().code(), "CONFIRMATION_REQUIRED");
}
```

- [x] Revalidate catalog and destination fingerprint before writes; changed state yields `STALE_PLAN`, requiring preview again. Lock app operations for the destination, stage files on the destination filesystem, and recheck each destructive target before replacing it. Use byte/digest checks for overwrite detection. Detect external modification; never restore rollback data over a newly changed user file.
- [x] Journal intent before moves; rename old targets into rollback storage; promote staged complete sets; mark commit durably; then delete rollback data. Startup recovery restores precommit operations or finishes committed cleanup. If recovery cannot safely complete, keep backups and report exact paths/actions. Cancellation before commit rolls back; after commit finish cleanup and report completion.
- [x] Write ZIP directly from chosen managed assets, with canonical names and no DB/trash. Enable ZIP64 when needed; stage beside output and finalize after success. Require confirmation to replace an existing archive; never truncate it before the staged archive is complete.
- [x] Test successful merge twice, modified collision, replacement removal only after confirmation, preserved unrelated files, archive round trip, insufficient space/write failures, cancellation, disconnect, and every injected recovery point. Persist manifest history only after a committed export. Exercise UI on disposable directories.

## Task 12 — Consistent backup, restore, and settings

**Files:** core `src/backup.rs`, `tests/backup.rs`; desktop settings/commands; frontend `src/features/backup/BackupDialog.tsx`; README.

**Interfaces:** `backup_library(lib: &Library, target: &Path, job: &JobContext) -> Result<()>`; `restore_library(archive: &Path, root: &Path, job: &JobContext) -> Result<Library>`. The caller acquires the library mutation gate for the entire backup snapshot/archive operation. Existing backup file overwrite uses an explicit confirmed path token at the command boundary.

- [x] Write backup round-trip tests and run `cargo test -p portrait-core --test backup`.

```rust
#[test]
fn restore_preserves_identity_and_refuses_nonempty_target() {
    let temp = tempfile::tempdir().unwrap();
    let lib = Library::create(&temp.path().join("library")).unwrap();
    let zip = temp.path().join("backup.zip");
    backup_library(&lib, &zip, &JobContext::default()).unwrap();
    let restored = restore_library(&zip, &temp.path().join("restored"), &JobContext::default()).unwrap();
    assert_eq!(restored.id(), lib.id());
    assert!(restore_library(&zip, &temp.path().join("restored"), &JobContext::default()).is_err());
}
```

- [x] Use SQLite backup API under the mutation gate, include manifest and active/trashed assets, omit cache/staging/local settings. Write and finalize archive atomically where supported. Restore through safe archive reader into staging, validate manifest/schema, SQLite integrity and foreign keys, relative paths/roles, and every referenced image, then rename to an empty/new target. Refuse newer format versions before opening as a library.
- [x] Add full metadata/source/selection/trash round-trip assertions, missing/corrupt asset failure, malicious backup paths, cancellation, nonempty target refusal, migration backup, and live-write exclusion tests. Restore never combines databases.
- [x] Wire backup/restore as separate actions from game-ready export. Explain closed-library copying for external backup tools in README; document machine-specific destinations, unsupported encrypted/multipart archives, and no cloud sync. Remember preview/grid preferences independently of backup contents.

## Task 13 — Scale, platform builds, and release evidence

**Files:** `scripts/{seed-benchmark,check-bundle}.mjs`; core benchmark fixture integration test; `.github/workflows/check.yml`; `docs/development/{verification,release-checklist}.md`; README; packaging configuration.

**Interfaces:** `npm run check` runs typecheck, frontend tests, and production build; core checks use `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace`. Benchmark fixture uses the same catalog/asset format as normal import.

- [x] Add a reproducible seed command accepting a destination under a temporary directory and a count, default 10,000. Generate synthetic images only, distinct portrait rows, multiple sources, names, labels, descriptions, and trash/selection states. Refuse existing libraries.
- [ ] Measure cold open, first visible grid, representative FTS queries, filter changes, select/remove all matching, and scroll memory. Record machine, dataset, timings, and peak memory. Target warm query p95 below 200 ms and first grid within 2 s on the development machine; investigate misses rather than masking them with smaller fixtures. Assert bounded DOM/page caches and no full-image decoding across all entries.
- [ ] Configure CI for Linux, Windows MSVC, and macOS native builds with locked dependencies and libarchive codecs. Build Linux on the oldest supported WebKitGTK-compatible base, initially Ubuntu 22.04; verify actual dependency compatibility before advertising that floor. Build AppImage/deb, Windows installer, and macOS app artifacts. Signing/notarization is a distribution step requiring credentials, not fabricated in local tests.

```yaml
strategy:
  fail-fast: false
  matrix:
    os: [ubuntu-22.04, windows-latest, macos-latest]
runs-on: ${{ matrix.os }}
```

- [ ] Inspect artifact dependencies using platform tools (`ldd`, `otool -L`, Windows dependency inspection) and run archive fixtures with no external extractor. Include bundled-library notices. Verify frontend runtime has no network dependency.
- [ ] Run Linux desktop workflow on synthetic fixtures end-to-end and read-only Steam discovery on the user's installations. Record actual game acceptance separately; do not export to live game directories automatically just to test. Complete Windows/macOS native smoke checks when runners or machines are available; report unperformed checks explicitly.
- [ ] Self-review requirement coverage and run the completion/review skills before claiming v1 done. Keep the final report specific: implemented workflows, passed commands, artifact locations, and remaining platform verification gaps.

## Coverage review

| Approved requirement | Tasks |
| --- | --- |
| Portable library, relative paths, lock, migrations | 1, 12 |
| ZIP/RAR/7z/folder imports, source ownership, validation | 2, 3 |
| Automatic labels and future model metadata provenance | 3, 7 |
| FTS text plus source/category filters | 4, 7 |
| Thousands of portraits, thumbnail preference, optional preview | 5, 6, 13 |
| Selection across all filtered results and restarts | 7 |
| Trash, restore, purge, no deduplication | 3, 8 |
| Steam/native/Proton discovery and manual other-launcher support | 9 |
| Import existing game portraits as a source | 3, 9 |
| All/selected to folder/game/ZIP; merge and confirmed replacement | 10, 11 |
| Recovery, stale-plan protection, save-reference warning | 1, 3, 8, 10, 11 |
| Full-library backup/restore and external portability | 12 |
| Linux-first and Windows/macOS packaging/verification | 2, 9, 13 |

Plan self-review: all approved sections map to tasks; shared API names and data types are defined above or in the producing task. Research uncertainties are represented as concrete fixture/build/game acceptance checks, not claims of completed validation. No application code has been written by this planning step.
