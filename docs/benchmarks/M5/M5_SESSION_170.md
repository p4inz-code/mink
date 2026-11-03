# Session 170 — cross-application M5 matrix

**Date:** 2026-11-03
**Status:** **AUDIT COMPLETE — 163/163 checks green across three applications.**
**Starting HEAD:** `67a3e38` (`benchmarks: audit M5 releasepack adversarially`).
**Scope:** selection check plus cross-application audit. No performance
measurement, no MINK source change, no new workload.

## 1. Is there a fourth application?

No. [`M5_APPLICATION_SELECTION.md`](M5_APPLICATION_SELECTION.md) evaluates
candidate workloads in section 4, selects exactly three in sections 5–7, and
describes the M5 gate in section 10. The three selected applications are
`repoaudit`, `hookbox` and `releasepack`; all three are now implemented and
audited. No further application is named in the document, the roadmap, or the
Session 163 freeze.

Per the session brief, no workload was invented. This session is therefore the
cross-application matrix audit.

## 2. Verified counts

Every count below was produced by running the committed suite in this
repository on 2026-11-03, not copied from a session record.

| Application | Command | Checks | Passed | Failed | Exit |
| --- | --- | ---: | ---: | ---: | ---: |
| `repoaudit` | `python benchmarks/m5/repoaudit/tests/run_tests.py --no-build` | 50 | 50 | 0 | 0 |
| `hookbox` | `python benchmarks/m5/hookbox/tests/driver.py --no-build` | 60 | 60 | 0 | 0 |
| `releasepack` | `python benchmarks/m5/releasepack/tests/driver.py --no-build` | 53 | 53 | 0 | 0 |
| **Total** | | **163** | **163** | **0** | |

`mink check` passes for all three sources. Each suite also contains its own
standalone executable case, repeated-run deterministic case, and byte-exact
golden comparisons.

## 3. Application matrix

| Application | Purpose | MINK capabilities exercised | Filesystem | JSON/data | Networking | Concurrency | Error handling | Determinism | Standalone | Validation cases | Known limitations | Status |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | ---: | --- | --- |
| `repoaudit` | Offline project inventory and policy audit | manual argv, JSON schema validation, recursive walk, physical-line counting, marker policy, CSV/JSON reports, exit-code policy | recursive tree, 40-file fixture budget, CRLF/LF/binary/oversized reads | config object, compact summary JSON, CSV writer with RFC-4180 quoting | none | none (sequential walk) | exits 0–7; stderr-only diagnostics; no reports on scan failure | sorted paths, sorted violations, fixed summary key order; 3 identical runs | 216,576-byte PE outside the tree | 50 | `markers[3..4]` accepted but no report column; no Unicode path guarantee; reports replaced in place, not atomic | **GREEN** |
| `hookbox` | Loopback-only HTTP webhook inbox | TCP listen/accept, HTTP/1.1 request parsing, routing, JSON body validation and re-serialization, event persistence, request budget | inbox creation/writability probe, six-digit event files, sequence recovery, cleanup | JSON object validation and compact serialization; NDJSON output; no stdlib change | **yes** — `127.0.0.1`, Content-Length requests, `Connection: close` | sequential: one connection at a time, no worker threads | exits 0–4; HTTP 400/404/405/413/500; no partial event on write failure | fixed event bytes, sequence, CRLF stdout; 3 identical runs | PE outside the tree; events under cwd | 60 | **valid JSON bodies 4,097–8,192 bytes return 400** (stdlib arena ceiling); Windows case-insensitive event names; no keep-alive/chunked/TLS | **APPLICATION GREEN / CONTRACT PARTIAL** |
| `releasepack` | Deterministic release ZIP + SHA-256 manifest, verify and extract | JSON config schema, recursive selection, ZIP write/read/extract, SHA-256, CSV, path safety | tree walk; output-parent checks; extraction | five-key config; sorted CSV manifest; SHA-256; Python `zipfile` interop | none | none (sequential build/verify/extract) | exits 0–6; sorted `MISMATCH` lines; pre-validated extraction | sorted entries, fixed level 6 and 1980 epoch, byte-identical rebuilds | 352,256-byte PE outside the tree | 53 | no ZIP64/encryption/signing; fully buffered; extraction can leave earlier files on a mid-run filesystem failure; no Unicode path guarantee; outputs must be plain relative paths (stricter) | **GREEN** |

The concurrency column is deliberately explicit: M5 includes one network
server but no concurrent worker execution, and the frozen contracts do not
require it. M4 owns the concurrency experiments.

## 4. Capability coverage

| Capability | `repoaudit` | `hookbox` | `releasepack` |
| --- | --- | --- | --- |
| Manual argument parsing | yes | yes | yes |
| JSON configuration | yes | CLI options only | yes |
| JSON body parsing | no | yes (≤ 4,096-byte valid object) | no |
| Recursive filesystem walk | yes | inbox scan (one level) | yes |
| Binary-safe file reads | yes (NUL probe) | request bytes | yes (ZIP payloads) |
| File writes with cleanup | reports | event files | archive + manifest |
| TCP server | no | yes | no |
| CSV output | yes | no | yes |
| ZIP create/read/extract | no | no | yes |
| SHA-256 | no | no | yes |
| Non-zero policy exit | yes (6) | no | yes (6) |
| Distinct failure exit classes | 0–7 | 0–4 | 0–6 |
| Independent oracle | Python reimplementation | own JSON/HTTP oracle | `hashlib`/`csv`/`zipfile` |
| Standalone outside repo | yes | yes | yes |

## 5. Cross-application findings

1. **No MINK source change was needed for any application.** The three
   applications use existing `filesystem`, `json`, `csv`, `strings`,
   `hashing`, `zip`, `zlib` and `httpserver` modules.
2. **The only contract-level gap is hookbox's 4,097–8,192-byte JSON body
   range**, caused by the fixed JSON arena geometry. It is demonstrated,
   bounded, and visible in the application README, Session 166, Session 167 and
   Session 172. `repoaudit` and `releasepack` are fully GREEN.
3. **Determinism is achieved by construction in all three**: sorted paths and
   fixed key order (`repoaudit`), fixed sequence/bytes (`hookbox`), fixed epoch
   and level with sorted entries (`releasepack`).
4. **Standalone evidence is consistent**: each suite copies the executable and
   fixtures outside the repository and runs there; no sidecar files are needed.
5. **The three applications are genuinely distinct**, matching the selection
   rationale: policy/report tool, network server, archive tool. None is a
   relabeled M4 workload.
6. **The 163 checks are not evenly distributed by accident**: 50
   (`repoaudit`), 60 (`hookbox`), 53 (`releasepack`). The counts differ because
   each contract has a different surface (8 exit classes, 4 endpoints, 3
   commands).

## 6. Limitations carried into the gate

| Limitation | Application | Classification |
| --- | --- | --- |
| Valid JSON bodies 4,097–8,192 bytes -> 400 | `hookbox` | genuine MINK stdlib limit; contract PARTIAL |
| `markers[3..4]` have no frozen report column | `repoaudit` | frozen-contract ambiguity, documented in Session 163/165 |
| No Unicode path guarantee | all three | scoped by ASCII fixtures; documented |
| Reports not written atomically | `repoaudit` | contract allows "atomically enough for this scope" |
| Extraction is not transactional on mid-run FS failure | `releasepack` | documented; unsafe archives are pre-validated |
| Sequential-only networking | `hookbox` | contract does not require concurrency |

None of these is hidden, and none was converted into a claimed GREEN item.

## 7. Git state

Since `bcd3fbd`, every changed path is under `benchmarks/m5/` or
`docs/benchmarks/M5/`; `src/` and `npm/mink/` have no diff in that range. The
pre-existing modified M3/M4 files in the working tree were not touched by any
M5 commit.

## 8. Final status

**MATRIX AUDIT COMPLETE.** All three frozen applications are implemented and
green; the aggregate is 163/163. The only contract-level gap remains
`hookbox`'s documented JSON body range. Session 171 now audits repository-wide
M5 evidence for accidental artifacts, stale claims and line-ending drift.
