# Pathfinder Portrait Manager

An offline desktop library for Pathfinder: Kingmaker and Wrath of the Righteous portraits. The application keeps its SQLite catalog and managed image files in a portable folder chosen by the user.

Import complete portrait sets from folders or archives, search and edit metadata, curate selections and trash, export game-ready portraits, and keep portable library backups.

## Requirements

- Rust 1.88 or newer (the development host tested Rust 1.95.0)
- Node.js 20.19 or newer
- npm 10 or newer
- Tauri 2 Linux prerequisites when building on Linux (GTK 3 and WebKitGTK 4.1 development packages)

Linux is the only platform with direct native workflow evidence so far. Windows and macOS are configured native build targets, pending their native build and smoke records. Release bundles do not require users to install Rust or Node.

## Development

```sh
npm install
npm run dev
npm test
cargo test --workspace --all-targets
npm run tauri -- dev
```

`npm run check` runs the TypeScript check, frontend tests, and production frontend build. Rust CI uses locked dependencies with formatting, warnings-denied Clippy, and workspace tests.

Create a production desktop binary without platform bundles:

```sh
npm run tauri -- build --no-bundle
```

To create configured platform bundles, use `npm run tauri -- build --bundles <target> --ci` on the corresponding native runner. Linux release candidates must be built on Ubuntu 22.04: this development host uses glibc 2.44, so its binaries cannot establish an Ubuntu 22.04-compatible artifact. The release checklist records the required package inspection and native smoke evidence.

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
