# M5 — Final engineering report

**Date:** 2026-11-04 (revised after Sessions 174-175)
**Milestone:** M5 — Representative real-world applications
**Status:** **M5 COMPLETE**
**Revision HEAD:** `aeceee1` (`fix: make JSON arena capacity caller-selectable`)
**Gate:** [`M5_SESSION_172_READINESS_GATE.md`](M5_SESSION_172_READINESS_GATE.md)
**Scope:** reporting and roadmap recommendation only. No performance
measurement, no push/tag/publish.

## 1. Executive summary

M5 asked whether MINK's demonstrated advantages survive when the deliverable is
a complete Windows application with real input validation, filesystem or
network behavior, error handling, tests and a standalone executable.

Three frozen applications were delivered: `repoaudit` (offline policy audit),
`hookbox` (loopback HTTP webhook inbox) and `releasepack` (deterministic ZIP +
SHA-256 manifest). All three are **GREEN**, all 163 application checks pass per
full run, and each ships as one native PE executable.

The milestone initially closed as PARTIAL because `hookbox` could not accept
valid JSON bodies of 4,097-8,192 bytes: the stdlib parser hardcoded its write
pointer at slot 8191 of a 64 KiB geometry. Sessions 174-175 closed that gap
with a capacity-aware `json_parse_in(input, arena, slots)` entry point, moved
`hookbox` to its real 32,768-slot arena, and added permanent boundary
regressions. The formerly open range is now accepted with exact event bytes.

### What the evidence demonstrates

- Three materially distinct Windows-native applications can be built with
  existing MINK standard-library modules.
- Deterministic, byte-exact outputs are achievable across text reports, HTTP
  responses/event files and ZIP archives.
- All defined process exit classes, HTTP error statuses, failure cleanup and
  outside-source-tree execution are exercised by 163 executable checks per run.
- Independent Python oracles reproduce contract semantics without deriving
  expectations from MINK output.
- A genuine standard-library limitation was isolated, fixed with a
  backward-compatible API, and covered by permanent tests.

### What the evidence does not demonstrate

- Broad superiority over Rust, Go, C#, or Python. M4 owns the cross-language
  comparison; M5 adds no timing, throughput or memory measurement.
- External-developer usability. Only the original developer built, debugged and
  tested these applications; M6 is designed to test that separately.
- Concurrency, load, Unicode paths, TLS, durability, or large-scale operation.
- That the same results hold on any platform other than Windows x86_64.

## 2. M5 objective and result

Objective: move beyond isolated language/runtime tests to complete,
useful applications with real I/O, validation, error handling and standalone
distribution.

Result: 3 of 3 frozen applications complete, 3 GREEN, 163 checks per run, no
open in-scope defect. M5 is COMPLETE.

## 3. Application summaries

### 3.1 `repoaudit`

- **Purpose:** audit an ASCII Windows project tree against a JSON policy and
  write deterministic CSV/JSON reports, exiting nonzero on violations.
- **Contract:** selection section 5, frozen in `8cdb33e`.
- **Capabilities:** manual argv and `--help`; seven-key JSON schema validation;
  recursive walk with extension/exclude filtering and byte-order sorting;
  binary/NUL detection; LF-based line counting; case-sensitive marker counts;
  size and marker policies; RFC-4180 CSV; fixed-order summary JSON; exits 0-7.
- **Source:** `benchmarks/m5/repoaudit/app/main.mink`, 1,006 lines.
- **Tests:** 50 checks per run, 0 failed, 0 skipped; independent oracle.
- **Artifact:** 216,576 bytes, SHA-256
  `3d1b15cf91f9afa8466a811c82355564660c716e753187c0bc4763e2a361b46a`.
- **Dependencies:** MINK standard library only.
- **Audit:** Session 165 (implementation 164). GREEN.
- **Limitations:** markers 3-4 have no frozen report column; reports not atomic;
  no Unicode path guarantee.

### 3.2 `hookbox`

- **Purpose:** loopback HTTP webhook inbox that stores each accepted JSON object
  as a numbered event file and stops after a request budget.
- **Contract:** selection section 6, frozen in `8cdb33e`.
- **Capabilities:** CLI/help; loopback bind; Content-Length requests including
  reassembly across TCP writes; four endpoints; method-before-query routing;
  JSON object validation and compact serialization; six-digit sequence files
  with restart recovery; bounded `/events`; CRLF status lines; exit codes 0-4
  and HTTP 400/404/405/413/500.
- **Source:** `benchmarks/m5/hookbox/app/main.mink`, 796 lines.
- **Tests:** 60 checks per run, 0 failed, 0 skipped, three consecutive runs;
  independent JSON/HTTP oracle and raw-TCP driver.
- **Boundary:** valid 4,096, 4,097 and 8,192-byte objects -> `201` with exact
  event bytes; 8,193 bytes -> `413`.
- **Artifact:** 231,424 bytes, SHA-256
  `a7c4be44a67f7f0dbb54372e4386fd6b862a842f3593d83e0a00add078a48e25`.
- **Dependencies:** MINK stdlib `httpserver`, `json`, `filesystem`, `strings`.
- **Audit:** Session 167; closure Session 175. GREEN.
- **Limitations:** sequential single-connection HTTP; Content-Length only; no
  keep-alive, chunked requests, TLS or Ctrl+C handler; ASCII path scope;
  Windows case-insensitive event names.

### 3.3 `releasepack`

- **Purpose:** build, verify and safely extract deterministic Windows release
  ZIP archives from JSON config, with a sorted SHA-256 CSV manifest.
- **Contract:** selection section 7, frozen in `8cdb33e`.
- **Capabilities:** three commands with exact arity; five-key config validation;
  recursive selection with sorted `/` entries; DEFLATE level 6 and fixed
  1980-01-01 epoch; SHA-256; RFC-4180 manifest; verify by name/size/digest with
  sorted `MISMATCH` lines; validated extraction rejecting unsafe names; exits
  0-6.
- **Source:** `benchmarks/m5/releasepack/app/main.mink`, 1,391 lines.
- **Tests:** 53 checks per run, 0 failed, 0 skipped; `hashlib`/`csv`/`zipfile`
  oracle.
- **Artifact:** 352,256 bytes, SHA-256
  `1ce414067003fe8a2c7924d3effc41a5e93f7a46597207ef08dc538cb1c8cadb`.
- **Dependencies:** MINK stdlib `zip`, `zlib`, `hashing`, `csv`, `filesystem`.
- **Audit:** Session 169 (implementation 168). GREEN.
- **Limitations:** no ZIP64/encryption/signing; fully buffered; extraction not
  transactional on mid-run filesystem failure; relative output paths required.

## 4. Contract verification

| Contract area | `repoaudit` | `hookbox` | `releasepack` |
| --- | --- | --- | --- |
| CLI, help, arity | PASS | PASS | PASS |
| Input/config validation | PASS | PASS | PASS |
| Core workflow | PASS | PASS | PASS |
| Exact output bytes | PASS | PASS | PASS |
| Exit codes | 0-7 | 0-4 | 0-6 |
| HTTP status behavior | n/a | 400/404/405/413/500 | n/a |
| Boundary behavior | PASS | PASS (4,096/4,097/8,192/8,193) | PASS |
| Failure cleanup | PASS | PASS | PASS |
| Standalone execution | PASS | PASS | PASS |
| **Result** | **GREEN** | **GREEN** | **GREEN** |

## 5. Independent oracle results

- `repoaudit`: Python recomputes inventory, line/binary/marker semantics,
  violations, summary and stdout; every deterministic case compares bytes.
- `hookbox`: oracle implements its own JSON reader/writer, event format, HTTP
  response format and stdout format; the driver speaks raw TCP and compares
  event files.
- `releasepack`: `hashlib`/`csv`/`zipfile`; archives are inspected by Python's
  ZIP reader, not MINK's writer; deterministic builds compared byte-for-byte.

No oracle imports the application source or parses MINK output to derive an
expectation. No count above treats supporting infrastructure as a scored case.

## 6. Golden-output results

Byte-exact goldens are committed for all three applications and compared on
every run: CSV/JSON reports, stdout and help (`repoaudit`); event files,
NDJSON, stdout and help (`hookbox`); manifest, archive digest, stdout and help
(`releasepack`). Line endings are deliberate: LF for CSV, events and manifests;
CRLF for stdout and help; `summary.json` has no trailing newline.

## 7. Standalone executable evidence

| Application | Size (bytes) | SHA-256 |
| --- | ---: | --- |
| `repoaudit.exe` | 216,576 | `3d1b15cf91f9afa8466a811c82355564660c716e753187c0bc4763e2a361b46a` |
| `hookbox.exe` | 231,424 | `a7c4be44a67f7f0dbb54372e4386fd6b862a842f3593d83e0a00add078a48e25` |
| `releasepack.exe` | 352,256 | `1ce414067003fe8a2c7924d3effc41a5e93f7a46597207ef08dc538cb1c8cadb` |

Each suite copies the executable and fixtures into a temporary directory
outside the repository and runs there. No sidecar files are required.

## 8. Dependencies

- Runtime: Windows x86_64 only; no third-party runtime, service or package.
- Build: the repository's `target/release/mink.exe` (version 1.0.4).
- Tests: Python 3.11 standard library; `cargo test --test json` for stdlib
  regressions.
- No application imports a non-MINK library.

## 9. Defect history

| Application / area | Defect | Resolution |
| --- | --- | --- |
| `repoaudit` | zero-capacity vector, unfreed summary/report paths, duplicate line ending | fixed before closure |
| `hookbox` | fatal `/events` read failure without diagnostic | diagnostic added |
| `hookbox` | driver asserted 405 as 404; request budget off by one | test corrected; precedence documented |
| `hookbox` / stdlib | valid 4,097-8,192-byte bodies refused | `json_parse_in` capacity API; permanent 8,192-byte regressions |
| `releasepack` | unmatched manifest row not detected | central-directory name scan |
| `releasepack` | missing destination parent created implicitly | explicit parent check |
| `releasepack` | `..`/absolute root bypassed inside-root check | plain relative root required |
| `releasepack` | archive and manifest could overwrite each other | same-path rejection |
| M5 docs | wrong archive digest recorded in Session 168 | corrected in Session 171 |
| M5 fixture | releasepack fixture lacked `out/` parent | documented `mkdir -p out` |

## 10. Compiler, runtime and stdlib findings

- The only capability gap found in M5 was the JSON arena geometry. It is a
  standard-library limitation, not a compiler or runtime defect.
- It was fixed by adding a capacity-aware parser entry point while keeping
  `json_parse` byte-for-byte compatible; 68 JSON tests pass, including the
  existing 4,091-byte refusal case.
- No compiler or runtime source was changed in M5. `mink --version` remains
  1.0.4.
- The fix landed the pending Session 138 JSON arena guard and serializer-base
  corrections that the capacity path depends on; both are covered in
  `tests/json.rs`.

## 11. Developer-experience findings

Observed, not measured: single-file programs with manual argv parsing and
single-exit ownership; explicit boundary handling for arena capacity and
Windows file-name case folding; one documented build command per application;
Python drivers larger than the applications in two cases because they carry an
independent oracle and process lifecycle management. The JSON capacity API is
the one place where an application needed a stdlib change rather than a
workaround.

## 12. Reliability findings

Failure paths produced no partial report, no partial event file and no files
from an unsafe archive. Diagnostics stayed on stderr with the application
prefix; no stack trace was observed. Listeners are closed before exit; no probe
or temp file remains after a clean run.

## 13. Limitations

### Language

- No Unicode path or argv guarantee in the tested scope.
- No console-control API (no graceful Ctrl+C).
- JSON capacity is explicit, not unbounded.

### Compiler / runtime / stdlib

- `json_parse` retains the historical ~3.5 KiB practical limit by design;
  callers needing more must use `json_parse_in` and a larger arena.
- `MAX_LIVE_ALLOCS` is documented at 1,024; M5 stayed well below it.
- HTTP requests are Content-Length, one connection at a time, 64 KiB read cap.

### Application

- `repoaudit`: markers 3-4 have no report column; reports not atomic.
- `hookbox`: sequential handling; no retention/search; six-digit sequence
  naming.
- `releasepack`: buffered; extraction not transactional on mid-run failure;
  stricter relative-path rule.

### Platform / external

- Windows x86_64 only; file names are case-insensitive.
- No external runtime or library dependency; Python is test-only.
- Disk-full and permission-denied were not separately simulated.

### Test methodology

- Oracles verify contract semantics but are not second MINK implementations.
- Determinism is checked on one machine; no load, soak, timing or concurrency
  tests.
- The committed `repoaudit` and `releasepack` executables were not rebuilt
  during this revision; `hookbox` was rebuilt and re-verified three times.
- No external developer has attempted the workflow.

## 14. Strategic conclusion

**Question:** can MINK make a meaningful class of native software substantially
simpler to build, test and ship than the alternatives?

**Evidence-backed answer:** for single-executable Windows x86_64 command-line
tools and loopback services built only on the standard library, MINK is
sufficient to deliver complete applications with deterministic behavior, real
validation, failure handling and standalone distribution. Three applications
across offline analysis, HTTP serving and archive engineering were built and
verified by 163 executable checks per run plus independent oracles; the one
contract-level gap found was closed in the standard library with regression
coverage.

This is not a claim of broad superiority. M5 measured no performance, startup,
memory or productivity. It did not test concurrency, large data, Unicode paths,
TLS, durability or third-party ecosystems. M4 owns comparison evidence; M5 owns
application-completeness evidence. Whether an unfamiliar developer can adopt
MINK remains untested and is the purpose of M6.

## 15. M6 recommendation

Proceed to M6, external-developer usability, without expanding feature scope.
M6 should test whether someone who did not write these applications can
understand, install, build, debug, test and ship one from committed
documentation alone, recording every point where hidden knowledge is required.
If a genuinely independent participant is unavailable, the result must be
labeled an internal cold-start simulation.

The M6 sessions in the handoff were not started because this block was used to
close M5. See [`../M6/M6_DEFERRED_SESSIONS.md`](../M6/M6_DEFERRED_SESSIONS.md).

M7 should evaluate real tooling/ecosystem demand, M8 must make an evidence-based
strategic decision, and M9 remains conditional: nothing in M5 justifies
promising Linux or other platform support.

## 16. Repository state

| Item | Value |
| --- | --- |
| Branch | `main` |
| HEAD at report revision | `aeceee1` |
| Origin | `https://github.com/p4inz-code/mink` |
| `origin/main` | `b860837`; local `main` ahead, not pushed |
| Worktree | pre-existing M3/M4 entries unrelated to M5 remain; M5 files committed |
| M5 commits | `8cdb33e`, `2fe1d21`, `bcd3fbd`, `75c3b3b`, `6714c52`, `4a5b551`, `67a3e38`, `3a06b19`, `b5d4d85`, `6d859ff`, `5e67649`, `d3f2211`, `aeceee1`, plus this report's commit |
| Test totals | 163 application checks per full run (50/60/53), 3 hookbox runs post-fix; 68 stdlib JSON tests |
| Outstanding actions | start M6 test design; no open M5 defect |

No push, tag, publish, reset, clean, stash, rebase or amend was performed. The
Git log is authoritative for the final hash.

## 17. Record index

| Record | Contents |
| --- | --- |
| [`M5_APPLICATION_SELECTION.md`](M5_APPLICATION_SELECTION.md) | frozen selection and contracts |
| [`M5_SESSION_163.md`](M5_SESSION_163.md) | contract freeze |
| [`M5_SESSION_164.md`](M5_SESSION_164.md), [`M5_SESSION_165.md`](M5_SESSION_165.md) | `repoaudit` implementation and audit |
| [`M5_SESSION_166.md`](M5_SESSION_166.md), [`M5_SESSION_167.md`](M5_SESSION_167.md) | `hookbox` implementation and audit |
| [`M5_SESSION_168.md`](M5_SESSION_168.md), [`M5_SESSION_169.md`](M5_SESSION_169.md) | `releasepack` implementation and audit |
| [`M5_SESSION_170.md`](M5_SESSION_170.md) | cross-application matrix |
| [`M5_SESSION_171.md`](M5_SESSION_171.md) | repository-wide evidence audit |
| [`M5_SESSION_172_READINESS_GATE.md`](M5_SESSION_172_READINESS_GATE.md) | closure gate and decision |
| [`M5_SESSION_173.md`](M5_SESSION_173.md) | original final report record |
| [`M5_SESSION_174_JSON_ARENA_CAPACITY.md`](M5_SESSION_174_JSON_ARENA_CAPACITY.md) | blocker reproduction, root cause and fix |
| [`M5_SESSION_175_CONTRACT_CLOSURE.md`](M5_SESSION_175_CONTRACT_CLOSURE.md) | boundary and full-suite re-verification |
