# Native archive compatibility

## Game destination discovery

Steam discovery uses `keyvalues-parser 0.2.4` for Valve KeyValues VDF and ACF files. It reads current and legacy Steam library-folder forms, installed app manifests, Proton prefixes, conventional Linux roots, Flatpak Steam, macOS Steam, and the Windows registry adapter. Discovery is read-only and never creates a game folder.

Linux has an evidence-backed native Kingmaker candidate at the Unity configuration path. WotR has no automatic native Linux path. Windows uses the LocalAppDataLow Known Folder adapter for both games. macOS probes only the documented Kingmaker aliases that already exist; WotR remains manual until its native data directory is verified.

Manual destinations accept a direct Portraits folder or a launcher-neutral Wine/Proton prefix. The resolver excludes Public and default template users, shows every remaining user candidate, records a destination in local desktop settings, and revalidates its state before returning it for use. Importing existing portraits uses the normal `game` import request and never starts automatically.

The core archive reader calls libarchive directly through a narrow internal FFI module. It registers only ZIP, 7-Zip, RAR4, and RAR5 readers and streams accepted regular-file bodies through a 64 KiB buffer. It never invokes `7z`, `unrar`, `bsdtar`, or another extractor process.

## Verified development host

Linux was verified on 2026-09-16 with libarchive 3.8.9 discovered through `pkg-config`. Real ZIP, 7z, original RAR, and RAR5 fixtures all passed. The same compiled test executable also passed with `PATH` empty, which confirms the reader has no command-line extractor runtime dependency. The private `p1.zip` and `heroes.rar` samples were read through the API and extracted only below temporary directories; their source files were not modified or copied into the repository.

## Release build configuration

`crates/portrait-core/build.rs` requires libarchive 3.8.9 or newer through `pkg-config` on Unix development builds and through vcpkg for MSVC builds. `packaging/libarchive/CMakeLists.txt` pins release-source builds to libarchive `v3.8.9`, disables its command-line programs and tests, and builds a static library. It accepts CMake 3.22, which is available on the Ubuntu 22.04 Linux release-build baseline. CMake presets cover Linux x86-64, Windows x64 with the `x64-windows-static-md` vcpkg triplet, and both macOS architectures. The CI matrix builds a native package candidate per runner; it does not yet produce a universal macOS binary.

The release build must retain ZIP deflate support and the LZMA, bzip2, zstd, and crypto libraries selected by libarchive for the tested 7z/RAR codecs. Static linking is preferred. If an AppImage uses shared libraries, Task 13 must bundle the pinned `libarchive.so` and its codec dependencies with an app-relative rpath, then test on a host without system libarchive.

Windows and macOS are configured targets, but they have not been built or run on this Linux host. Native package verification remains required before release. The GitHub Actions workflow produces candidate artifacts on their native runners; published Windows and macOS releases still need package inspection, no-system-libarchive smoke testing, and platform signing/notarization.

## Supported and rejected input

Support is enabled only after a real fixture passes: ZIP, 7z, RAR4, and RAR5 currently pass on Linux. The reader rejects other signatures, encrypted entries or unknown encryption state, conventional multipart names, ZIP multi-disk metadata, RAR4/RAR5 volume and split flags, links, special files, unsafe portable paths, and case-folded path collisions. Split 7z has no dependable in-container multipart flag, so rejection combines known split-volume filename patterns with a complete single-file libarchive traversal; this is the documented detection boundary.
