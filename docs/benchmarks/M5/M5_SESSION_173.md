# Session 173 — final M5 evidence report and roadmap decision

**Date:** 2026-11-03
**Decision:** **M5 PARTIAL**
**Scope:** final evidence report. No performance measurement, no language
comparison (M4 owns that), no MINK source change, no push/tag/publish.

## 1. Representative applications completed

| Application | Domain | Implementation | Contract |
| --- | --- | --- | --- |
| `repoaudit` | offline project inventory and policy audit | `benchmarks/m5/repoaudit/app/main.mink` | GREEN |
| `hookbox` | loopback HTTP webhook inbox | `benchmarks/m5/hookbox/app/main.mink` | PARTIAL |
| `releasepack` | deterministic ZIP + SHA-256 manifest, verify, extract | `benchmarks/m5/releasepack/app/main.mink` | GREEN |

All three are single native PE executables with no sidecars and no runtime
dependency beyond Windows.

## 2. Total validation cases

| Suite | Checks | Passed | Failed | Skipped |
| --- | ---: | ---: | ---: | ---: |
| `repoaudit` | 50 | 50 | 0 | 0 |
| `hookbox` | 60 | 60 | 0 | 0 |
| `releasepack` | 53 | 53 | 0 | 0 |
| **Total** | **163** | **163** | **0** | **0** |

Each suite was run three consecutive times during Sessions 171–173; every run
reported the same result and exit 0.

## 3. Status totals

- Applications: 2 GREEN, 1 PARTIAL, 0 FAILED, 0 BLOCKED.
- Contract clauses: all GREEN except the `hookbox` 4,097–8,192-byte valid
  JSON body clause.
- Session outcome: **M5 PARTIAL**, not COMPLETE and not BLOCKED.

## 4. Standalone executable evidence

| Application | Size | SHA-256 |
| --- | ---: | --- |
| `repoaudit.exe` | 216,576 | `3d1b15cf91f9afa8466a811c82355564660c716e753187c0bc4763e2a361b46a` |
| `hookbox.exe` | 230,912 | `6ac524fae3282caa62bdb0138841295480545c147319d5b67d2d750d9e993c77` |
| `releasepack.exe` | 352,256 | `1ce414067003fe8a2c7924d3effc41a5e93f7a46597207ef08dc538cb1c8cadb` |

Each suite copies the executable and fixtures into a temporary directory
outside the repository, runs the application there, and checks output bytes
and exit codes. `releasepack` was additionally copied with its fixture and run
manually: build `files=8 bytes=196 archive_bytes=1075`, verify
`verified files=8`, extract `extracted files=8`, all exit 0.

## 5. Repeated-run evidence

- `repoaudit`: 50/50 three times.
- `hookbox`: 60/60 three times; its determinism case also byte-compares three
  independent event sequences.
- `releasepack`: 53/53 three times plus two byte-identical builds per test run.

No flaky case, no order-dependent failure.

## 6. Independent-oracle evidence

- `repoaudit`: Python recomputation of inventory, lines, binary detection,
  marker policy, violations, summary and stdout.
- `hookbox`: oracle with its own JSON reader/writer, event format, HTTP
  response format and stdout format; the driver speaks raw TCP.
- `releasepack`: `hashlib`/`csv`/`zipfile` oracle; archives are inspected by
  Python's ZIP reader, not MINK's writer.

No oracle imports the application source or parses MINK output to derive an
expectation.

## 7. MINK capabilities exercised

Manual argv and validation; JSON configuration and JSON body parsing; compact
JSON/NDJSON serialization; recursive filesystem walks; binary/NUL detection;
line counting; CSV parsing and quoting; SHA-256; ZIP create/read/verify/extract;
TCP listening and HTTP/1.1 request parsing; routing; sequential request
handling; deterministic file naming; CRLF/LF byte control; non-zero policy and
failure exits; stderr/stdout separation; single-exit cleanup.

## 8. Genuine MINK limitations discovered

1. **JSON arena capacity.** `stdlib/json.mink` accepts a simple object body up
   to 4,096 bytes and refuses 4,097–8,192 bytes with the required write-pointer
   reset. This is the only contract-level gap. `stdlib/json.mink` was not
   modified.
2. **Windows file-name case folding.** `000026.JSON` and `000026.json` are the
   same file; the event-name rule is byte-exact lowercase and the next event
   overwrites an upper-case variant. Documented and kept off the live sequence.
3. **No console-control API.** `hookbox` has no Ctrl+C handler; OS termination
   is not claimed as graceful.
4. **Unicode paths.** No Unicode path guarantee; all scored fixtures are ASCII.

## 9. Application defects found and fixed

| Application | Defect | Resolution |
| --- | --- | --- |
| `repoaudit` | zero-capacity vector, unfreed summary/report paths, duplicate line ending | fixed in application before closure |
| `hookbox` | fatal `/events` read failure exited without a diagnostic | diagnostic added |
| `hookbox` | test asserted 405 as 404 and a request budget was off by one | tests corrected; precedence documented |
| `releasepack` | unmatched manifest row not detected (ghost row verified) | replaced flag vector with a name scan |
| `releasepack` | `fs_create_dir` created a missing destination parent | explicit parent check |
| `releasepack` | root with `..`/absolute bypassed the inside-root check | plain-relative root required |
| `releasepack` | archive and manifest could overwrite each other | same path rejected with exit 4 |
| M5 docs | Session 168 recorded the golden file's digest as the archive digest | corrected in Session 171 |
| M5 fixture | committed fixture lacked the `out/` parent | documented `mkdir -p out`; suite creates it |

No MINK compiler/runtime/stdlib defect was found beyond the arena ceiling.

## 10. Unresolved capability gaps

**One:** valid JSON request bodies of 4,097–8,192 bytes are refused by
`hookbox` with `400`. The frozen contract allows bodies up to 8,192 bytes.
Resolving it requires a MINK-side JSON arena change or an explicit contract
amendment; neither was authorized in M5. The gap is demonstrated by permanent
boundary tests and is visible in the application README, Sessions 166, 167,
170, 171 and 172.

## 11. Exact commits

| Commit | Work |
| --- | --- |
| `8cdb33e` | freeze three M5 application contracts |
| `2fe1d21` | implement and verify `repoaudit` |
| `bcd3fbd` | close `repoaudit` audit |
| `75c3b3b` | implement and verify `hookbox` |
| `6714c52` | audit `hookbox` adversarially |
| `4a5b551` | implement and verify `releasepack` |
| `67a3e38` | audit `releasepack` adversarially |
| `3a06b19` | cross-application matrix |
| `b5d4d85` | repository-wide M5 evidence audit |
| `6d859ff` | M5 readiness classification |
| this session's commit | this final report |

The Git log is authoritative for the final hash.

## 12. Git cleanliness for M5-owned files

- After the final commit, `git status --porcelain benchmarks/m5 docs/benchmarks/M5`
  is empty.
- Every path changed since `bcd3fbd` is under `benchmarks/m5/` or
  `docs/benchmarks/M5/`; `git diff bcd3fbd..HEAD -- src/ npm/mink/ target/` is
  empty.
- No executable, build directory, probe log or `__pycache__` is tracked.
- Pre-existing modified M3/M4 files in the working tree were not staged or
  changed by any M5 commit.
- No reset, clean, stash, rebase, amend, force, push, tag or publish was used.

## 13. Conclusion

**Does the accumulated evidence demonstrate that MINK can be used to build
representative Windows-native applications beyond isolated language/runtime
test cases?**

**Yes, within a bounded qualification.** Three standalone applications across
three distinct domains (offline policy/report tooling, a TCP/HTTP server, and
an archive/verification tool) were built with existing MINK modules, without
compiler or stdlib changes, and are backed by 163 executable checks, three
repeated runs, independent oracles, byte-exact goldens, deterministic outputs,
all defined exit classes, failure cleanup, and outside-tree execution.

The qualification is the `hookbox` JSON body range: the frozen 8 KiB contract
is not fully satisfiable, so **M5 is PARTIAL, not COMPLETE**. The representative
application set is complete and no further workload should be invented. The
remaining work is a MINK capability fix (JSON arena capacity) or a deliberate
contract amendment, followed by re-running the existing boundary cases.
