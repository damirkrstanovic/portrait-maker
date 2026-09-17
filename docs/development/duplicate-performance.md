# Duplicate scan performance

The previous implementation decoded all three PNGs and rebuilt their pixel fingerprints on every scan. Import review and import also repeated full validation and decoding. In the development profile, RGBA conversion, temporary packing buffers and unoptimized SHA-256 added substantial CPU cost.

The current implementation stores two derived caches in SQLite:

- `portrait_fingerprints`: complete-set fingerprint and per-file identity/change stamps. Unchanged managed portraits need only file metadata checks and database reads.
- `pixel_fingerprints`: encoded PNG SHA-256 plus rendering mode → validated pixel fingerprint and dimensions. Review and import can reuse decoding work even if an archive is extracted to a different temporary folder. Resize modes have separate keys.

Every import populates fingerprints, including “Import anyway.” Old libraries populate them lazily. Changed files are rechecked. Cache writes are batched; backup/restore excludes derived cache contents. Consolidation revalidates content before moving copies to Trash. Hot image/hash routines are optimized in development builds while retaining debug symbols and assertions.

## Reproduce

With the project's Rust/native dependencies installed:

```sh
cargo run -p portrait-core --example duplicate-benchmark
```

The probe creates 12 synthetic complete sets at canonical game dimensions in a temporary directory. It times import, an import-seeded scan, a scan after clearing only this disposable library's fingerprint caches, a warm scan, import review and import with duplicate skipping. It never opens an existing user library.

Initial measurements on the Linux development host (single runs, not a general latency guarantee):

| Operation | Previous implementation | Cached implementation |
|---|---:|---:|
| Import 12 sets | 1.603 s | 2.041 s |
| Uncached library scan | 9.756 s | 2.021 s |
| Repeated library scan | 9.639 s | 0.001 s |
| First scan after new import | 9.756 s | 0.001 s |
| Import duplicate review | — | 0.005 s |
| Skip duplicates after review | — | 0.011 s |

Import now does fingerprint work up front. Warm scans avoid pixel work altogether. Initial cache filling and changed files still require processing; timings depend on image sizes, compression, storage and library size. These figures use the development build and generated images, not a 10,000-portrait artwork benchmark.
