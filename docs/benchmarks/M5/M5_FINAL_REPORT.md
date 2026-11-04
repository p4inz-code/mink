# M5 — Final engineering report

**Date:** 2026-11-04
**Milestone:** M5 — Representative real-world applications
**Status:** **M5 PARTIAL**
**Starting HEAD:** `5e67649` (`docs: publish final M5 evidence report`)
**Gate:** [`M5_SESSION_172_READINESS_GATE.md`](M5_SESSION_172_READINESS_GATE.md)
**Scope of this report:** reporting and roadmap recommendation only. No
features, no MINK source change, no performance measurement, no push/tag/publish.

## 1. Executive summary

### Objective

M5 asked whether MINK's demonstrated advantages survive when the deliverable is
a complete Windows application with real input validation, filesystem or
network behavior, error handling, tests and a standalone executable. It is an
application-engineering evidence stage, not another five-language benchmark
suite.

### Applications completed

| Application | Domain | Contract | Result |
| --- | --- | --- | --- |
| `repoaudit` | offline project inventory and policy audit | section 5 | GREEN |
| `hookbox` | loopback HTTP webhook inbox | section 6 | PARTIAL |
| `releasepack` | deterministic ZIP + SHA-256 manifest, verify, extract | section 7 | GREEN |

All three are single native PE executables with no sidecar files and no runtime
dependency beyond Windows.

### Overall status

**M5 PARTIAL.** Two applications are GREEN and one is
APPLICATION GREEN / CONTRACT PARTIAL. The single unmet frozen clause is
`hookbox`'s acceptance of valid JSON bodies between 4,097 and 8,192 bytes,
which the stdlib JSON arena refuses. That is a genuine MINK capability gap, not
a test gap, and it is recorded rather than hidden.

### What the evidence demonstrates

- Three materially distinct Windows-native applications can be built with
  existing MINK standard-library modules and no compiler, runtime or stdlib
  change.
- Deterministic, byte-exact outputs are achievable across text reports, HTTP
  responses/event files and ZIP archives.
- All defined process exit classes, HTTP error statuses, failure cleanup and
  outside-source-tree execution are exercised by 163 executable checks per run.
- Independent Python oracles reproduce the contract semantics without deriving
  expectations from MINK output.

### What the evidence does not demonstrate

- Broad superiority over Rust, Go, C#, or Python. M4 owns the cross-language
  comparison; M5 adds no timing, throughput or memory measurement.
- Full satisfaction of the frozen `hookbox` 8 KiB body contract.
- External-developer usability. Only the original developer built, debugged and
  tested these applications.
- Concurrency, load, Unicode paths, TLS, durability, or large-scale operation.
- That the same results hold on any platform other than Windows x86_64.

## 2. Application summaries

### 2.1 `repoaudit`

- **Purpose:** audit an ASCII Windows project tree against a JSON policy and
  write deterministic CSV/JSON reports, exiting nonzero on violations.
- **Contract:** selection section 5, frozen in `8cdb33e`.
- **Implemented capabilities:** manual argv and `--help`; seven-key JSON schema
  validation; recursive walk with extension/exclude filtering and byte-order
  sorting; binary/NUL detection; LF-based line counting; case-sensitive marker
  counts; size and marker policies; RFC-4180 CSV; fixed-order summary JSON; exit
  codes 0-7.
- **Source:** `benchmarks/m5/repoaudit/app/main.mink`, 1,006 lines.
- **Tests:** 50 checks per run, 0 failed, 0 skipped. Oracle recomputes inventory,
  violations, summary and stdout independently.
- **Standalone build:** 216,576 bytes, SHA-256
  `3d1b15cf91f9afa8466a811c82355564660c716e753187c0bc4763e2a361b46a`.
- **Dependencies:** MINK standard library only; no network or third-party
  library.
- **Audit:** Session 165 (implementation Session 164). GREEN.
- **Limitations:** `markers[3..4]` accepted by the schema but the frozen report
  schema has no column for them; reports are replaced in place, not atomic; no
  Unicode path guarantee.

### 2.2 `hookbox`

- **Purpose:** a loopback-only HTTP webhook inbox that stores each accepted
  JSON object as a numbered event file and shuts down after a request budget.
- **Contract:** selection section 6, frozen in `8cdb33e`.
- **Implemented capabilities:** manual argv and `--help`; port bind on
  `127.0.0.1`; Content-Length request reading and reassembly; four endpoints;
  routing with method precedence; JSON object validation and compact
  re-serialization; six-digit sequence files with recovery from existing names;
  bounded `/events` windows; CRLF status lines on stdout; stderr diagnostics;
  exit codes 0-4 and HTTP 400/404/405/413/500.
- **Source:** `benchmarks/m5/hookbox/app/main.mink`, 796 lines.
- **Tests:** 60 checks per run, 0 failed, 0 skipped. Oracle implements its own
  JSON reader/writer, event format and response format; the driver speaks raw
  TCP.
- **Standalone build:** 230,912 bytes, SHA-256
  `6ac524fae3282caa62bdb0138841295480545c147319d5b67d2d750d9e993c77`.
- **Dependencies:** MINK standard library `httpserver` and `json` (plus their
  own filesystem/network dependencies); Python standard library for the oracle.
- **Audit:** Session 167 (implementation Session 166).
  APPLICATION GREEN / CONTRACT PARTIAL.
- **Limitations:** valid JSON bodies 4,097-8,192 bytes return `400` (stdlib
  arena); Windows case-insensitive event names; one connection at a time;
  Content-Length only; no TLS, keep-alive, chunked requests or Ctrl+C handler;
  ASCII paths only.

### 2.3 `releasepack`

- **Purpose:** build, verify and safely extract deterministic Windows release
  ZIP archives from a JSON configuration, with a sorted SHA-256 CSV manifest.
- **Contract:** selection section 7, frozen in `8cdb33e`.
- **Implemented capabilities:** three commands with exact arity; five-key JSON
  config validation; recursive selection with sorted `/` entries; DEFLATE level
  6 with the fixed 1980-01-01 DOS epoch; SHA-256 per file; RFC-4180 CSV
  manifest; verification by name, size and digest with sorted `MISMATCH` lines;
  validated extraction rejecting absolute, drive, UNC, ADS and `..` names; exit
  codes 0-6.
- **Source:** `benchmarks/m5/releasepack/app/main.mink`, 1,391 lines.
- **Tests:** 53 checks per run, 0 failed, 0 skipped. Oracle uses `hashlib`,
  `csv` and `zipfile`, and inspects archives independently of MINK's writer.
- **Standalone build:** 352,256 bytes, SHA-256
  `1ce414067003fe8a2c7924d3effc41a5e93f7a46597207ef08dc538cb1c8cadb`.
- **Dependencies:** MINK standard library `zip`, `zlib`, `hashing`, `csv`,
  `filesystem`; Python standard library for the oracle.
- **Audit:** Session 169 (implementation Session 168). GREEN.
- **Limitations:** no ZIP64, encryption, signing, permissions or timestamps
  beyond the fixed epoch; fully buffered (fixture target at most 200 KiB
  uncompressed); extraction is not transactional on a mid-run filesystem
  failure; output paths must be plain relative paths (stricter than the frozen
  text).

## 3. Consolidated verification

Counts in this table are **checks per full suite run**. Three runs were recorded
on 2026-11-03 (Sessions 170, 171, 173) and one independent re-run was executed
on 2026-11-04. Repeated runs execute the same check set and are not additional
cases.

| Application | Contract checks | Oracle-derived comparisons | Golden-output status | Audit | Standalone build | Final result |
| --- | ---: | --- | --- | --- | --- | --- |
| `repoaudit` | 50 | every deterministic case; reports, summary and stdout recomputed | byte-exact, committed | Session 165 | PASS, outside tree | GREEN |
| `hookbox` | 60 | every deterministic response/event; own JSON and HTTP model | byte-exact, committed; events LF, stdout/help CRLF | Session 167 | PASS, outside tree | PARTIAL |
| `releasepack` | 53 | every deterministic case; `hashlib`/`csv`/`zipfile` | byte-exact, committed; archive digest golden | Session 169 | PASS, outside tree | GREEN |
| **Total per run** | **163** | no shared oracle code with the applications | 100% byte-compared | 3 audits | 3 PEs | 2 GREEN, 1 PARTIAL |

Supporting counts, deliberately not added to the contract-check total:

- 3 recorded full-suite runs per application, 1 independent re-run: 4 runs of
  the 163-check set.
- Each application has 1 outside-source-tree execution case and at least 1
  repeated-determinism case inside its suite.
- Exit classes exercised: `repoaudit` 0-7, `hookbox` 0-4, `releasepack` 0-6.

No count above includes a performance measurement, because M5 has none.

## 4. Engineering findings

### Application structure

All three applications are single-file MINK programs with manual argument
parsing, explicit error labels, single-exit ownership and no module split. The
largest is 1,391 lines. Test drivers are larger than the applications in two
cases (hookbox 1,232 lines, releasepack 793) because they include an independent
oracle, process lifecycle management and byte comparison.

### Standard-library usability

The standard library covered every required capability without modification:
filesystem traversal and reads, JSON parse/serialize, CSV parse/write, SHA-256,
ZIP create/read/extract, TCP listen/accept and HTTP request parsing. No
application needed a third-party dependency or a platform shim. The single
capability shortfall is JSON input capacity.

### Filesystem and JSON workflows

Recursive ASCII traversal, binary detection, CRLF/LF handling, sorted output
and path filtering were reliable once ownership was handled explicitly.
Compact re-serialization preserved member order and duplicate keys, which made
event bytes and config diagnostics deterministic. JSON configuration worked
for all configs used; the arena ceiling appears only with large request bodies.

### HTTP and server workflows

The `httpserver` module supported a complete loopback server: bind, accept,
read a Content-Length request, route, respond and close, including a request
reassembled from three TCP writes. Recorded boundaries: one connection at a
time, Content-Length only, a 64 KiB request read cap, and no keep-alive,
chunked requests, TLS or console-control handler.

### Concurrency

No concurrent application behavior is verified. `hookbox` serves one connection
at a time by design, and the frozen contracts require no concurrency. M4 owns
the concurrency experiments; M5 makes no concurrency claim.

### Error handling

Every declared process exit class and every declared HTTP error status was
exercised. Failure paths produced no partial report, no partial event file and
no extracted files from an unsafe archive. Diagnostics stayed on stderr with the
application prefix, and no stack trace was observed.

### Build and distribution

Each application builds to one PE executable with the documented
`target/release/mink.exe build <source>` command, and each suite runs the
executable from a temporary directory outside the repository. `mink check`
passes all three sources (18,927 / 22,276 / 37,636 tokens). No sidecar files,
environment variables or installed runtimes are required.

### Determinism

All three produce byte-identical output across repeated runs: sorted reports,
fixed JSON key order, fixed event sequence and bytes, fixed ZIP level and epoch.
Line endings are deliberate and tested: LF for CSV, event files and manifests;
CRLF for stdout and help text; `summary.json` has no trailing newline.

### Practical developer experience (observed, not measured)

Writing these applications required explicit ownership and a single-exit style
to avoid leaks, and explicit boundary handling for the JSON arena and Windows
file-name case folding. The workflow itself is short: one source file, one
documented build command, one Python driver. These are observations from three
applications by the original developer, not a usability study.

## 5. Limitations and risks

### Language limitations

- No Unicode path or argv guarantee in the tested scope; all fixtures are ASCII.
- No console-control API, so a server has no graceful Ctrl+C path.
- JSON input is constrained by fixed arena geometry.

### Compiler, runtime and stdlib limitations

- `stdlib/json.mink` accepts a simple object body up to 4,096 bytes and refuses
  4,097-8,192 bytes even with the required write-pointer reset. This is the one
  contract-level gap.
- `MAX_LIVE_ALLOCS` is documented at 1,024 simultaneous allocations. M5 stayed
  well below it; the ceiling was not stressed.
- The HTTP server reads Content-Length requests up to 64 KiB, one connection at
  a time.

### Application-specific limitations

- `repoaudit`: markers 3-4 have no report column; reports are not atomic.
- `hookbox`: sequential handling; no retention/search; sequence names above six
  digits are theoretically unrepresentable.
- `releasepack`: buffered operation; non-transactional mid-extraction failure;
  stricter relative-output-path rule than the frozen text.

### Windows platform constraints

- File names are case-insensitive: `000026.JSON` and `000026.json` are the same
  file. Event recognition is byte-exact lowercase.
- Executables are x86_64 PE only; no other target was tested.
- Failure injection used a directory on the write-probe name and a removed
  inbox; disk-full and permission-denied were not separately simulated.

### External dependency constraints

- None at runtime beyond Windows.
- Test oracles require Python 3.11 standard library only.
- No package manager, network install or service account is involved.

### Test methodology limitations

- Oracles verify contract semantics independently but are not second MINK
  implementations.
- Determinism is checked on one machine; no cross-machine or clock-sensitive
  case exists.
- There is no load, soak, concurrency or timing test.
- The committed executables were hashed, not rebuilt, during this reporting
  session; prior sessions recorded their builds.
- No external developer has attempted the workflow.

## 6. Strategic conclusion

**Question:** can MINK make a meaningful class of native software substantially
simpler to build, test and ship than the alternatives?

**Evidence-backed answer:** for a narrow, well-defined class - single-executable
Windows x86_64 command-line tools and loopback services built only on the
standard library - MINK is already sufficient to deliver complete applications
with deterministic behavior, real validation, failure handling and standalone
distribution. Three applications across offline analysis, HTTP serving and
archive engineering were built without compiler or stdlib changes, and their
contracts are verified by 163 executable checks per run plus independent
oracles.

That is a meaningful result, but it is not a claim of broad superiority. M5
measured no performance, no startup, no memory and no cross-language
productivity. It did not test concurrency, large data, Unicode paths, TLS,
durability or third-party ecosystems. M4 provides the comparison evidence; M5
provides application-completeness evidence.

What MINK currently does well, on this evidence: compact single-file programs,
direct filesystem and byte control, deterministic artifacts, simple standalone
distribution and a standard library broad enough for these three domains.

Where it remains weaker on this evidence: JSON input capacity, Unicode/path
handling, concurrency, graceful shutdown, and any workflow requiring external
libraries or tooling. Whether an unfamiliar developer can adopt it is untested.

## 7. Roadmap recommendation

Locked roadmap: M0 Foundation, M1 representative Windows workloads, M2 deep
Windows x86_64 validation, M3 targeted gaps, M4 five-language comparison,
M5 representative applications, M6 external developer usability, M7
tooling/ecosystem demand, M8 strategic decision, M9 conditional platform
expansion.

**Recommendation: proceed to M6, scoped as an external-developer usability
stage, without expanding MINK's feature scope first.**

Rationale: the application set is complete (3 of 3) and every remaining
limitation is bounded. The largest missing evidence is not another workload but
whether someone who did not write these applications can understand, install,
build, debug, test and ship one from the documentation alone.

M6 should test exactly that, for example by having an independent developer
build and modify one existing M5 application using only committed docs and the
toolchain, and recording every point of friction. It should not add speculative
features.

Prerequisites and constraints for M6:

- Accept M5 as PARTIAL; do not relabel it.
- Decide explicitly whether to fix the JSON arena capacity or amend the
  `hookbox` contract. Either choice requires a regression-backed change; neither
  belongs inside a usability session.
- Keep M5's evidence intact; do not rewrite passing records.
- Do not begin M6 in this session.

M7 should evaluate real tooling and ecosystem demand rather than build
speculative infrastructure. M8 must make an evidence-backed strategic decision
using M4, M5 and M6 results. M9 remains conditional: nothing in M5 justifies
promising Linux or other platform support.

## 8. Final repository state

| Item | Value |
| --- | --- |
| Branch | `main` |
| HEAD at report start | `5e67649` (`docs: publish final M5 evidence report`) |
| Origin | `https://github.com/p4inz-code/mink` |
| `origin/main` | `b860837`; local `main` is 26 ahead / 1 behind |
| Worktree | 67 pre-existing M3/M4 entries unrelated to M5; M5 files clean before this report |
| M5-owned commits | `8cdb33e`, `2fe1d21`, `bcd3fbd`, `75c3b3b`, `6714c52`, `4a5b551`, `67a3e38`, `3a06b19`, `b5d4d85`, `6d859ff`, `5e67649`, plus this report's commit |
| Uncommitted work | only this report and the renamed readiness gate; pre-existing M3/M4 work left untouched |
| Test totals | 163 checks per full run: 50 `repoaudit`, 60 `hookbox`, 53 `releasepack`; 163 passed, 0 failed, 0 skipped on 2026-11-04 |
| Artifacts | `repoaudit.exe` 216,576 B `3d1b15cf...`; `hookbox.exe` 230,912 B `6ac524fa...`; `releasepack.exe` 352,256 B `1ce41406...` |
| Outstanding actions | resolve or formally amend the `hookbox` 4,097-8,192-byte JSON body gap; decide M6 scope |

No push, tag, publish, reset, clean, stash, rebase or amend was performed. The
Git log is authoritative for the final hash.

## 9. Record index

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
| [`M5_SESSION_173.md`](M5_SESSION_173.md) | Session 173 record |

## 10. Handoff

M5 closes as **PARTIAL** with the representative application set complete. The
next phase is a scoped M6 usability decision, not more M5 workloads. The one
technical prerequisite to decide first is whether the JSON arena capacity is
fixed or the `hookbox` body contract is formally amended; both require a
separate, regression-backed change.
