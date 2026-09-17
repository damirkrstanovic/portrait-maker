# Browsing performance

Investigation on 2026-09-17 used a read-only SQLite snapshot of a real library:
2,853 portraits (2,164 active), 8,559 PNGs, five sources, and 15 labels. Originals
were never modified. The first 24 active portrait sets were copied to a temporary
library for thumbnail timings; its cache started empty.

The 200-item catalog page took 9–10 ms and source/label facets took 30–31 ms.
The grid already virtualizes rows and pages results, so total catalog size was
not the measured bottleneck. First-view thumbnail creation was expensive and
held the shared library mutex, blocking catalog commands. The custom image
protocol also performed that work synchronously.

Changes:

- Serve image requests asynchronously on the runtime's blocking worker pool.
- Resolve managed paths under the library lock, then release it before image
  decoding, resizing, encoding, or waiting for the thumbnail worker budget.
- Retain the two-renderer/memory budget and existing disk caches.
- Avoid enlarging small originals just to create a thumbnail.
- Optimize PNG compression/decompression dependencies in development builds,
  alongside the existing image/core optimizations. Release builds already
  optimize these dependencies.

Sequential development-build timings for 24 thumbnails at a requested 360px
maximum edge (local machine; filesystem caches may be warm):

| Role | Before, empty thumbnail cache | After, empty thumbnail cache |
| --- | ---: | ---: |
| Small | 1.44 s | 0.08 s |
| Medium | 1.87 s | 0.39 s |
| Large | 4.08 s | 1.02 s |

A repeat pass using the disk thumbnail cache took about 1.2 ms for all 24.
These measure backend work, not end-to-end webview frame rate. Async serving
also prevents this work from occupying the synchronous protocol callback;
no UI-latency multiplier is claimed from these sequential timings.

Reproduce with:

```sh
cargo run -p portrait-core --example browse-probe -- /path/to/library
```

The probe opens the source SQLite database read-only, snapshots it and copies
only the first 24 active portrait sets into a disposable temporary directory.
It writes thumbnails only in that temporary copy. It does not print portrait
names or retain artwork in the repository.
