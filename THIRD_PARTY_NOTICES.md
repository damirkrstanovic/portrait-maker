# Third-party notices

## libarchive

Portrait Manager uses libarchive 3.8.9 to read ZIP, 7z, RAR4, and RAR5 archives. Libarchive is a mixed permissive distribution; the complete upstream notice is included at `licenses/libarchive-COPYING`. A static release build must preserve the applicable BSD-style, UC Regents compress-reader, and selected BLAKE2 terms from that notice.

Release packages must also reproduce the notices for each codec and platform library copied into that target’s package. The exact dependency set is recorded from inspection of each final package during release validation; the local Arch/CachyOS probe is not used as a substitute for Ubuntu 22.04 package evidence.

## Unicode case-folding data

The portable collision checker includes the full-fold mappings that differ from Rust's lowercase mapping, generated from Unicode 16.0.0 data. Unicode's license is included at `licenses/UNICODE-LICENSE.txt`.
