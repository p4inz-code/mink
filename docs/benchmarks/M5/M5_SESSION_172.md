# Session 172 — M5 readiness / closure gate

**Date:** 2026-11-03
**Decision:** **M5 PARTIAL**
**Scope:** formal readiness classification of the three frozen applications.
No performance measurement, no MINK source change, no contract change.

## 1. Evidence required per application

`✓` means demonstrated by the committed suite; `partial` means the application
is green but a frozen contract clause exceeds a MINK capability.

| Evidence | `repoaudit` | `hookbox` | `releasepack` |
| --- | --- | --- | --- |
| Contract defined and frozen | ✓ section 5 | ✓ section 6 | ✓ section 7 |
| Implementation complete | ✓ `app/main.mink` | ✓ `app/main.mink` | ✓ `app/main.mink` |
| Validation suite complete | ✓ 50 checks | ✓ 60 checks | ✓ 53 checks |
| Oracle independent | ✓ Python recomputation | ✓ own JSON/HTTP oracle | ✓ `hashlib`/`csv`/`zipfile` |
| Success paths tested | ✓ | ✓ | ✓ |
| Failure paths tested | ✓ exits 1–5, 7 | ✓ exits 1–4, HTTP 4xx/5xx | ✓ exits 1–6 |
| Boundary paths tested | ✓ size/marker/NUL/line boundaries | ✓ 4,096/4,097/8,192/8,193; limits 1/20 | ✓ empty match, limits, CSV quoting |
| Deterministic output tested | ✓ 3 identical runs | ✓ 3 identical runs | ✓ byte-identical rebuilds |
| Repeated-run evidence | ✓ 3 full suite runs | ✓ 3 full suite runs | ✓ 3 full suite runs |
| Standalone executable | ✓ 216,576-byte PE | ✓ 230,912-byte PE | ✓ 352,256-byte PE |
| Outside-source-tree execution | ✓ | ✓ | ✓ |
| Resource cleanup | ✓ no reports on failure | ✓ no partial event/probe | ✓ no output on failure |
| Known limitations documented | ✓ README + Session 165 | ✓ README + Sessions 166/167 | ✓ README + Sessions 168/169 |
| Session record | ✓ 164/165 | ✓ 166/167 | ✓ 168/169 |
| Commit evidence | ✓ `2fe1d21`, `bcd3fbd` | ✓ `75c3b3b`, `6714c52` | ✓ `4a5b551`, `67a3e38` |
| **Classification** | **GREEN** | **PARTIAL** | **GREEN** |

## 2. Per-application classification

### `repoaudit` — GREEN

All 32 frozen clauses are GREEN, including the seven-key schema, recursive
inventory, binary/oversized handling, marker policy, three reports, and exits
0–7. The `markers[3..4]` schema ambiguity is a frozen-contract limitation
recorded in Session 163/165, not a missing implementation. Standalone and
repeated-run evidence are present. No unresolved correctness defect.

### `hookbox` — PARTIAL

Application behavior is GREEN: CLI, all four endpoints, routing precedence,
event naming and recovery, request budget, stdout/stderr separation, exits 0–4,
and HTTP 400/404/405/413/500 are exercised. **The frozen contract's 8,192-byte
valid-JSON body clause is only met through 4,096 bytes.** The stdlib JSON arena
refuses well-formed bodies at 4,097 bytes; the application maps that range to
the contract's malformed-body `400`, keeps the 8,192-byte limit, and does not
modify `stdlib/json.mink`. This is a genuine MINK capability gap, not a test
gap, and it prevents a GREEN contract classification.

### `releasepack` — GREEN

All frozen clauses are GREEN: schema, selection, deterministic ZIP (level 6,
1980 epoch, sorted entries), CSV manifest, `verify` by name with sorted
mismatches, safe extraction, and exits 0–6. Foreign stored/out-of-order archives
verify and extract correctly. Documented boundaries (no ZIP64/encryption,
buffered operation, non-transactional mid-extraction failure, ASCII paths) do
not violate the frozen contract.

## 3. Gate result

| Gate | Condition | Result |
| --- | --- | --- |
| A. M5 READY FOR CLOSURE | all required applications complete and every limitation bounded | **not met** — one frozen `hookbox` clause is unmet |
| B. M5 BLOCKED | a required application cannot be completed | **not met** — all three are implemented and runnable |
| C. M5 PARTIAL | representative set substantially complete, known capability gap prevents full contract closure | **MET** |

**Decision: M5 PARTIAL.** The representative set is complete (3 of 3
applications, 163 checks, all green), and every limitation is explicitly
bounded. M5 is not declared COMPLETE because `hookbox`'s 8 KiB body contract
is not fully satisfiable with the current stdlib JSON arena.

The gap must not be closed by weakening the contract, deleting the boundary
test, or editing `stdlib/json.mink` without authorization. The correct next
step is a MINK-side JSON arena change (or an explicit contract amendment),
which is outside this session's scope.

## 4. Evidence statement

- Cumulative suites: 50 + 60 + 53 = **163 checks**, three consecutive runs
  each, **163 passed / 0 failed / 0 skipped** per run.
- Aggregate application status: 2 GREEN, 1 PARTIAL, 0 FAILED, 0 BLOCKED.
- No performance data is used or claimed.
- No MINK source changed: `git diff bcd3fbd..HEAD -- src/ npm/mink/` is empty.

## 5. Final status

**M5 PARTIAL.** Session 173 records the final evidence report and the exact
remaining gap; it does not claim COMPLETE.
