# Development verification

## Task 13 — scale, packaging configuration, and release evidence

One practical release-mode 10,000-portrait check ran on 2026-09-16. `npm run benchmark:seed -- /tmp/portrait-manager-benchmark-10000 10000` generated only plain-color canonical PNGs in four temporary source folders, imported them with the normal folder-import API, and retained the source folders and library for inspection. It created 10,000 total rows: 9,565 active, 435 trashed, and 2,385 persisted selections after the seed's initial selection/trash state.

The fresh measurement process returned the first 16-item catalog page in 9.80 ms, representative lexical FTS p95 in 38.10 ms, the combined source/label filter in 7.49 ms, select-all matching in 5.85 ms, remove-all matching in 5.49 ms, and a 13,156 KiB process peak RSS. Fresh-process library open measured 0.50 ms, with normal filesystem cache state left intact. The FTS p95 target passed. The 9.80 ms result is a core first-page query, not a newly measured first-visible-grid image-paint result. Full detail: `/tmp/portrait-manager-benchmark-10000.benchmark.json`.

This is a catalog-core measurement. It records a 16-item page bound and does not claim browser heap measurements or full-image decoding. Existing terminal browser evidence in the Task 5 section verifies the virtual grid renders fewer than 200 cards with a 10,000-record fixture. Existing native Linux evidence is consolidated in `.superpowers/sdd/2026-09-16-portrait-manager/native-verification.md`; no extra native workflow was run for this task.

Focused configuration verification: `node --check scripts/seed-benchmark.mjs`, `node --check scripts/check-bundle.mjs`, and JSON parsing of `package.json` and `src-tauri/tauri.conf.json` passed. `npm run check` passed with 12 test files / 60 tests, TypeScript checking, and a Vite production build (61 modules).

The GitHub Actions matrix and bundle configuration are unexecuted targets. Windows/macOS native artifacts and smoke tests, Ubuntu 22.04 bundle dependency inspection, final AppImage/deb closure, no-system-libarchive package smoke, signing/notarization, and actual in-game acceptance remain release requirements; they are not claimed as complete.

## Task 1 — core workspace, library lifecycle, desktop entry

Verified on Linux on 2026-09-16 with Rust 1.95.0, Node.js 26.8.2, npm 12.0.2, GTK 3.24.52, and WebKitGTK 2.52.6.

TDD evidence:

- RED — `cargo test -p portrait-core --test library` failed because `Library`, `CoreError`, and the lifecycle API did not exist.
- GREEN — `cargo test -p portrait-core --tests`: 9 passed (4 IPC contracts, 5 lifecycle/schema/migration tests).
- RED — `npm test -- --run src/app/App.test.tsx` failed because the `App` entry component did not exist.
- GREEN — `npm test`: 3 passed (create/close, remembered reopen, actionable lock error).
- RED — `cargo test -p portrait-manager --test lifecycle` failed because `DesktopState` did not exist.
- GREEN — `cargo test -p portrait-manager --test lifecycle`: 2 passed (settings/reopen and stable lock errors).

Final checks:

- `cargo test --workspace --all-targets`: 11 passed, 0 failed.
- `npm test`: 3 passed, 0 failed.
- `cargo fmt --all -- --check`: passed.
- `npm run build`: TypeScript check and Vite production build passed (35 modules).
- `npm run tauri -- build --no-bundle`: passed; optimized Linux binary built at `target/release/portrait-manager`.
- Native WebKit create/open/close smoke: delegated to the controller's isolated Tauri driver after the desktop binary is available.

The private `p1.zip` and `heroes.rar` symlinks were not read, modified, or included in build output.

### Review round 1

- RED — `asset_paths_accept_only_portable_relative_components` failed because the initial `assets.relative_path` check accepted an empty managed path.
- GREEN — the focused core test passed after enforcing nonempty slash-separated relative paths with no drive prefix, backslash, empty component, or `.`/`..` component.
- RED — the two native dialog rejection cases failed with unhandled promise rejections and no visible alert.
- GREEN — both create and open dialog rejection cases passed after moving selection into the guarded open operation.
- Final `cargo test --workspace --all-targets`: 12 passed, 0 failed.
- Final `npm test`: 5 passed, 0 failed.
- Final `npm run build`: TypeScript and Vite production build passed.
- Final `cargo fmt --all -- --check`: passed.
- The controller's Linux WebKit create/open/close/reopen smoke passed before this review round; the review fixes do not change desktop command or lock behavior.

### Controller native smoke — Task 1

2026-09-16: optimized `target/release/portrait-manager` launched under isolated Xvfb through tauri-driver 2.0.6 / system WebKitWebDriver. Verified rendered chooser; actual IPC create/open/close using `/tmp/portrait-native-test/library-task1`; stable library UUID; remembered path after reload; clicked Reopen library then Close library and checked resulting UI. Screenshot `/tmp/portrait-task1.png` visually inspected. Test settings used `/tmp/portrait-native-test/config`; no live game destinations were written.

## Task 2 — safe archive reader and distribution feasibility

Verified on Linux on 2026-09-16 against libarchive 3.8.9.

TDD evidence:

- RED — `cargo test -p portrait-core --test archive` failed because the archive module, extraction limits, job context, and cancellation error did not exist.
- GREEN — the focused suite passed real ZIP, 7z, RAR4, and RAR5 extraction plus unsafe path, link, collision, truncation, unsupported format, encrypted, multipart, limits, cancellation, progress, cleanup, and staging cases.
- RED/GREEN — a nested `A/one.txt` versus `a/two.txt` fixture initially extracted successfully; prefix-level portable collision tracking then made the regression pass.
- RED/GREEN — Windows-forbidden characters and `CONIN$`/`CONOUT$` initially passed path validation; portable validation now rejects them.

Final checks:

- `cargo test -p portrait-core --tests`: 19 passed, 0 failed, 1 intentionally ignored private-fixture smoke.
- Compiled archive test binary with `PATH=/nonexistent`: 9 passed, 0 failed, 1 ignored. No command-line extractor was available.
- Private read-only `p1.zip` extraction into a `tempfile` staging directory: passed in 0.58s.
- Private read-only `heroes.rar` extraction into a `tempfile` staging directory: passed in 3.40s.
- `cargo clippy -p portrait-core --tests -- -D warnings`: passed.
- `cargo fmt --all -- --check`: passed.
- `pkg-config --modversion libarchive`: 3.8.9. `ldd` showed the test binary linked to `/usr/lib/libarchive.so.13` with local zstd, bzip2, and lzma codec libraries.

Windows and macOS received pinned CMake/vcpkg-oriented build configuration but were not natively built or tested on this Linux host. Packaged-app dependency and no-system-libarchive smoke testing remain Task 13 work.

### Task 2 review round 1

- RED — the bounded crafted-RAR run panicked immediately on a RAR4 long-block subtraction; the paired regression also covers the reviewed oversized RAR5 data offset. GREEN — checked absolute offsets reject both promptly as `ARCHIVE_INVALID`, while preflight loops check cancellation and stop at the entry-count bound.
- RED — a valid ZIP comment containing an EOCD signature was rejected, and true ZIP64 multi-disk metadata was accepted. GREEN — structurally matched EOCD and ZIP64 locator/record checks accept the comment and single-disk fixture and reject multi-disk as `ARCHIVE_MULTIPART`.
- RED — Greek sigma and final-sigma archive names extracted together after UTF-8 entry handling was enabled. GREEN — complete Unicode 16.0 default case folding reports `ARCHIVE_PATH_COLLISION` while a standalone Unicode name extracts unchanged.
- FFI audit — `archive_entry_filetype` uses target `libc::mode_t` on Unix and `u16` on Windows, matching libarchive 3.x `__LA_MODE_T`.
- Final `cargo test -p portrait-core --tests`: 24 passed, 0 failed, 1 intentionally ignored private-fixture smoke.
- Compiled post-fix archive test binary with `PATH=/nonexistent`: 14 passed, 0 failed, 1 ignored.
- Post-fix private smokes: `p1.zip` passed in 0.62s and `heroes.rar` passed in 3.73s using temporary extraction directories.

### Task 2 review round 2

- RED — the real one-file RAR5 fixture failed with `ARCHIVE_ENTRY_LIMIT` when `max_entries` was one because RAR preflight counted non-entry metadata headers against the extraction limit.
- GREEN — preflight now has a separate 200,000-header safety budget with cancellation checks. The one-file fixture extracts at `max_entries = 1`, while a synthetic 200,001-header RAR returns `ARCHIVE_INVALID` promptly.
- Final `cargo test -p portrait-core --tests`: 25 passed, 0 failed, 1 intentionally ignored private smoke. Rust formatting and Clippy with warnings denied passed.

## Task 3 — import, validation, inference, and job UI

- RED — focused import/inference tests failed before the import and vocabulary APIs existed.
- GREEN — folder/archive import validates complete image sets, preserves nonstandard images with warnings by default, resizes only on opt-in, commits one staged set at a time, and recovers abandoned import/extraction staging safely.
- Review regression coverage includes symlinked managed-parent rejection, unsupported-file candidate reporting, root-folder label inference, and extraction staging cleanup.
- Tauri lifecycle test verifies cancellation does not wait for the library worker; frontend coverage verifies editable source name, opt-in resize, job results, and issue folder rendering.
- Private read-only archive smoke in disposable libraries: p1 imported 45 default-preserved and 45 opt-in-resized sets; heroes imported 377 canonical sets. Sources were unchanged.

## Task 4 — catalog queries and FTS5

Verified on Linux on 2026-09-16.

TDD evidence:

- RED — `cargo test -p portrait-core --test search` failed with 11 compile errors because the catalog module, `Query::default`, and pagination error did not exist.
- GREEN — `cargo test -p portrait-core --test search`: 8 passed, 0 failed. Coverage includes escaped token compilation, Unicode/diacritic matching, punctuation, descriptions, source and grouped-label filters, bound hostile input, selected/trash isolation, source refresh, full rebuild, stable paging/revision, import indexing, and page-size validation.
- RED — `npm test -- --run src/features/catalog/useCatalog.test.tsx` failed because `useCatalog` did not exist.
- GREEN — `npm test -- --run src/features/catalog/useCatalog.test.tsx`: 3 passed, 0 failed. Coverage includes text debounce, immediate direct filters, page reset, stale-response rejection, and retaining selected results while a replacement request is pending.
- Integration correction — `cargo test -p portrait-manager --all-targets` initially failed because the new `allow-query-catalog` Tauri permission was absent; after adding the scoped permission it passed 3 tests.
- Quality correction — the first `cargo clippy --workspace --all-targets -- -D warnings` run rejected a manual `Default` implementation; deriving `Default` resolved it.

Final checks:

- `cargo test --workspace --all-targets`: 52 passed, 0 failed, 2 intentionally ignored private-archive smokes.
- `npm test`: 9 passed, 0 failed across 2 files.
- `npm run build`: TypeScript checking and Vite production build passed (37 modules).
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `cargo fmt --all -- --check`: passed.

Catalog count, revision, and page rows are read in one SQLite transaction. Imported portraits refresh FTS and revision in their catalog transaction. Source rename and metadata workflows can call the exposed transaction-scoped refresh helpers; the source refresh and rebuild paths are covered against real FTS5. The desktop query command uses a blocking worker while the existing import lock remains isolated from job status and cancellation state.

## Task 5 — thumbnail service and virtualized grid

TDD evidence:

- RED — `cargo test --offline -p portrait-core --test thumbnails` failed with the intended missing thumbnail module and error APIs.
- GREEN — the same command: 3 passed, 0 failed; it covers bad ID/edge rejection, dimension-bounded PNG generation without original mutation, metadata cache regeneration, and switching library roots.
- RED — `npm test -- --run src/features/catalog/PortraitGrid.test.tsx` failed because the grid did not exist.
- GREEN — the same command: 2 passed, 0 failed; a 10,000-record scroll renders fewer than 200 cards and visible behavior covers stale-row suppression, loading, error, empty, focus, and selection callbacks.

Final checks:

- `npm test`: 13 passed, 0 failed.
- `npm run build`: passed (47 modules).
- `cargo test --offline --workspace --all-targets`: 57 passed, 0 failed, 2 intentionally ignored private-fixture smokes.
- `cargo fmt --all -- --check` and `cargo clippy --offline --workspace --all-targets -- -D warnings`: passed.
- Terminal Playwright using system Chromium verified a delayed Source A filter removes `Old ranger` immediately and shows only `Updating results…`; the response then displays `Filtered ranger`. Screenshot: `output/playwright/task5-filtered.png`.

The desktop asset protocol accepts only a UUID, known role, and bounded thumbnail edge, resolving through the active library. Thumbnails use two bounded workers, a 256 MiB decoded-image budget, and per-library metadata-keyed disk cache. Native desktop smoke and release build remain deferred to the controller after Task 6.

### Task 5 review round 1

- Explicit paging reaches offset 200 in a 10,000-record fixture while retaining fewer than 200 cards. `no-store` protocol responses prevent stale ID-only assets after a library switch. Import terminal state refreshes catalog/facets without remounting query, role, or focus, and committed request identity hides retained rows immediately after direct filters.
- Tauri routes now originate from `convertFileSrc`; the handler accepts its percent-encoded route after one decode and CSP permits `http://portrait.localhost`. Facets use normalized query keys and separate display values. Virtual rows use `measureElement`; terminal Chromium observed two columns at 375px, four at 1280px, and five at 1600px, with all artwork roles active. Screenshot: `output/playwright/task5-review-responsive.png`.
- Final: `npm test` 18 passed; `npm run build` passed; `cargo test --offline --workspace --all-targets` 57 passed, 0 failed, 2 private-fixture smokes ignored; formatting and warnings-denied Clippy passed. Native release build was not run.

### Task 5 review round 2

- The fixed-height shell now bounds the catalog with `min-height: 0` and explicit toolbar/grid/pager tracks. A development-only `?testCatalog=1&count=201` fixture supplied a 200-record first page plus pagination for a real Chromium layout check.
- At 1080×720 Chromium measured a 584px catalog main, 444px scrolling portrait viewport with 26,224px content, pager bottom y=703, and 12 rendered virtual cards. The restored scrolling sidebar exposed and focused its final facet (`scrollHeight` 1,580px). Small, Medium, and Large all retained a 14px card gap. Screenshot: `output/playwright/task5-fix2-layout.png`.
- `npm test -- --run src/features/catalog/PortraitGrid.test.tsx src/features/catalog/CatalogBrowser.test.tsx`: 6 passed. `npm run build`: passed (47 modules). No Rust or native release build was run because this round is frontend CSS/test-fixture only.

## Task 6 — optional three-size preview

TDD evidence:

- RED — `npm run test -- PortraitPreview` failed because portrait cards exposed `Focus Elf` instead of the required `Preview Elf` action. The asserted regression confirmed that opening a preview calls focus and does not call the independent selection toggle.
- GREEN — the same focused command passed after the card exposed the preview action. The preview suite then covered its three fitted role thumbnails, metadata, on-demand native-resolution original, Escape close, and arrow-key navigation. Preference coverage confirms that preview visibility and grid role persist together in browser storage.
- Integration coverage opens the optional pane from the real catalog, moves to the next item with ArrowRight, closes with Escape, and confirms that the portrait checkbox stays unselected.

Final frontend checks:

- `npm test`: 23 passed, 0 failed across 6 files.
- `npm run build`: TypeScript checking and Vite production build passed (51 modules).

The preview uses thumbnail protocol URLs for Small, Medium, and Large by default. It renders an original protocol URL only after the matching `View … at native resolution` action, so opening a preview does not preload full-size image files. Focus is frontend-only state and never invokes the selection callback; selection mutations remain Task 7 work.

Browser CLI availability: the Playwright prerequisite `npx` is installed and system Chromium is available, but the required `playwright_cli.sh` wrapper is absent and no project Playwright dependency is declared. No unpinned browser package was downloaded. Existing Task 5 Chromium layout evidence remains valid for the virtualized grid; Task 6 native preview acceptance is owned by the controller.

Controller native import evidence available at `.superpowers/sdd/2026-09-16-portrait-manager/native-milestone-a-imports.json`: a disposable library imported 1,390 portraits (1,001 folder, 6 ZIP, 6 7z, 377 RAR), with zero skips and issues. The controller's post-build native preview, close/reopen, move-library, and source-removal smoke remains required before Milestone A is accepted.

### Task 6 review round 1

- RED — the native 1080×720 screenshot showed the docked three-column layout overflowing because the 600px grid minimum and non-wrapping toolbar remained active above the 1060px viewport breakpoint. It also showed modal Escape bubbling to the preview pane and leaving focus on `BODY`.
- RED — `npm run test -- PortraitPreview CatalogBrowser` failed: the native-resolution close control was not focused, Escape called the preview close handler, and closing a pane failed to restore the originating card’s focus.
- GREEN — preview dialog tests now verify close-control focus, Arrow suppression, Escape closing only the dialog, and native-trigger restoration. Catalog integration verifies preview Escape restores the card that opened it. Native buttons now use their built-in keyboard activation once.
- Browser regression — `npm run test:e2e`: 1 passed at 1080×720 through pinned Playwright 1.64.0-alpha-2026-09-14 and system Chromium. It opens the test catalog, asserts the preview controls fit without catalog horizontal overflow, then verifies dialog and pane Escape/focus transitions.
- Final `npm test`: 25 passed, 0 failed. `npm run build`: passed (51 modules). The local Chromium run uses the dev-only `testCatalog` fixture; it makes no desktop IPC or native-source mutation.

### Task 6 review round 2

- RED — `npm run test -- PortraitPreview` showed that native-dialog Tab and Shift+Tab were not prevented. Chromium confirmed that Tab left the Close native resolution control for a background preview action.
- GREEN — the dialog now prevents both Tab directions and returns its sole close control to focus, while retaining the existing Arrow suppression, Escape close, and trigger restoration. `npm run test -- PortraitPreview`: 5 passed.
- RED — at 760×560, the former narrow rows exceeded the available shell height and the first visible card was pointer-obscured by catalog controls before preview opened.
- GREEN — narrow screens allocate a bounded filter/catalog split, cap card media height, and render the preview as an independently scrollable fixed overlay. `npm run test:e2e`: 2 passed, covering ordinary pointer preview open, layout containment, modal Tab/Shift+Tab, dialog Escape, pane Escape, and focus restoration at 1080×720 and 760×560.

## Task 7 — metadata editing and persistent selection

TDD evidence:

- RED — `cargo test --offline -p portrait-core --test selection --test metadata` initially failed because the selection and metadata APIs did not exist. The frontend focused command also failed because `MetadataEditor` and `SelectionToolbar` did not exist.
- GREEN — the focused core command passed 5 tests. It verifies an `INSERT ... SELECT` matching selection reaches all 1,500 records beyond the first 100-row result page; remove-matching uses the supplied query snapshot; clear and selection persist across reopening; metadata updates search documents and revisions; user labels take precedence over later inference; inference removals create suppressions; source rename persists; and blank or multi-portrait renames are rejected without partial changes.
- GREEN — `npm test -- --run src/features/catalog/CatalogBrowser.test.tsx src/features/catalog/PortraitPreview.test.tsx src/features/metadata/MetadataEditor.test.tsx src/features/selection/SelectionToolbar.test.tsx`: 16 passed. The regression suite covers a single checkbox command, all-matching actions, bulk edit over 201 persisted selected portraits through 200-row requests, custom labels, failure display, and arrows/Escape remaining inside metadata controls rather than reaching preview navigation or closure.

Final checks:

- `npm test`: 31 passed, 0 failed.
- `npm run build`: TypeScript checking and Vite production build passed (53 modules).
- `cargo test --offline --workspace --all-targets`: passed; 2 existing private archive tests remained ignored.
- `cargo clippy --offline --workspace --all-targets -- -D warnings` and `cargo fmt --all -- --check`: passed.

The desktop exposes scoped metadata, source rename, and selection commands. Selection operations run as SQL transactions and matching targets use the catalog query SQL rather than loaded browser rows. The metadata editor fetches full persisted selection in bounded catalog pages before one bulk operation, so it is not limited to the virtualized page. It supports custom labels and removal of the union of labels in that selection. No optimized native build, private archive smoke, or game-directory mutation was run for this task; native Task 7 acceptance is owned by the controller.

### Task 7 fix round 1

Focused regression verification passed after review fixes: `npm test -- --run src/features/catalog/CatalogBrowser.test.tsx src/features/metadata/MetadataEditor.test.tsx` (14 passed), and `cargo fmt --all && cargo fmt --all -- --check && cargo test --offline -p portrait-core --test metadata --test library && cargo clippy --offline -p portrait-core --test metadata --test library -- -D warnings` (format, 9 tests, warnings-denied Clippy). Coverage includes label-wide suppression across inference producer/version changes, migration schema version 2, bulk selected-count failure and page/revision drift, shared-description apply/clear, selected-only filtering, and bulk-dialog initial focus/Tab/Escape/trigger return. No native, archive, or private-fixture smoke was repeated.

### Task 7 fix round 2

`cargo fmt --all && cargo fmt --all -- --check && cargo test --offline -p portrait-core --test library --test metadata && cargo clippy --offline -p portrait-core --test library --test metadata -- -D warnings` passed with 10 focused tests. A true schema-v1 fixture proves legacy inferred-label suppressions are backfilled before a future producer is rejected. `npm test -- --run src/features/catalog/CatalogBrowser.test.tsx` passed 13 tests, including successful bulk-save focus restoration after a deliberately delayed global-count reload. No native, archive, or private-fixture smoke was repeated.

`npm run build` also passed (53 modules), emitting `index-Bc1FvilF.css` and `index-CRqCQqkd.js`.

## Task 8 — Trash, restore, and permanent deletion

TDD and focused verification on 2026-09-16:

- RED — `cargo test --offline -p portrait-core --test trash` failed because `portrait_core::trash` did not exist.
- GREEN — `cargo test --offline -p portrait-core --test trash` passed 7 tests: selection-removing soft trash/restore, purge staging and catalog deletion, restored-at-execution ID exclusion, actual repeated import after trash, pre-cancelled purge preservation, uncommitted-purge restart restoration, committed-purge restart cleanup, and external-path preservation.
- RED — `npm test -- --run src/features/trash/TrashView.test.tsx` failed because the trash view was absent.
- GREEN — focused trash/catalog frontend tests passed 15 tests. They cover local trash marking, restore routing, a count-specific destructive confirmation, Escape, and focus return.
- Final source checks: `cargo test --offline --workspace --all-targets` passed with 2 existing private archive smokes ignored; `cargo clippy --offline --workspace --all-targets -- -D warnings` and `cargo fmt --all -- --check` passed; `npm test` passed 40 tests; `npm run build` passed, emitting `index-O9YeOfm_.css` and `index-LZrETpYP.js`.
- Browser curation smoke: `npm run test:e2e -- --grep "moves a selected portrait"` passed 1 Chromium test through the disposable `?testCatalog=1` fixture. It selects, trashes, views/marks, restores, re-trashes, confirms purge, and observes an empty view. The sandbox denied loopback binding for the initial attempt; the identical test passed after approved local-loopback escalation. No desktop IPC, private archives, or game paths were used.
- Browser layout regression: the complete suite initially timed out at 760×560 because the now-present selection controls could intercept the first card. The narrow controls area is height-bounded and internally scrollable; `npm run test:e2e -- --grep "760×560"` then passed.
- Confirmation focus regression: Escape returns focus to both Empty trash and Purge marked triggers; the TrashView unit coverage verifies both paths.

### Task 8 fix round 1

- RED — `cargo test --offline -p portrait-core --test trash` failed three new regression checks: a final-progress callback cancellation still committed permanent deletion; an uncommitted recovery operation could traverse a symlinked operation directory; malformed purge journal JSON opened silently.
- GREEN — focused core trash suite passed 10 tests. It covers staged rollback after callback cancellation, operation-directory symlink rejection without external modification, actionable malformed-journal retention, and search-document cleanup after purge.
- Desktop job coverage: `cargo test --offline -p portrait-manager --test lifecycle` passed 4 tests, including a purge job reaching a terminal state through the shared job registry.
- Frontend coverage: `npm test -- --run src/features/trash/TrashView.test.tsx src/features/catalog/CatalogBrowser.test.tsx` passed 19 tests. Cross-page marked IDs remain authoritative, a running purge provides Cancel deletion, and a failed all-trash count surfaces an alert while Empty trash remains disabled.
- Focused quality checks: `cargo fmt --all -- --check`, `cargo clippy --offline -p portrait-core --test trash -- -D warnings`, `cargo check --offline -p portrait-manager`, and `npm run build` passed.
- Final fix-round browser/frontend checks: `npm test` passed 43 tests and complete `npm run test:e2e` passed 3 tests. Production build emitted `index-O9YeOfm_.css` and `index-CqtGsQi4.js`.
- Native Task 8 acceptance remains controller-owned. No optimized native build, private archive smoke, or real game-directory mutation was run by the Task 8 worker.

## Task 8 fix round 2 verification (2026-09-16)

- `cargo fmt --check` — passed.
- `cargo test -p portrait-core --test trash opening_clears_committed_purge_journal_when_staging_cleanup_already_finished` — passed after the expected RED failure.
- `cargo test -p portrait-manager --test lifecycle purge_uses_the_shared_job_registry_and_counts_nonexistent_ids_as_zero` — passed after the expected RED failure.
- `cargo test -p portrait-core --test trash` — 11 passed.
- `cargo test -p portrait-manager --test lifecycle` — 4 passed.
- `cargo check -p portrait-core -p portrait-manager` — passed.
- `cargo clippy -p portrait-core --test trash -- -D warnings` and `cargo clippy -p portrait-manager --test lifecycle -- -D warnings` — passed.
- `npm run build` — passed (55 modules), producing `dist/assets/index-O9YeOfm_.css` (`2a93b48c1ccdcd749717986d80addd990723ac8b175a716caadbc4d5060ac950`) and `dist/assets/index-CqtGsQi4.js` (`2147f02042b224bce55920433398966d63f6ae352dba93c1f005af00a0025737`).

The round covers only committed-journal restart cleanup and zero-effective-ID desktop reporting. No optimized native build, private archive smoke, or real game-directory mutation was run.

## Task 9 — discovery and manual destinations

- RED: `cargo test --offline -p portrait-core --test discovery` failed because the `discovery` module and game Steam metadata methods did not exist. The destination UI test also failed because `DestinationList` was absent.
- GREEN: `cargo test --offline -p portrait-core --test discovery` passed 3 tests for game IDs/suffixes, fixture Steam libraries with native and Proton candidates, and a manual prefix with a nonstandard user while Public is ignored. `cargo test --offline -p portrait-manager --test destinations` passed saved-destination revalidation.
- Frontend: `npm test -- --run src/features/destinations/DestinationList.test.tsx` passed 2 tests. It covers normal game-import routing only for an existing Portraits directory and the missing-folder state. The destination picker is a modal, so it does not consume catalog shell height.
- Layout: the existing Chromium checks passed through `npm run test:e2e -- --grep "760×560|1080×720"`; `npm run build` passed and emitted `index-D9lLgwmq.css` and `index-CKwaXuiu.js`.
- Platform limit: Linux fixture and local verification do not establish Windows registry/Known Folder behavior or macOS path behavior. WotR native macOS remains manual pending native-machine data-path verification. No game directories were written.
- Final source checks: `cargo test --offline --workspace --all-targets` passed (including the new discovery and desktop-destination suites; 2 pre-existing private archive tests stayed ignored), and `npm test` passed 45 tests across 10 files. Focused warnings-denied Clippy passed for both discovery and desktop destination tests.
- Read-only local discovery: `cargo test --offline -p portrait-core --test discovery local_steam_discovery_keeps_native_and_proton_candidates_visible -- --ignored` passed. It found at least two existing Kingmaker destinations (native and Proton) and an existing WotR destination; no game files were changed.
- Task 9 fix round 1: command permissions were generated and granted for all five discovery commands. `cargo test --offline -p portrait-core --test discovery` passed 4 fixture checks (one local-only smoke ignored), `cargo check --offline -p portrait-manager` passed, `npm test -- --run src/app/App.test.tsx` passed 7 tests including destination-to-import-progress transition, and `npm run build` passed with `index-D9lLgwmq.css` and `index-BsMcJueC.js`. No native build or game write was run.
- Task 9 fix round 2: automatic discovery now returns an explicit warning report for malformed/unreadable VDF and inaccessible automatic prefixes while continuing later roots; the modal renders these as `Discovery warnings`; the Windows machine fallback uses logical `SOFTWARE\\Valve\\Steam` in the 32-bit view. Discovery fixtures passed 9 with one local-only smoke ignored, focused frontend passed 9, warnings-denied Clippy/formatting passed, and build emitted `index-ClV7TN_w.css` (`6ce5b6451b539f15922fe226bc7be83f7730671a0a3f4c23f2a83bc8e78a82ea`) and `index-Bow15Owb.js` (`5f435b2ec212732419a612be2714f1314a1131d29de775429aeaba240265e795`).

### Task 10 — Export planning and confirmation

- RED: missing export core module/registry and missing ExportDialog; later behavioral RED tests caught portable case collisions producing conflicting add/preserve actions and a misleading overwrite label for dimension-only confirmation.
- GREEN: `cargo test --offline -p portrait-core --test export_plan` (13 passed); `cargo test -p portrait-manager --test export_planning` (2 passed); focused export dialog (10 passed); `npm test` (57 passed / 11 files); focused warnings-denied Clippy, formatting and `npm run build` passed.
- Browser: `npm run test:e2e -- tests/e2e/export.spec.ts tests/e2e/library.spec.ts` passed 6 tests, including 1,395 portraits / 4,185 action rows with long target/saved destination paths at 1080×720 and 760×560, preview invalidation, exact confirmation and catalog focus/layout regressions. The added action initially crowded the old vertical header at 760×560; compact wrapping fixed the failing existing regression. Screenshots were inspected; export controls remain sticky and rows wrap/scroll. Local loopback/Chromium needed approved escalation.
- All export planning IPC commands have explicit permission files, build/handler entries and default capability grants. Native ACL acceptance remains root-owned; production apply is intentionally unavailable until Task 11. No optimized native build, private archive smoke, real-game writes or root-reserved fixture changes were performed.
- Final assets: `index-CGewIvL5.css` (`0508ac043c60ae471d782274ce34557232ad9bf47f8143d311246a82bd8e9b4e`) and `index-C1HTZWbo.js` (`29e0426e75ad1b718247c0f86c27b9122fb8eb2ea88f4de7acad38b26b942bf7`). Detailed registry/apply handoff, IPC shapes and platform limitations are in `.superpowers/sdd/2026-09-16-portrait-manager/task-10-report.md`.

#### Task 10 fix round 1

- RED: changing the known-history overwrite assertion to require confirmation reproduced the review finding (`cargo test --offline -p portrait-core --test export_plan provenance_is_bound`). The destination identity test then exposed the missing destinationId/routed planner contract.
- GREEN: `cargo test --offline -p portrait-core --test export_plan` passed 13; `cargo test --offline -p portrait-manager --test export_planning` passed 2; `cargo clippy --offline -p portrait-core --test export_plan -p portrait-manager --test export_planning -- -D warnings` and formatting passed.
- Every Overwrite now requires confirmation, including matching previous exports. Provenance affects reasons/warnings only. ExportHistory includes destination ID, library ID and canonical path; `plan_export_for_destination` matches them against an independently resolved current routing ID, which is frozen in StoredExportPlan. Missing/mismatched/recreated IDs cannot inherit ownership. The existing no-ID planner conservatively treats history as unknown.
- Task 11 saved-destination/arbitrary-target UUID and `(destination_id, library_id)` manifest routing is documented in the Task 10 report. Frontend assets, IPC request shapes, ACL grants and UI selectors are unchanged. No repeated broad/native/UI check or optimized native build was performed for this core-only fix.

### Task 11 — Recoverable directory/ZIP export

- Initial RED: missing core apply/journal/ZIP API, missing desktop apply/report methods, and App export jobs not opening. Behavioral regressions exposed missing-original recovery retirement, unsafe tampered cleanup paths, modified staging content, rollback retry quarantine, and staging location for an existing collection/mount point; each was fixed and verified.
- Final core `cargo test --offline -p portrait-core --test export_apply --test export_plan`: 16 apply/recovery + 13 planner tests passed. Crash matrices enumerate every actual apply journal/rename/commit boundary and every precommit/committed recovery cleanup boundary, reopen temporary libraries, and compare bytes. ENOSPC/EIO/ENODEV are injected, not physical-device tests.
- Desktop `cargo test --offline -p portrait-manager --test export_jobs --test export_planning`: 4 jobs/history/ACL + 2 planning tests passed. External history is committed-only, independently routed by destination/library/path/identity, with durable receipts for failed storage and retry on opening.
- `cargo test --offline --workspace` passed before the final focused mount-point correction; opt-in private/local smokes stayed ignored. Final correction reran all affected export/planner/desktop suites. Focused warnings-denied Clippy and formatting passed. `npm test`: 58 passed / 11 files. `npm run build`: passed.
- `npm run test:e2e -- tests/e2e/export.spec.ts tests/e2e/library.spec.ts`: 9 passed, including 1080×720/760×560 report geometry, focus containment/return, cancellation, exact reviewed confirmation, and 1,395-portrait preview. Report screenshots inspected. Loopback Vite needed the approved sandbox escalation.
- Final assets: `index-D534c-aW.css` (`3e6380d6ca08673e6d99aee6a70322fd6d57e7e216862e29d114378b9dbb0a44`), `index-CrfPhNhU.js` (`1301c9044a4460310237c2b55007d6a3c820c875a25f96090c5ee82b98544d78`). Full native IPC/selector/backup handoff and platform limits: `.superpowers/sdd/2026-09-16-portrait-manager/task-11-report.md`.
- No worker optimized native build, private archive smoke, real-game writes, or root-owned fixture mutation. Native ACL/apply/ZIP and 1,395-portrait throughput acceptance remain root-owned. macOS/Windows adapters are implemented but unverified; Windows identity/link/race checks are weaker than Linux and require native release validation.

#### Task 11 fix round 1 — recovery ownership and scale

- Reproduced both Important review bugs: committed removed-folder cleanup prevented reopening, and an exclusive-create collision was wrongly deleted as an unstamped staging path. Regressions now pass; the original reviewer probe reports `REMOVAL_RECOVERY: reopened` and `UNSTAMPED_COLLISION: error=RECOVERY_FAILED, user_file_exists=true`.
- New version-2 bounded journal headers append ordered `export_delta` records. Original unversioned snapshots remain readable without migration. Unique parent preparation and local checked-parent moves remove quadratic work, with cancellation checks and a final full identity/digest sweep retained. Partial files lacking durable completed stamps remain for explicit recovery; ordinary owned directory-copy cancellation can clean only bytes proven by the successful-write stamp.
- Disposable 1,395-portrait / 4,185 tiny-PNG benchmark: **4.577 s export**, **5,995,032 total journal bytes / 15,350 rows**, header <2 KB. Cancellation after 10 prepared folders returned in **0.557 s**, with no promotions. This is synthetic overhead evidence, not a replacement for the root's failed >180 s real-image native acceptance.
- Focused GREEN: core apply/recovery **23**, planner **13**, scale **1**, desktop jobs/planning/lifecycle **4 + 2 + 4**, React **19**, focused export progress browser **3**. Focused warnings-denied Clippy, formatting, and production frontend build passed. No broad unchanged/native/private checks were repeated by the worker; root's interrupted journal and targets remain untouched.
- Job messages now identify staging, ZIP writing, destination checking/preparation, file writing, verification, cleanup, and rollback phases; counters describe the current phase. IPC/ACL/selectors unchanged. Final assets: `index-D534c-aW.css` (`3e6380d6ca08673e6d99aee6a70322fd6d57e7e216862e29d114378b9dbb0a44`) and `index-Ea_trYK1.js` (`ca7d8c8af669754ab9c7b3e62d5777cab4bfb9aebbfd7e23109b74e7542159a3`). Full forward-compatibility/backup/native handoff is appended to `task-11-report.md`.

#### Task 11 fix round 2 — live ZIP cancellation

- RED reproduced ordinary ZIP cancellation retaining a blocking journal and ZIP Drop modifying externally edited scratch bytes. The private seek-aware writer now tracks exact successful writes, freezes Drop writes on entry errors/cancel, and supplies in-memory cleanup proof only after full content verification. Crash/externally edited partial files remain protected. Durable journal format and legacy recovery are unchanged.
- Completed focused checks: export apply/recovery **25 passed**; live ZIP adapter **3 passed** (seek/cross-block writes, same-length edit rejection, 1,100-entry buffered round trip **1.056 s debug**); final existing external-edit regression passed for both changed length and same-length edits. Warnings-denied focused Clippy and formatting passed. Testing stopped at the user's request; no additional desktop/browser/native/private/broad suite was repeated.
- Cancellation at archive creation and each written entry preserves old ZIP bytes, retires header/deltas, permits reopening and the next successful export. Source is frozen; IPC/ACL/selectors/frontend assets are unchanged from round one. Root owns the remaining targeted native ZIP cancellation/reopen check. Exact evidence and unchanged platform limits are appended to `task-11-report.md`.

## Task 12 — Focused portable backup/restore delivery (2026-09-16)

- Added SQLite snapshot + full active/trash image ZIP backup and staged validated restore, with mutation serialization, exact-path replacement confirmation, cancellation, and fresh/empty-target refusal. Portable snapshots reject unresolved operations including orphan `export_delta` rows and drop local export receipts.
- Focused checks only: core `--test backup` **2 passed**; desktop `--test backup_jobs` **1 passed**; BackupDialog **2 passed**; affected App create/menu/close test **1 passed** (8 unrelated tests skipped). Core/desktop library clippy, formatting, and final TypeScript/Vite build passed. No broad retest, new fault matrix, native build, or packaging investigation.
- Backup uses **Library ▾ → Back up library**; **Close library** moves into that same compact menu. Restore is on the chooser. Display/grid preferences remain machine-local. Each new command has explicit ACL permission and default capability grant; native acceptance is root-owned.
- Final assets: `index-BtuGDRPe.css` (`6fe615f6ca59eabc0435384844281f79543a83e70983525b0ee6dfff8d603a66`), `index-C_Y01Xlf.js` (`a2183b6c53e0ae564301b0e09d3121a991218b39da291ca8ad475686fce75f91`). Full behavior, command arguments, selectors, initial failures, and practical limits: `.superpowers/sdd/2026-09-16-portrait-manager/task-12-report.md`.
