# Third-party notices

The root `LICENSE` covers this project's original work. It does not replace the
licenses of dependencies, generated Unicode data, or upstream archive fixtures.

## JavaScript and Rust dependencies

Versions are pinned in `package-lock.json` and `Cargo.lock`. The JavaScript
runtime packages (React, React DOM, scheduler, TanStack Virtual, and the Tauri
API/dialog packages) are MIT-licensed or offer MIT as a license option. The
development dependency `caniuse-lite` uses CC-BY-4.0 for its browser data.

Rust dependencies mostly use permissive MIT, Apache-2.0, BSD, ISC, Zlib, Unicode,
or similar terms. The following locked crates use MPL-2.0; their unmodified
source, including upstream notices, is available at these versioned links:

| Crate | Version | Source |
| --- | --- | --- |
| cssparser | 0.36.0 | [Source](https://crates.io/api/v1/crates/cssparser/0.36.0/download) |
| cssparser-macros | 0.6.1 | [Source](https://crates.io/api/v1/crates/cssparser-macros/0.6.1/download) |
| dtoa-short | 0.3.5 | [Source](https://crates.io/api/v1/crates/dtoa-short/0.3.5/download) |
| option-ext | 0.2.0 | [Source](https://crates.io/api/v1/crates/option-ext/0.2.0/download) |
| selectors | 0.36.1 | [Source](https://crates.io/api/v1/crates/selectors/0.36.1/download) |

MPL-covered files retain their license; this project's separate original files
remain MIT-licensed. For distributed binaries containing MPL code, preserve the
source availability notice and provide any modifications to those covered files.
See the [Mozilla MPL FAQ](https://www.mozilla.org/en-US/MPL/2.0/FAQ/).

This is a dependency overview, not a complete binary attribution bundle. Before
publishing installers, collect and include the full copyright/license notices
for the JavaScript and Rust dependencies actually included in each release,
including applicable Apache NOTICE files. Native libraries copied into packages
(such as GTK/WebKit and codecs) also retain their own terms, which may include
LGPL source/relinking requirements. Inspect each final package; the application's
MIT license does not override those obligations.

## libarchive

Portrait Manager uses libarchive 3.8.9 to read ZIP, 7z, RAR4, and RAR5 archives. Libarchive is a mixed permissive distribution; the complete upstream notice is included at `licenses/libarchive-COPYING`. A static release build must preserve the applicable BSD-style, UC Regents compress-reader, and selected BLAKE2 terms from that notice.

Release packages must also reproduce the notices for each codec and platform library copied into that target’s package. The exact dependency set is recorded from inspection of each final package during release validation; the local Arch/CachyOS probe is not used as a substitute for Ubuntu 22.04 package evidence.

## Unicode case-folding data

The portable collision checker includes the full-fold mappings that differ from Rust's lowercase mapping, generated from Unicode 16.0.0 data. Unicode's license is included at `licenses/UNICODE-LICENSE.txt`.
