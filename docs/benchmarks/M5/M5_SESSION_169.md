# Session 169 — `releasepack` adversarial closure audit

**Date:** 2026-11-03
**Status:** **GREEN — 53/53 checks green, three consecutive full runs.**
**Contract:** section 7 of [`M5_APPLICATION_SELECTION.md`](M5_APPLICATION_SELECTION.md), frozen in Session 163.
**Starting HEAD:** `4a5b551` (`benchmarks: implement and verify M5 releasepack`).
**Scope:** adversarial audit of the Session 168 application only. No
performance measurement, no MINK source change, no `repoaudit`/`hookbox`
change.

## 1. Method

The frozen contract was re-read line by line and every clause was mapped to an
executable case in `tests/driver.py`. Nothing was accepted by inference: each
row below is asserted by running the standalone executable and comparing exit
code, stdout, stderr and bytes on disk. The oracle
(`tests/oracle.py`) was re-read to confirm it derives expectations from
`hashlib`, `csv` and `zipfile`, not from MINK output.

## 2. Line-by-line verification

| Contract area | Audit result | Evidence |
| --- | --- | --- |
| Commands and arity | exact; wrong forms exit 1 with stderr | 12 usage cases; no stdout on failure |
| `--help` | byte-exact golden, exit 0 | golden comparison |
| Config shape | exactly five keys, no extras; types/ranges enforced | 21 schema/syntax cases -> 3 |
| `root` | exists, is a directory, plain relative path | missing/file/absolute/`..` -> 4 |
| Output paths | parents exist, not inside root, plain relative, distinct | 6 path cases -> 4, nothing written |
| Selection | case-insensitive extensions, excludes at any depth | 8/11 fixture files selected; oracle walk agrees |
| Archive bytes | sorted `/` names, DEFLATE, fixed 1980 epoch, deterministic | Python `zipfile` inspection; two builds byte-identical |
| Manifest | header, sorted rows, RFC-4180 quoting, LF, full SHA-256 | byte-exact golden; quoted `comma,name.txt` |
| Build stdout | exact `files/bytes/archive_bytes`, CRLF | byte comparison; size equals the real file |
| Empty match | exit 5 and no outputs | asserted both paths absent |
| Verify success | same set/size/hash, `verified files=N`, empty stderr | exact stdout |
| Verify mismatches | extra/missing/size/sha256, sorted lines, exit 6 | combined and single cases, byte-exact |
| Verify corruption | truncated/foreign-invalid archive -> 6 | no MISMATCH invention for unreadable archives |
| Extract success | round-trip of all 8 files, `extracted files=N` | byte comparison of every file |
| Extract safety | traversal, absolute, drive, UNC-shaped names -> 6, nothing written | pre-validation asserted |
| Extraction errors | missing/corrupt archive, bad destination -> 2/4/6 | each exit asserted |
| Exit codes | 0,1,2,3,4,5,6 all exercised | see cases above |
| Diagnostics | `releasepack: <label>` on stderr, no stdout before fatal | every failure case asserts stdout |
| Determinism | three repeated full builds byte-identical | digest comparison |
| Standalone | run outside the repository from a temp cwd | driver case + manual run |
| Cleanup | no leaked handles/strings in the new code paths | manual single-exit review |

## 3. Adversarial cases added this session

| Case | Expected | Result |
| --- | --- | --- |
| Foreign archive written by Python `zipfile`, entries in reverse order, `ZIP_STORED` | verify 0, extract 0, all bytes round-trip | pass |
| Manifest with CRLF records | verify 0 | pass |
| `archive` and `manifest` set to the same path | exit 4, no output written | pass (after fix) |
| `root` containing `..` or an absolute root | exit 4 | pass |

The foreign-archive case is the important independence check: it proves
`verify` and `extract` read the ZIP central directory by name rather than
relying on MINK's own writer or entry order.

## 4. Defects found and fixed

| Defect | Severity | Fix |
| --- | --- | --- |
| `archive` and `manifest` could name the same file, so the manifest overwrote the archive and the command still exited 0 | application correctness | rejected with exit 4 and a regression case |
| `root` with `..` or an absolute root bypassed the string-based inside-root check (found while closing Session 168) | application safety | roots are required to be plain relative paths, exit 4 |
| a one-byte flag vector failed to mark unmatched manifest rows, so a ghost row verified successfully (found during Session 168 development) | application correctness | replaced with a direct central-directory name scan |
| test harness created `out/` unconditionally in a fixture that already had it | test-only | `mkdir(exist_ok=True)` |

No MINK compiler/runtime/stdlib defect was found. The ZIP reader accepted
foreign stored and out-of-order archives without changes.

## 5. Evidence

`python benchmarks/m5/releasepack/tests/driver.py --no-build` ->
**53 passed, 0 failed, 0 skipped, 53 checks**, exit 0, three consecutive runs.

Artifact: 352,256 bytes, SHA-256
`1ce414067003fe8a2c7924d3effc41a5e93f7a46597207ef08dc538cb1c8cadb`.

Standalone: the driver copies the executable and fixture tree to a temporary
directory outside the repository, runs `build`, `verify` and `extract` there,
compares all outputs to the oracle and asserts that nothing was created beside
the executable. The foreign-archive case also runs in that copied tree.

Golden integrity: `expected/basic/manifest.csv`,
`expected/basic/archive.sha256`, `expected/basic/stdout.txt` and
`expected/help.txt` are compared as raw bytes and are unchanged by this
session. `git diff --check` is clean.

## 6. Commit

Staged only `benchmarks/m5/releasepack/{app/main.mink,tests/driver.py,tests/oracle.py,README.md}`
and this record. No reset, clean, stash, rebase, amend, force, push, tag or
publish. Pre-existing M3 work was not touched.

## 7. Final status

**GREEN.** Every frozen `releasepack` clause is exercised by an executable
case, the two audit defects are fixed and covered by permanent regressions,
and the complete suite passes three times plus a standalone run outside the
source tree. Session 170 may now treat `releasepack` as closed.
