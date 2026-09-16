# Archive test fixtures

These fixtures contain only short synthetic text. They contain no portrait art or user data.

`safe.zip`, `two-files.zip`, `case-collision.zip`, `nested-case-collision.zip`, `unicode-case-collision.zip`, `unicode-name.zip`, `symlink.zip`, `truncated.zip`, and `multidisk.zip` were generated locally for this project on 2026-09-16. The source text was written for these tests and is released under the repository license. `unicode-case-collision.zip` contains Greek sigma/final-sigma names (`SHA-256 ab578231ef90c86e86a20b692b22489c49a1d50d68eac47a1d247a0bb871dcbb`); `unicode-name.zip` verifies a non-ASCII UTF-8 name (`SHA-256 e9820014cad7937b30d7c48085c3e0bcc5b9a2054b6f33e7d28b750d1ec4e041`). `truncated.zip` is `safe.zip` with its final eight bytes removed. `multidisk.zip` has the EOCD disk number set to one (`SHA-256 728c3f300bfdf164f734e01b992cba6cd3ff34723a0ff244a93affac452d3368`). The ZIP fixtures are checked in so test execution does not require `zip`, `bsdtar`, Python, or another archive program.

The following binary fixtures were decoded from uuencoded reference assets in the upstream [libarchive test suite](https://github.com/libarchive/libarchive/tree/master/libarchive/test), retrieved on 2026-09-16:

| Local file | Upstream reference file | SHA-256 |
| --- | --- | --- |
| `safe.7z` | `test_read_format_7zip_copy.7z.uu` | `67a85f85950c4aaa524eb3f22be1fbac586e90c76de21e10fc1c2ce7685e04c2` |
| `safe-rar4.rar` | `test_read_format_rar_windows.rar.uu` | `8d689455e9ecd92c19426604e2360b5ef8eb023890fe46aabbe2864260b70fc9` |
| `safe-rar5.rar` | `test_read_format_rar5_stored.rar.uu` | `35d75e315d164d2e329afc28f7d844f013271b4fcffd4ddd78efcdd114a383a7` |
| `encrypted.7z` | `test_read_format_7zip_encryption.7z.uu` | `91f5427859ad1391c9fb877c98e4b55213c479f39fd012882f7d112388842076` |
| `multipart.part01.rar` | `test_read_format_rar5_multiarchive.part01.rar.uu` | `2f3894dc4b8a1bba90830ef1f89f457b0295b6f9030c70febd57283721bc2ce3` |

Libarchive distributes these test assets under its repository licensing terms. Its license is reproduced at `licenses/libarchive-COPYING`. The fixtures remain unmodified after decoding; local names make their test purpose clear.

`not-an-archive.txt` is original project test data under the repository license.
