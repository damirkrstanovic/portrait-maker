# Pathfinder portrait manager — v1 design

Status: approved by the user on 2026-09-16. Implementation in progress; optional import resizing approved on 2026-09-16.

## Purpose and scope

Provide an offline desktop library for curating thousands of game-ready portraits for Pathfinder: Kingmaker and Pathfinder: Wrath of the Righteous. Users import collections, find portraits through text and filters, prune them, build a selection, and export to either game. The library survives game reinstalls and can move between computers independently of game installations.

Use Tauri, a React/TypeScript frontend, a Rust backend, SQLite with FTS5, and ordinary image files in a user-selected library folder. Linux is the first directly tested platform; Windows and macOS are supported architecture and packaging targets, with native verification required before claiming release readiness.

V1 includes folder, ZIP, RAR, and 7z import; automatic editable name-derived labels; browsing and search; trash; selection; game-ready export; library backup/restore; automatic Steam discovery; and manual destinations for other launchers.

V1 excludes cloud sync, automatic duplicate detection, image editing/cropping, individual-image import, VLM execution, image generation, and automatic GOG/Heroic/Lutris discovery. External utilities handle backup transport or synchronization. ChatGPT authentication and image-generation integration are deferred; this design makes no OAuth availability assumption.

## Library ownership and portability

The app opens one library at a time. First launch offers create or open; moving a library only requires opening its new location.

```text
library/
  library.json                 # format version and library UUID
  library.sqlite3             # catalog, labels, sources, selection, trash state
  portraits/<portrait-uuid>/
    Small.png
    Medium.png
    Fulllength.png
  cache/                      # disposable thumbnails
  staging/                    # temporary operations and recovery information
```

All managed asset paths are relative to the library root. Absolute original import paths may be retained as informational history, never as a dependency for loading images. Stable UUIDs and generated ASCII directory names avoid name collisions and filesystem differences. Display names retain Unicode.

Machine-specific settings live in the platform application configuration directory: last library location, saved game destinations, discovery overrides, preview visibility, and grid preference. They are not included in the portable catalog.

Use a single-writer library lock. External tools should copy the library while the app is closed, or copy an app-created backup. Live file copying of SQLite and images is not an atomic backup. V1 does not merge externally conflicted libraries or support concurrent writers across machines/network shares.

Schema versions and migrations are explicit. Before migration, create a recoverable database backup. Refuse newer unsupported formats with a clear message.

## Data model and module boundaries

- `sources`: UUID, editable name, kind (folder/archive/game destination), import timestamp, informational original location.
- `portraits`: UUID, source UUID, editable display name, original relative folder path, optional description and provenance, creation timestamp, nullable trash timestamp.
- `assets`: portrait UUID, size role, relative path, decoded image dimensions, file size. Three required roles per active portrait.
- `labels`: category and normalized value with a user-facing display value. Initial categories include gender, race, class, and freeform tags; labels can be edited and extended.
- `portrait_labels`: portrait/label relationship, origin (`filename`, `user`, or future `model`), producer/version where applicable. Preserve user removals of inferred labels so future enrichment cannot silently reinstate them.
- `selection`: portrait UUIDs for the current library's persisted bulk selection.
- `search_documents` and an FTS5 index: combined searchable name, original folder path, source name, labels, and description. Rebuildable from catalog data.
- Operation records: recoverable state for import, permanent deletion, export, and backup as needed.

Each imported portrait set gets its own entry, including repeated imports. No hashes, visual matching, or automatic source merging are used to deduplicate portraits. A repeated import creates a separately named source by default; users may rename sources.

Rust modules have distinct responsibilities: library/database, import/archive readers, label inference, catalog/search, selection/trash, discovery, export, and backup. Frontend components call narrow typed Tauri commands; filesystem access and validation stay in Rust. Long operations expose progress, cancellation where safe, and structured results.

## Import and validation

1. Choose a directory or ZIP/RAR/7z archive and an editable source name, initially derived from its name.
2. Recursively discover portrait folders, including nested pack directories. A portrait is one directory containing the three required PNG roles. Match expected filenames case-insensitively on import, then normalize managed filenames. Ambiguous duplicate role files are reported as invalid.
3. Decode files and validate required roles and image dimensions. The implementation must verify the exact accepted dimensions for both games against authoritative/game-generated templates before encoding strict checks. Missing or unreadable images are invalid. Complete nonstandard-dimension sets import unchanged with warnings by default. The user may explicitly choose resize during import: fit each image proportionally within its canonical canvas and pad uncovered areas, preserving the whole image. Never resize silently or alter the source archive/folder. Record actual managed dimensions and warn on export if nonstandard images remain. This supersedes strict dimension rejection by user instruction on 2026-09-16.
4. Stage valid sets inside the library, infer labels, and commit each complete set. Catalog entries must never point at partly copied sets. Keep successful sets if another set fails; report imported, skipped, and failed counts with per-folder reasons.
5. Source metadata and source-relative folder names survive removal of the original archive or folder.

Filename inference uses a versioned vocabulary and token/phrase boundaries rather than arbitrary substring matches. Use the portrait folder and meaningful source-relative parent names. Normalize aliases such as female/woman and male/man. Leave unknown attributes unset; retain conflicting recognized values for human correction rather than guessing. Labels are immediately searchable and editable individually or in bulk.

Archive readers must work without requiring users to install command-line extractors. Select and verify a distributable backend for ZIP, RAR, and 7z on all target platforms during implementation planning, including licensing and supported RAR versions. Encrypted and multipart archives are unsupported in v1 and receive explicit errors. Test representative RAR and 7z archives, not only ZIP.

Reject traversal paths, absolute paths, archive links, and entries escaping staging. Bound extraction and decoded image resource use; report limits clearly. Do not follow directory symlinks outside the chosen import root. Cancellation removes uncommitted staging data; committed portraits remain and are counted in the result. Original input is never modified.

## Browser, preview, and curation

Use a virtualized thumbnail grid with lazy loading and bounded image caching for thousands of portraits. Small/Medium/Large chooses the corresponding portrait image role; Large means Fulllength. Remember this preference. Cache scaled thumbnails instead of decoding every full-resolution image while scrolling.

An optional preview pane displays all three images for the focused portrait, with its name, source, description, and editable labels. Focus and bulk selection are separate: opening a preview does not toggle export selection. Keyboard navigation and clearly labeled controls are required.

Show sources, categorized label filters, a free-text search field, result count, and selection count. Provide a selected-only view. Pagination/query windows are implementation details; bulk operations apply to all matching records, not only rendered rows.

Search and filter changes preserve selection. Provide select/unselect one, add all matching, remove all matching, and clear selection. Matching operations use the current search/filter snapshot and operate transactionally. Selection persists across app restarts and excludes trashed portraits.

## Search and metadata extension

Use SQLite FTS5 for free-text search across names, original folder paths, source names, labels, and descriptions. Ordinary user text is safely tokenized/escaped; malformed FTS syntax cannot break the UI. Default multiword search requires all terms, with case-insensitive Unicode-aware tokenization and diacritic normalization. Empty text applies only direct filters. This is lexical search, not semantic image search.

Direct filters combine OR within a category and AND across categories. Multiple selected sources combine with OR; source, text, and label predicates combine with AND. Unlabeled portraits remain accessible without a label filter. Sort deterministically, using relevance for text search and name otherwise, with UUID as tie-breaker.

Maintain FTS documents transactionally after name, source, label, description, or trash changes. Source renaming updates affected search documents. Search always excludes trash except in the dedicated trash view.

Metadata enrichment is independent of indexing. A future VLM can write descriptions and label suggestions through the same metadata service, recording producer, version, and origin. It must preserve user-edited fields and removed inferred labels. No model, embeddings, credentials, or background inference runs in v1.

## Trash

Trash marks portraits deleted in the database while retaining their image files. They disappear from normal search, selection, and exports. A trash view supports preview, restore, permanent deletion of selected entries, and empty trash with explicit confirmation.

Restoring makes portraits active again but does not automatically reselect them. Permanent deletion removes managed files and metadata with recoverable operation state. It never removes imported originals or game destination files. Without deduplication, importing the same content again creates a new active entry even if an older copy is trashed.

## Game discovery and destinations

Treat installations, user-data directories, and portrait destinations as separate concepts. Discovery proposes destinations; it never imports or writes automatically.

Steam discovery must inspect configured Steam libraries, game manifests, platform user-data locations, and Proton prefixes when relevant. On Linux include conventional Steam locations and Flatpak Steam, multiple libraries, and native-versus-Proton possibilities. On Windows and macOS use platform-specific discovery adapters. Show multiple plausible candidates instead of silently selecting one.

Valve documents per-game Proton prefixes at `steamapps/compatdata/<appid>/pfx/`; custom prefix overrides may require manual selection. Do not assume every user directory inside a prefix is named `steamuser`. Native games and compatibility-layer installations require different user-data resolution.

Support manual portrait-directory selection on every platform and compatibility-prefix selection where applicable. These cover GOG, Heroic, Lutris, and other launchers in v1. Save named destinations locally with game identity, path, discovery/manual origin, and last validation result. Revalidate before use.

Distinguish a verified existing destination from a candidate whose portrait subdirectory is absent and a game whose user-data/prefix has not been initialized. Explain missing state and offer manual selection. Create a missing portrait directory only as part of a user-requested export to a validated destination.

Offer explicit import of existing game portraits as a normal source, initially named `Kingmaker existing` or `WotR existing`. Copy assets into the library; do not link to game files. This is a snapshot, not ongoing synchronization.

Exact game identifiers, native platform paths, historical path aliases, and portrait dimensions require a verified compatibility table during implementation planning. Search results contain conflicting spellings and dimensions; these must not be treated as proven constants. Native platform support in the utility does not imply either game has a native build for every platform.

Reference: [Valve Proton FAQ](https://github.com/ValveSoftware/Proton/wiki/Proton-FAQ), inspected 2026-09-16.

## Game-ready export

Choose all active portraits or the persisted selection, then a saved game destination, arbitrary folder, or ZIP archive. An empty selection is an error, never an implicit request to export all. Freeze the chosen IDs for the operation. Trashed portraits are always excluded.

Output one folder per portrait with the three canonical PNG filenames. Use a stable ASCII folder name containing library UUID and portrait UUID, so repeated exports address the same folders and distinct libraries do not collide. Human-readable names remain in the catalog. Game-ready archives contain portrait folders directly, without the database or trash.

Folder/game export has two modes:

- **Merge (default):** add missing portrait folders and update the app's previously exported entries. Preserve unrelated folders and files. Unexpected collisions or locally modified export targets are shown in the preview; overwriting requires explicit validation. Export identity is tracked using a machine-local manifest keyed by library and destination; it is not content deduplication.
- **Replace collection:** destination portrait collection should contain exactly the chosen set after completion. Preview additions, overwrites, removals, affected portrait folder names, and the absolute destination. Require an explicit user confirmation for that exact plan on every replacement. Nothing is deleted before validation. Preserve unrelated nonportrait files/directories and list them as preserved; never recursively clear an arbitrary selected root.

Preview destination state must be rechecked immediately before applying destructive steps. If it changed, regenerate the plan and require fresh validation. Reject destinations overlapping the managed library. Replacement of a parent/root directory is not allowed merely because it was selected in a folder picker.

Stage export data before changing existing files; retain rollback copies until successful completion. Record interrupted operations so the next launch can recover or clearly report partial completion. Cross-folder exports are not assumed to be one atomic filesystem transaction. Insufficient space, permission failures, or disconnects must not result in an unqualified success message.

Existing game saves can refer to original custom portrait folder names. Importing and re-exporting under new stable IDs does not rewrite those saves. Replacement previews must explain that removing existing folders may affect saved characters; merge preserves them. V1 does not migrate saved-game references.

ZIP output is staged and finalized only after success. Replacing an existing ZIP requires normal explicit overwrite confirmation.

## Library backup and restore

Provide a distinct library backup ZIP containing a consistent database snapshot, all managed images including trash, and the format manifest. Omit caches, staging, and machine-local destinations/settings. Pause mutations during snapshot creation so metadata and files agree.

Restore validates archive safety, format compatibility, and referenced assets into a new or empty folder, then offers to open it. It does not merge catalogs or overwrite an existing library. Ordinary game-ready import and full-library restore are separate actions with clear descriptions.

## Verification and completion criteria

Automated tests must cover:

- Folder and ZIP/RAR/7z import with nested sets, mixed filename case, invalid sets, traversal/link rejection, cancellation, and representative archive versions.
- Persistence after deleting the original source, moving the library, and backup/restore including labels, sources, selection, and trash.
- Filename token boundaries, aliases, unknown labels, user edits, and preservation of user overrides.
- FTS updates and safe free text, combined direct filters, source renames, and trash exclusion.
- Add/remove all matching over thousands of records beyond the visible window, selection persistence, and focus independence.
- Trash/restore/permanent deletion and repeated imports remaining separate entries.
- Steam discovery fixtures for multiple libraries, native/proton candidates, Flatpak, missing prefixes, nonstandard users, Windows/macOS path handling, and manual overrides.
- Merge preservation, stable output names, explicit replacement validation, stale-plan rejection, unrelated-file preservation, interrupted export recovery, and empty-selection behavior.
- Game-ready export contents and full-library backup round trips.

Use a synthetic collection of at least 10,000 entries to verify bounded rendering, query behavior, and image memory use; document the test machine and measurements. No full-library image decoding on startup or during filter changes.

Perform a Linux desktop smoke test covering create/open, each supported archive format, browsing/search, preview preferences, selection, trash, discovery/manual destination, both export modes, and backup/restore. Test real game acceptance of exported sets where installations are available. Run build/test checks on Windows and macOS and native smoke tests before describing those releases as verified.

## Delivery order

1. Verify portrait compatibility constants and cross-platform archive backend; establish Tauri shell, library format, and database migrations.
2. Implement import, label inference, catalog/FTS, and thumbnail browsing.
3. Add metadata editing, persistent selection, and trash.
4. Add discovery adapters, manual destinations, export planning/application, and backup/restore.
5. Complete recovery checks, large-library validation, Linux end-to-end testing, and platform packaging/verification.

The implementation plan will name concrete files, dependencies, tests, and milestones after review of this specification.
