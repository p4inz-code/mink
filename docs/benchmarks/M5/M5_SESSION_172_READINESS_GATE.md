# Session 172 — M5 readiness and closure gate

**Date:** 2026-11-04 (first classified 2026-11-03 in `6d859ff`; revised after
Sessions 174-175 closed the JSON arena blocker)
**Decision:** **M5 COMPLETE**
**Scope:** formal readiness classification of the three frozen M5 applications.
No performance measurement, no contract change.
**Revision HEAD:** `aeceee1` (JSON arena capacity fix)
**Contract:** [`M5_APPLICATION_SELECTION.md`](M5_APPLICATION_SELECTION.md),
sections 5-7, frozen in Session 163.

## 1. Method

- Re-read the frozen selection document and every M5 session record.
- Re-ran all three committed suites against the committed executables:
  50 + 60 + 53 = 163 checks, all green, exit 0.
- Verified the boundary that previously blocked closure: 4,096, 4,097 and
  8,192-byte JSON objects now return `201` with exact event bytes; 8,193 bytes
  returns `413`.
- Ran `cargo test --test json` (68 tests) after the capacity-aware parser
  change.
- Re-hashed the executables and goldens against recorded values.
- Checked the Git range for changes outside M5 ownership.

## 2. Closure checklist

Status vocabulary: PASS, FAIL, PARTIAL, BLOCKED, NOT APPLICABLE.

### 2.1 Application completeness

| Requirement | Status | Evidence |
| --- | --- | --- |
| Every required M5 application implemented or dispositioned | PASS | 3 of 3 frozen applications: `repoaudit`, `hookbox`, `releasepack` |
| Every application has a frozen contract | PASS | selection sections 5, 6, 7 (commit `8cdb33e`) |
| Every application has independent verification | PASS | Python oracles + drivers, 163 checks per full run |
| Every application has an audit record | PASS | Sessions 165, 167, 169; closure in 174/175 |
| Every application has a standalone build | PASS | 216,576 / 231,424 / 352,256-byte PEs, run outside the tree |
| No application invented beyond the frozen selection | PASS | Session 170 records that no fourth application is named |

### 2.2 Correctness and reliability

| Requirement | Status | Evidence |
| --- | --- | --- |
| All required tests pass | PASS | 163/163; hookbox 60/60 three consecutive runs after the fix |
| No known in-scope application defect remains unresolved | PASS | all defects found in Sessions 166-169 and 174 were fixed and covered |
| Boundary behavior documented | PASS | hookbox 4,096/4,097/8,192/8,193; repoaudit size/marker/NUL/line; releasepack empty set and limits |
| Error handling verified | PASS | exits 0-7, 0-4, 0-6 plus HTTP 400/404/405/413/500 actually exercised |
| Resource ownership audited | PASS | single-exit paths; no partial reports/events/extractions on the tested failures |
| Deterministic output verified where required | PASS | three byte-identical runs per suite; fixed ZIP epoch/level |
| Golden outputs match exactly | PASS | byte comparison in every suite; goldens re-hashed |
| Every frozen hookbox clause is met | PASS | 8,192-byte valid objects return `201`; capacity-aware parse verified at 4,096/4,097/8,192/8,193 |

### 2.3 Evidence quality

| Requirement | Status | Evidence |
| --- | --- | --- |
| Results are reproducible | PASS | documented commands re-executed 2026-11-04 |
| Test counts are exact | PASS | 50 / 60 / 53 = 163 per full run; 68 stdlib JSON tests separately |
| Commands are documented | PASS | three application READMEs and session records |
| Binary hashes are recorded | PASS | new hookbox hash recorded after the fix; other two re-verified |
| Dependencies are declared | PASS | MINK stdlib only; Python 3.11 standard library for oracles |
| Limitations are explicit | PASS | READMEs plus Sessions 165-177 |
| No unsupported claims remain | PASS | no timing, concurrency, Unicode or cross-language superiority claim |

### 2.4 Repository integrity

| Requirement | Status | Evidence |
| --- | --- | --- |
| No unrelated changes introduced | PASS | source changes limited to the JSON arena fix and hookbox; no compiler/runtime change |
| No MINK release version changed | PASS | `mink --version` remains 1.0.4; no version file touched |
| No unapproved publishing or pushing occurred | PASS | local `main` is ahead of `origin/main` (`b860837`); no push performed |
| No destructive Git operations occurred | PASS | no reset, clean, stash, rebase or amend |
| Session records reflect repository state | PASS | commits and hashes match `git log` |
| All intended work committed or recorded as pending | PASS | M5 work committed; pre-existing unrelated M3/M4 worktree changes left untouched |

## 3. Application matrix

| Application | Contract | Checks | Contract status | Standalone PE | Audit |
| --- | --- | ---: | --- | --- | --- |
| `repoaudit` | section 5 | 50 | GREEN | 216,576 bytes, `3d1b15cf...` | Session 165 |
| `hookbox` | section 6 | 60 | GREEN | 231,424 bytes, `a7c4be44...` | Sessions 167, 175 |
| `releasepack` | section 7 | 53 | GREEN | 352,256 bytes, `1ce41406...` | Session 169 |
| **Total** | | **163** | **3 GREEN** | | |

## 4. Test evidence

| Suite | Command | Result | Exit |
| --- | --- | --- | ---: |
| `repoaudit` | `python benchmarks/m5/repoaudit/tests/run_tests.py --no-build` | 50 passed, 0 failed, 0 skipped | 0 |
| `hookbox` | `python benchmarks/m5/hookbox/tests/driver.py --no-build` | 60 passed, 0 failed, 0 skipped (3 runs) | 0 |
| `releasepack` | `python benchmarks/m5/releasepack/tests/driver.py --no-build` | 53 passed, 0 failed, 0 skipped | 0 |
| JSON stdlib | `cargo test --test json` | 68 passed, 0 failed | 0 |

Counts are per full run; repeated runs are executions of the same check set.

## 5. Build evidence

| Application | `mink check` | Artifact size | SHA-256 |
| --- | --- | ---: | --- |
| `repoaudit` | exit 0 | 216,576 | `3d1b15cf91f9afa8466a811c82355564660c716e753187c0bc4763e2a361b46a` |
| `hookbox` | exit 0 | 231,424 | `a7c4be44a67f7f0dbb54372e4386fd6b862a842f3593d83e0a00add078a48e25` |
| `releasepack` | exit 0 | 352,256 | `1ce414067003fe8a2c7924d3effc41a5e93f7a46597207ef08dc538cb1c8cadb` |

Each suite contains an outside-tree case that copies the executable and
fixtures to a temporary directory and runs from there.

## 6. Known limitations

| Limitation | Application | Classification |
| --- | --- | --- |
| Parsed input plus tree must fit below the arena's final slot; serializer needs space after the tree | `hookbox` | documented capacity contract; 8 KiB bodies supported in 256 KiB |
| `json_parse` keeps the historical ~3.5 KiB practical limit | stdlib callers | backward-compatible default |
| `markers[3..4]` accepted by schema but have no frozen report column | `repoaudit` | frozen-contract ambiguity |
| Reports replaced in place, not atomic | `repoaudit` | contract scoped |
| Extraction not transactional on a mid-run filesystem failure | `releasepack` | documented; unsafe archives pre-validated |
| Output paths required to be plain relative paths | `releasepack` | stricter than the frozen text, documented |
| Windows file-name case folding for event names | `hookbox` | Windows platform boundary |
| No Unicode path guarantee; ASCII fixtures only | all three | scoped language/platform boundary |
| No console-control handler (no Ctrl+C) | `hookbox` | MINK platform gap |
| Sequential request handling only | `hookbox` | contract does not require concurrency |

## 7. Unresolved issues

**None for M5.** The former contract gap (valid JSON bodies 4,097-8,192 bytes
returning `400`) is closed by `json_parse_in` and verified end to end. Broader
capability limits (concurrency, Unicode paths, TLS, large data) are outside the
frozen contracts and are carried into M6/M7 as documented boundaries, not as
open M5 defects.

## 8. Repository state

- Branch `main`, revision HEAD `aeceee1`.
- `origin/main` at `b860837`; local `main` is ahead and has not been pushed.
- M5-owned files committed; the working tree also contains pre-existing M3/M4
  modifications unrelated to M5, left untouched.
- Source changes in this closure: `stdlib/json.mink` and its npm mirror,
  `tests/json.rs`, `benchmarks/m5/hookbox/app/main.mink` and its driver. No
  compiler or runtime source changed.

## 9. Closure decision

| Gate | Condition | Result |
| --- | --- | --- |
| A. M5 READY FOR CLOSURE | every frozen clause met and every limitation bounded | **MET** |
| B. M5 BLOCKED | a required application cannot be completed | **not met** |
| C. M5 PARTIAL | bounded capability gap prevents full contract closure | **not met** - gap closed |

**Decision: M5 COMPLETE.** All three frozen applications are GREEN, 163 checks
pass per full run, the previously blocking boundary is verified at 4,096,
4,097, 8,192 and 8,193 bytes, and every remaining limitation is outside the
frozen contracts and explicitly documented.

## 10. Recommended next action

Proceed to M6 as the next milestone: external-developer usability. The M6
sessions defined in the handoff are currently NOT STARTED because this block
was repurposed to close M5; see
[`../M6/M6_DEFERRED_SESSIONS.md`](../M6/M6_DEFERRED_SESSIONS.md).

Do not expand MINK's feature scope as part of M6. The first M6 task is the test
design (Session 174 in the M6 plan), including an honest cold-start simulation
or a genuinely independent participant.
