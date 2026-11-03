# Session 168 — `releasepack` implementation

**Date:** 2026-11-03
**Status:** **GREEN — 51/51 checks green, three consecutive full runs.**
**Contract:** [`M5_APPLICATION_SELECTION.md`](M5_APPLICATION_SELECTION.md),
section 7 (`releasepack`), frozen in Session 163.
**Starting HEAD:** `6714c52` (`benchmarks: audit M5 hookbox adversarially`).
**Scope:** the third and last frozen M5 application. No performance
measurement, no MINK source change, no change to `repoaudit` or `hookbox`.

## 1. Selection

`M5_APPLICATION_SELECTION.md` names exactly three applications:
`repoaudit` (complete), `hookbox` (complete, contract partial), and
`releasepack`. `releasepack` is therefore the next and final planned
application; no workload was invented.

## 2. What was built

| Path | Purpose |
| --- | --- |
| `benchmarks/m5/releasepack/app/main.mink` | complete `build` / `verify` / `extract` application |
| `benchmarks/m5/releasepack/fixtures/basic/tree/**` | 11-file ASCII fixture tree (8 selected) |
| `benchmarks/m5/releasepack/fixtures/basic/config.json` | frozen-shape build config |
| `benchmarks/m5/releasepack/expected/basic/manifest.csv` | byte-exact manifest golden |
| `benchmarks/m5/releasepack/expected/basic/archive.sha256` | deterministic archive digest |
| `benchmarks/m5/releasepack/expected/basic/stdout.txt` | stdout golden (archive size substituted) |
| `benchmarks/m5/releasepack/expected/help.txt` | byte-exact help golden |
| `benchmarks/m5/releasepack/tests/oracle.py` | independent oracle (`hashlib`, `csv`, `zipfile`) |
| `benchmarks/m5/releasepack/tests/driver.py` | contract driver |
| `benchmarks/m5/releasepack/README.md` | usage, semantics, limitations |
| `benchmarks/m5/releasepack/.gitattributes` / `.gitignore` | byte-exact fixtures; ignored build outputs |

The ZIP work is delegated to `stdlib/zip.mink` (fixed level 6 and fixed DOS
epoch 1980-01-01), hashing to `stdlib/hashing.mink`, CSV parsing to
`stdlib/csv.mink`. No stdlib or compiler file was modified.

## 3. Contract mapping

| # | Frozen requirement | Evidence |
| ---: | --- | --- |
| 1 | Three commands with exact arity, `--help`; other forms exit 1 | 12 usage cases + exact help golden |
| 2 | Config has exactly five keys; types/ranges validated | 3 syntax cases + 18 schema cases -> 3 |
| 3 | `root` exists and is a directory | missing/file -> 4; unsafe/absolute/`..` -> 4 |
| 4 | `archive`/`manifest` parents exist; not inside `root` | missing parent, parent-is-file, inside-root -> 4 |
| 5 | Extensions 1–8, `^\.[A-Za-z0-9]{1,8}$`, case-insensitive | schema cases; `src/mixed.TXT` selected |
| 6 | Excludes 0–8 base names, case-insensitive at any depth | `skip` and `nested/skip` absent; slash/colon rejected |
| 7 | Level 6, fixed 1980-01-01 epoch, sorted `/` entries | `zipfile` reports `ZIP_DEFLATED`, `(1980,1,1,0,0,0)`, sorted names |
| 8 | Manifest `path,size,sha256`, LF, full lowercase SHA-256 | oracle byte comparison + golden; 64-hex validated |
| 9 | stdout `files=N bytes=N archive_bytes=N` | exact bytes; `archive_bytes` equals the file size |
| 10 | Empty match set -> 5, no outputs | `.zzz` config; neither file exists |
| 11 | Verify same file set, sizes and SHA-256; `verified files=N` | success case; extra/missing/size/sha256 cases -> 6 |
| 12 | Sorted `MISMATCH <path> <reason>` lines | single and combined mismatch cases, byte-compared |
| 13 | Corrupt/unsupported archive -> 6 | truncated archive on verify and extract |
| 14 | Extract creates one level; parent must exist | parent-missing -> 4, destination-is-file -> 4 |
| 15 | Unsafe/absolute/traversal names -> 6, nothing written | `../`, `/`, `C:/`, nested traversal; no escape files |
| 16 | Extract round-trips; `extracted files=N` | 8/8 files byte-identical |
| 17 | Exit codes 0–6 | 0,1,2,3,4,5,6 all exercised |
| 18 | Diagnostics `releasepack: <label>`, no stdout before fatal | every failure case asserts empty or expected stdout |
| 19 | Deterministic output | two builds byte-identical; three repeated full builds identical |
| 20 | One standalone executable | 351,744-byte PE run outside the source tree |

## 4. Test evidence

`python benchmarks/m5/releasepack/tests/driver.py --no-build` ->
**51 passed, 0 failed, 0 skipped, 51 checks**, three consecutive runs, exit 0.

Regression goldens:

| Golden | SHA-256 |
| --- | --- |
| `expected/basic/manifest.csv` | `f3c942b27ad1fc82e5cd0b02c403884dfe501958c7d5c183a9983c51883fd544` |
| `expected/basic/archive.sha256` | `d68e6775c88f011099aa61f283e25e37c9fc0b1c26e1c3c78f47c36b2062bc58` (file digest) |
| recorded archive digest | `d68e6775c88f011099aa61f283e25e37c9fc0b1c26e1c3c78f47c36b2062bc58` |
| `expected/basic/stdout.txt` | `251202aa2fa092ece802c89267d66a82e9b82988e639d0d8ec377e7bf94c5ed0` |
| `expected/help.txt` | `89e0f105d52b85d9ca90669639464ba7e3c286bf42541efe291707f67152876e` |

`archive.sha256` contains the archive digest followed by one newline; the
digest is the same 64-character value as the table's archive row.

The basic build produces 8 entries / 196 source bytes / a 1,075-byte archive /
a 689-byte manifest. The manifest is quoted only for `comma,name.txt`; the
binary file, empty file, mixed-case extension, spaces, and excluded directories
are all covered.

## 5. Defects found and fixed (application, not MINK)

| Defect | Fix |
| --- | --- |
| one `rel` path leaked per built file (E-R06, exit 106) | released on the single loop exit |
| nested `rt_str_concat` temporaries in mismatch lines leaked | one `mismatch_line` helper with bound intermediates |
| duplicate manifest path set the error code but leaked its three fields | freed in that branch |
| a one-byte "matched" flag vector did not mark rows reliably (a ghost manifest row verified successfully) | replaced with a direct central-directory name scan |
| `fs_create_dir` creates intermediate directories, so a missing destination parent could be created | explicit parent directory check before creating the destination |
| a `root` containing `..` or an absolute root bypassed the string-based inside-root check | roots are now required to be plain relative paths (exit 4) |

All were application defects. No MINK compiler/runtime/stdlib defect was found.
The ZIP, hashing and CSV stdlib modules behaved as documented.

## 6. Standalone evidence

The driver copies the executable and a fixture tree into temporary directories
outside the repository, runs `build`, `verify` and `extract` from a working
directory there, checks the manifest/archive against the oracle, compares the
extracted bytes and asserts that nothing was created next to the executable.

Artifact: 351,744 bytes, SHA-256
`675acd35f7355c9b1797b395ffa0f5460d06845bb97ecbefc4623f921f5190b0`.

## 7. Commit

Only `benchmarks/m5/releasepack/**` and this record were staged. Pre-existing
M3 work was untouched. No reset, clean, stash, rebase, amend, force, push, tag
or publish was used.

## 8. Final status

**GREEN.** Every frozen `releasepack` requirement is implemented and exercised,
including all seven exit classes, deterministic archive bytes, independent
`zipfile` inspection, mismatch reporting and safe extraction. Session 169 may
now perform the adversarial closure audit.
