# Pathfinder Portrait Manager

An offline desktop library for Pathfinder: Kingmaker and Wrath of the Righteous portraits. The application keeps its SQLite catalog and managed image files in a portable folder chosen by the user.

Import complete portrait sets from folders or archives, search and edit metadata, curate selections and trash, export game-ready portraits, and keep portable library backups.

## License

The project's original code is available under the [MIT License](LICENSE),
which permits commercial use, modification, and redistribution while retaining
the license notice. Dependencies and upstream test fixtures retain their own
licenses; see [third-party notices](THIRD_PARTY_NOTICES.md). Imported portrait
artwork is not covered by this project's license.

## Install and run

Download the bundle for your platform from the [GitHub Actions artifacts](https://github.com/damirkrstanovic/portrait-maker/actions) or [Releases page](https://github.com/damirkrstanovic/portrait-maker/releases) when one is available. A released bundle includes the application itself; Rust and Node.js are only needed to build it from source.

On Linux, there are two useful formats:

- **AppImage** is the easiest default. Mark it executable and launch it:

  ```sh
  chmod +x ./*.AppImage
  ./*.AppImage
  ```

- **Debian/Ubuntu `.deb`** integrates with the desktop and package manager:

  ```sh
  sudo apt install ./'Pathfinder Portrait Manager'_*.deb
  ```

Your library remains a regular portable folder. Put it somewhere with enough storage for the originals, then choose **Create library** or **Open library** in the app. Closing the library before copying it with another backup utility avoids inconsistent database/image copies.

## Build from source

- Rust 1.88 or newer (the development host tested Rust 1.95.0)
- Node.js 20.19 or newer
- npm 10 or newer
- Tauri 2 Linux prerequisites when building on Linux (GTK 3 and WebKitGTK 4.1 development packages)

Linux is the only platform with direct native workflow evidence so far. Windows and macOS are configured native build targets, pending their native build and smoke records. Release bundles do not require users to install Rust or Node.

Install the build prerequisites and dependencies once. On Debian/Ubuntu (including the Ubuntu 22.04 release-build baseline):

```sh
sudo apt update
sudo apt install build-essential cmake ninja-build pkg-config \
  libgtk-3-dev libwebkit2gtk-4.1-dev libssl-dev libbz2-dev \
  liblzma-dev libzstd-dev zlib1g-dev libacl1-dev
```

Then install Rust 1.88+, Node.js 20.19+, and project dependencies:

```sh
npm install
```

Start the desktop application:

```sh
npm run desktop
```

On Linux, `npm run desktop` first prepares the pinned libarchive dependency automatically; the first run downloads and builds it. Later runs reuse the build. Windows needs the MSVC build tools and libarchive through vcpkg; macOS needs Xcode command-line tools and libarchive 3.8.9+ discoverable by pkg-config. See [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for platform setup.

The development profile optimizes the image and hashing routines while retaining debug symbols and assertions. This keeps duplicate scans responsive without a release build.

`npm run dev` starts only Vite's browser development server. It is useful for frontend work, but it does not start the Tauri desktop shell or provide filesystem/game access.

Run the normal checks when changing the project:

```sh
npm run check
cargo test --workspace --all-targets
```

## Create distributable packages

On any supported native build machine, use:

```sh
npm run package
```

It selects the package format for that operating system.

On Linux, this is the normal release-candidate command:

```sh
npm run package:linux
```

It builds the pinned static archive reader, then asks Tauri to create a Debian package and an AppImage in `target/release/bundle/deb/` and `target/release/bundle/appimage/`. The AppImage is convenient for direct download; the `.deb` supplies desktop integration and dependencies for Debian/Ubuntu. [Tauri recommends producing Linux builds](https://v2.tauri.app/distribute/appimage/) on the oldest supported base with WebKitGTK 4.1—Ubuntu 22.04 or Debian 12—because newer glibc versions can make an otherwise valid AppImage fail on older systems.

For a Debian package only (without downloading AppImage tooling), use `npm run package:linux -- deb`. Both `.deb` and AppImage were built on the Linux development host. Windows/macOS packages have not been verified locally. The Linux script disables linuxdeploy’s extra stripping step to accommodate modern system libraries ([upstream option](https://github.com/linuxdeploy/linuxdeploy/issues/72)). Bundle formats follow [Tauri distribution](https://v2.tauri.app/distribute/).

Windows and macOS installers must be built on their native platforms:

```sh
npm run package:windows # NSIS installer and MSI
npm run package:macos   # app bundle and DMG
```

Unsigned local packages are usable for testing. Signing/notarization is recommended for public Windows and macOS downloads to avoid security warnings. GitHub Actions builds native bundle candidates on Linux, Windows, and macOS and retains them as [downloadable workflow artifacts](https://github.com/damirkrstanovic/portrait-maker/actions); tagged releases should use those native artifacts rather than cross-compiling on a Linux workstation.

Create a production desktop binary without platform bundles:

```sh
npm run tauri -- build --no-bundle
```

Use that binary only for local testing. `npm run package:linux` is the supported distribution command. Linux release candidates must be built on Ubuntu 22.04: this development host uses glibc 2.44, so its binaries cannot establish an Ubuntu 22.04-compatible artifact. The release checklist records the required package inspection and native smoke evidence.

## Browsing portraits

The grid starts with compact thumbnails (about five columns at the default window size). Use **Zoom** to change card size from **75% to 200%**; the app remembers it. **Small / Medium / Large** chooses the portrait image variant independently of zoom. Click a portrait to preview all three images.

## Duplicate portraits

Imports first scan for exact matches against active library portraits and earlier sets in the same import. A duplicate must have identical decoded pixels and dimensions in **all three sizes**; PNG compression and embedded metadata do not affect matching. Resized images and alternative crops are not exact duplicates. If you enable **Fit and pad**, matching compares the resulting managed images.

When matches are found, review the list and choose **Skip duplicates and import** or **Import anyway**. Skipping retains the existing portrait while adding the incoming source association and inferred labels, respecting labels you previously removed. Portraits in Trash do not cause an incoming set to be skipped.

For existing libraries, choose **Library ▾ → Find duplicates**. Choose the keeper for each group, or uncheck a group to leave it alone. Differing names and descriptions are shown for review. After confirmation, extra copies move to Trash, source associations and labels are combined, selection is retained if any copy was selected, and the keeper retains its name and description. Original metadata remains on the trashed copies. Nothing is permanently deleted by deduplication; empty Trash separately when satisfied.

Pixel fingerprints are cached in SQLite and computed during import. Later scans check file metadata and reuse unchanged fingerprints; edited or missing images invalidate their entries. Existing libraries fill the cache on their first scan. Import review and import reuse validated PNG fingerprints, including when an archive is extracted again. Before moving copies to Trash, the app still revalidates file content rather than relying on modification times.

The first version detects exact matches only. It does not guess whether visually similar artwork is the same portrait.

## Scale fixture

Create a disposable 10,000-portrait library with normal folder imports:

```sh
npm run benchmark:seed -- /tmp/portrait-manager-benchmark-10000 10000
```

The destination must be a new child of the system temporary directory. The command creates canonical plain-color PNG sets, four source folders, deterministic names/labels/descriptions, and persisted selection/trash states. It writes timings and process peak RSS to `/tmp/portrait-manager-benchmark-10000.benchmark.json`; browser DOM bounds come from the existing virtual-grid browser check rather than a synthetic heap claim.

The library folder contains `library.json`, `library.sqlite3`, `portraits/`, `cache/`, `staging/`, and an OS-locked `library.lock`. Copy or move it only while it is closed. Machine-specific settings, including the most recent library path, stay in the operating system's application configuration directory.


## Backup, restore, and local preferences

Choose **Library ▾ → Back up library** to save a portable ZIP outside the library folder. It includes library identity, sources, metadata, labels, selections, and all active and trashed images. Existing files require confirmation for the exact backup path. You can cancel before the completed archive replaces the output file.

To restore, close the current library from **Library ▾ → Close library**, then choose **Restore library backup**. Select the backup ZIP and a new or empty destination folder. Restore validates the database and every referenced image before opening the restored library. It never combines libraries or overwrites a nonempty folder. Unsupported newer library versions, malformed archives, missing/corrupt images, and unresolved recovery operations are rejected. Use **Open restored library** after verification finishes.

Portable backups exclude thumbnail caches, staging data, writer locks, machine-local export receipts/history, and app settings. Saved game destinations remain specific to this computer. Grid image size and preview visibility keep their existing local preferences when you restore another library. Game-ready export is separate: it writes playable portrait sets and excludes trash and library metadata.

For external backup tools, first close the library in the app, then copy the complete library folder. Copying a live SQLite file and image directory separately can produce an inconsistent backup. Reopen and resolve any reported recovery issue before creating a portable backup. There is no cloud synchronization. Encrypted and multipart archives are unsupported. Restore uses the existing bounded archive reader (100,000 entries, 20 GiB total, 128 MiB per file) and needs temporary disk space beside the target for the extracted library. Backups also need temporary space beside the ZIP output.

## Describe portraits with a local vision model

Choose **Library → Describe portraits**. The default server is
`http://127.0.0.1:8080/v1/chat/completions`, using `gemma-4-26b-a4b` through
[llama.cpp's chat completions API](https://github.com/ggml-org/llama.cpp/blob/master/tools/server/README.md#post-v1chatcompletions-openai-compatible-chat-completions-api). The endpoint and model can be changed in the
dialog. Supply the path to a text file containing the API token, such as `.apikey`;
the backend reads it directly. The token is never stored in the portrait library
or included in backups. The repository ignores its root `.apikey` file.

Select portraits using the existing search/filter and selection controls, then
analyze **Selected active portraits**, or choose **All active portraits**. Only
the large image is sent. Analysis is explicit, runs one portrait at a time, and
can continue in the background while you browse. Reopen the dialog to see
progress or stop the batch. Stopping waits for the current request to finish;
completed results remain saved. Restarting normally skips analyzed portraits.
Use the reanalysis checkbox to replace earlier model results.

The model creates a visual description and suggested labels for apparent gender
presentation, fantasy ancestry (`race`), class archetype, combat style, weapons,
armor, and visible magic. Uncertain attributes should be omitted. These are
visual suggestions, not established character lore. Model descriptions appear
in the preview separately from your notes. Your notes and manually edited or
removed labels are preserved during reanalysis.

SQLite FTS indexes the descriptions alongside names, sources, and labels. For
example, search for `red cloak` or `glowing staff`, then combine that search with
source and label filters. Generated descriptions travel with library backups;
server settings and the token file stay on the computer. Normal library use
still works without a model server or network connection.

Model labels are freeform category/value pairs, not a fixed vocabulary. The RPG
categories above are suggestions; specific weapons, additional ancestries, and
other useful visual tags are accepted. Descriptions have no application-level
character limit, and the app does not impose a label-count limit. Blank labels
are ignored. The model server controls its generation budget; the request still
has a 120-second timeout.

Analysis failures are recorded in `logs/analysis-errors.jsonl` inside the app's
machine-local configuration folder (on Linux, normally
`~/.config/dev.portraitmaker.pathfinder/`). Each entry contains the portrait and
job IDs, timestamp, model, detailed error, and server response when available.
The failure message gives the log path. API tokens are redacted, and image
payloads and authorization headers are not logged. Logs are separate from the
portable library and its backups. Old failures from before logging was added
cannot be reconstructed; rerun with reanalysis unchecked to retry portraits
without saved results.
