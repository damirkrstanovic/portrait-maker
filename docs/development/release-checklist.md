# Release checklist

## Build candidates

- [ ] Run the locked GitHub Actions matrix in `.github/workflows/check.yml` on Ubuntu 22.04, Windows MSVC, and macOS. The configuration is present; no matrix run is claimed by this repository yet.
- [ ] On Ubuntu 22.04, build libarchive 3.8.9 from the release tarball using the SHA-256 recorded in `packaging/libarchive/CMakeLists.txt`. The static archive must include ZIP deflate, LZMA/LZMA2, bzip2, zstd, and OpenSSL support.
- [ ] Produce and retain the Linux `.deb` and AppImage, Windows NSIS installer, and macOS `.app` artifacts. A compiler is required only to create these candidates, never to run a released bundle.
- [ ] Inspect each final native executable with its platform tool: `ldd`/`readelf` on Linux, `dumpbin /dependents` on Windows, and `otool -L` on macOS. Record the direct dependencies, rpath where relevant, and the Linux glibc symbol ceiling.
- [ ] On Linux, verify the release executable has no `libarchive.so` dependency. Extract and inspect the AppImage and `.deb`; verify the final AppImage library closure and Debian control dependencies on Ubuntu 22.04.
- [ ] Run the ZIP, 7z, RAR4, and RAR5 fixture smoke with an empty `PATH` and without a system libarchive visible. This confirms the packaged application does not depend on an external extractor or host libarchive.
- [ ] Run the native desktop smoke on each platform. Linux synthetic-fixture evidence exists; Windows and macOS checks are still required.

## Product acceptance

- [ ] Test a game-ready export in each supported installed game before release. Existing PNG headers establish the dimensions; they do not prove in-game acceptance.
- [ ] Confirm read-only Steam discovery against the chosen installations. Do not use a live game directory for export or deletion testing.
- [ ] Run the 10,000-portrait command below on the release candidate's development machine and retain its JSON result. Investigate target misses; do not reduce the fixture to make a result pass.

```sh
npm run benchmark:seed -- /tmp/portrait-manager-benchmark-10000 10000
```

The command refuses an existing destination, generates only plain-color canonical PNGs in temporary input folders, imports them through the normal folder-import path, and records a fresh-process open, catalog query, filter, bulk-selection, and process-RSS measurement in `/tmp/portrait-manager-benchmark-10000.benchmark.json`. Its 16-item catalog page bound is paired with the existing browser virtual-grid evidence; it does not claim to measure browser heap memory.

## Distribution

- [ ] Include the root MIT `LICENSE` (configured as a Tauri bundle resource). Original project code is MIT; third-party components keep their own licenses.
- [ ] Collect full copyright/license and applicable Apache NOTICE texts for the JavaScript and Rust code included in each binary. Retain the MPL source links in `THIRD_PARTY_NOTICES.md` and provide source for any modified MPL-covered files. Check source/relinking obligations for any bundled LGPL native libraries.
- [ ] Preserve the final `THIRD_PARTY_NOTICES.md`, full libarchive notice, Unicode notice, and the exact codec notices for libraries copied into a final package. The current libarchive notice must retain its complete upstream wording, including the applicable compress-reader and BLAKE2 terms.
- [ ] Publish from tracked source or clean CI output, not a ZIP of the development directory: `.apikey`, `.env*`, private portrait archives, generated logs, and local build output must stay private. Recheck new commits for credentials. Git history contains the author's name/email and the former private model-server hostname; removing current references does not erase history.
- [ ] Sign Windows and notarize/sign macOS artifacts with release credentials. These are distribution operations and are intentionally absent from local checks.
- [ ] Publish only after the completed platform evidence, package inspection, and game acceptance records are attached to the release.
