# Session 172 — M5 readiness and closure gate

**Date:** 2026-11-04 (gate originally classified 2026-11-03 in commit `6d859ff`)
**Decision:** **M5 PARTIAL**
**Scope:** formal readiness classification of the three frozen M5 applications.
No performance measurement, no MINK source change, no contract change.
**Starting HEAD:** `5e67649` (`docs: publish final M5 evidence report`).
**Contract:** [`M5_APPLICATION_SELECTION.md`](M5_APPLICATION_SELECTION.md),
sections 5-7, frozen in Session 163.

This revision adds the checklist, application matrix, build evidence and
repository-state sections required by the M5 continuation brief. It does not
change the decision.

## 1. Method

- Re-read the frozen selection document and every M5 session record.
- Re-ran all three committed suites on 2026-11-04 against the committed
  executables (`--no-build`): 50 + 60 + 53 = 163 checks, all green, exit 0.
- Re-hashed all three executables and all committed goldens against the values
  recorded in Sessions 166, 168, 170, 171 and 173: every hash matched.
- Ran `mink check` on each application source: exit 0 for all three.
- Checked the Git range `bcd3fbd..HEAD` for changes outside M5 ownership.

## 2. Closure checklist

Status vocabulary: PASS, FAIL, PARTIAL, BLOCKED, NOT APPLICABLE.

### 2.1 Application completeness

| Requirement | Status | Evidence |
| --- | --- | --- |
| Every required M5 application implemented or dispositioned | PASS | 3 of 3 frozen applications: `repoaudit`, `hookbox`, `releasepack` |
| Every application has a frozen contract | PASS | selection sections 5, 6, 7 (commit `8cdb33e`) |
| Every application has independent verification | PASS | Python oracles + drivers, 163 checks per full run |
| Every application has an audit record | PASS | Sessions 165, 167, 169 |
| Every application has a standalone build | PASS | 216,576 / 230,912 / 352,256-byte PEs, run outside the tree |
| No application invented beyond the frozen selection | PASS | Session 170 records that no fourth application is named |

### 2.2 Correctness and reliability

| Requirement | Status | Evidence |
| --- | --- | --- |
| All required tests pass | PASS | 163/163 on 2026-11-04; 163/163 three times on 2026-11-03 |
| No known in-scope application defect remains unresolved | PASS | all defects found in Sessions 166-169 were fixed and covered |
| Boundary behavior documented | PASS | hookbox 4,096/4,097/8,192/8,193; repoaudit size/marker/NUL/line; releasepack empty set and limits |
| Error handling verified | PASS | exits 0-7, 0-4, 0-6 plus HTTP 400/404/405/413/500 actually exercised |
| Resource ownership audited | PASS | single-exit paths; no partial reports/events/extractions on the tested failures |
| Deterministic output verified where required | PASS | three byte-identical runs per suite; fixed ZIP epoch/level |
| Golden outputs match exactly | PASS | byte comparison in every suite; goldens re-hashed 2026-11-04 |
| One frozen hookbox clause is met | **PARTIAL** | valid JSON bodies 4,097-8,192 bytes return `400` (stdlib arena) |

### 2.3 Evidence quality

| Requirement | Status | Evidence |
| --- | --- | --- |
| Results are reproducible | PASS | documented commands re-executed 2026-11-04 |
| Test counts are exact | PASS | 50 / 60 / 53 = 163 per full run, no skips |
| Commands are documented | PASS | three application READMEs and session records |
| Binary hashes are recorded | PASS | Sessions 170, 171, 173; all three matched on 2026-11-04 |
| Dependencies are declared | PASS | MINK stdlib only; Python 3.11 standard library for oracles |
| Limitations are explicit | PASS | READMEs plus Sessions 165-173 |
| No unsupported claims remain | PASS | no timing, concurrency, Unicode or cross-language superiority claim |

### 2.4 Repository integrity

| Requirement | Status | Evidence |
| --- | --- | --- |
| No unrelated changes introduced | PASS | every M5 commit path is under `benchmarks/m5/` or `docs/benchmarks/M5/`; no MINK source diff since `bcd3fbd` |
| No MINK release version changed | PASS | no manifest/version file appears in any M5 commit |
| No unapproved publishing or pushing occurred | PASS | local `main` is 26 ahead / 1 behind `origin/main` (`b860837`); no push performed |
| No destructive Git operations occurred | PASS | no reset, clean, stash, rebase or amend |
| Session records reflect repository state | PASS | commits and hashes match `git log`; this record corrects the file name only |
| All intended work committed or recorded as pending | PASS | M5 work committed; pre-existing M3/M4 worktree changes are unrelated and explicitly left untouched |

## 3. Application matrix

| Application | Contract | Checks | Contract status | Standalone PE | Audit |
| --- | --- | ---: | --- | --- | --- |
| `repoaudit` | section 5 | 50 | GREEN | 216,576 bytes, `3d1b15cf...` | Session 165 |
| `hookbox` | section 6 | 60 | APPLICATION GREEN / CONTRACT PARTIAL | 230,912 bytes, `6ac524fa...` | Session 167 |
| `releasepack` | section 7 | 53 | GREEN | 352,256 bytes, `1ce41406...` | Session 169 |
| **Total** | | **163** | 2 GREEN, 1 PARTIAL | | |

## 4. Test evidence

| Suite | Command | Result | Exit |
| --- | --- | --- | ---: |
| `repoaudit` | `python benchmarks/m5/repoaudit/tests/run_tests.py --no-build` | 50 passed, 0 failed, 0 skipped | 0 |
| `hookbox` | `python benchmarks/m5/hookbox/tests/driver.py --no-build` | 60 passed, 0 failed, 0 skipped | 0 |
| `releasepack` | `python benchmarks/m5/releasepack/tests/driver.py --no-build` | 53 passed, 0 failed, 0 skipped | 0 |

The same 163-check result was recorded three times on 2026-11-03 (Sessions
170, 171, 173) and reproduced once more on 2026-11-04 for this gate. Counts are
per full run; repeated runs are executions of the same check set, not
additional cases.

## 5. Build evidence

| Application | `mink check` | Documented build | Artifact size | SHA-256 |
| --- | --- | --- | ---: | --- |
| `repoaudit` | exit 0, 18,927 tokens | `target/release/mink.exe build benchmarks/m5/repoaudit/app/main.mink` | 216,576 | `3d1b15cf91f9afa8466a811c82355564660c716e753187c0bc4763e2a361b46a` |
| `hookbox` | exit 0, 22,276 tokens | `target/release/mink.exe build benchmarks/m5/hookbox/app/main.mink` | 230,912 | `6ac524fae3282caa62bdb0138841295480545c147319d5b67d2d750d9e993c77` |
| `releasepack` | exit 0, 37,636 tokens | `target/release/mink.exe build benchmarks/m5/releasepack/app/main.mink` | 352,256 | `1ce414067003fe8a2c7924d3effc41a5e93f7a46597207ef08dc538cb1c8cadb` |

Each suite contains an outside-tree case that copies the executable and
fixtures to a temporary directory and runs from there. No sidecar file is
required by any application.

## 6. Known limitations

| Limitation | Application | Classification |
| --- | --- | --- |
| Valid JSON bodies 4,097-8,192 bytes return `400` | `hookbox` | MINK stdlib JSON arena geometry; contract PARTIAL |
| `markers[3..4]` accepted by schema but have no frozen report column | `repoaudit` | frozen-contract ambiguity |
| Reports replaced in place, not atomic | `repoaudit` | contract scoped |
| Extraction not transactional on a mid-run filesystem failure | `releasepack` | documented; unsafe archives pre-validated |
| Output paths required to be plain relative paths | `releasepack` | stricter than the frozen text, documented |
| Windows file-name case folding for event names | `hookbox` | Windows platform boundary |
| No Unicode path guarantee; ASCII fixtures only | all three | scoped language/platform boundary |
| No console-control handler (no Ctrl+C) | `hookbox` | MINK platform gap |
| Sequential request handling only | `hookbox` | contract does not require concurrency |

## 7. Unresolved issues

**One contract-level gap:** `hookbox` cannot accept a valid JSON object of
4,097-8,192 bytes. The application resets the JSON write pointer before every
parse, keeps the frozen 8,192-byte limit, refuses over-limit bodies with the
contract's `413`, and maps the arena-refused range to `400`. It does not modify
`stdlib/json.mink` and does not claim the full range.

No other unresolved defect, test failure or uncommitted in-scope change exists.

## 8. Repository state

- Branch `main`, starting HEAD `5e67649`.
- `origin/main` at `b860837`; local `main` is 26 ahead / 1 behind.
- M5-owned files are clean; the only M5 change in this session is this record's
  file name.
- Working tree contains 67 pre-existing M3/M4 modifications and untracked
  files unrelated to M5; none was staged or altered.
- No MINK source change: `git diff --stat bcd3fbd..HEAD -- src/ npm/mink/ stdlib/ tests/`
  is empty.

## 9. Closure decision

| Gate | Condition | Result |
| --- | --- | --- |
| A. M5 READY FOR CLOSURE | every frozen clause met and every limitation bounded | **not met** - one `hookbox` clause is unmet |
| B. M5 BLOCKED | a required application cannot be completed | **not met** - all three are implemented and runnable |
| C. M5 PARTIAL | representative set complete, bounded capability gap prevents full contract closure | **MET** |

**Decision: M5 PARTIAL.** The representative set is complete (3 of 3
applications, 163 checks per run, all green) and every limitation is explicitly
bounded. M5 is not COMPLETE because the frozen `hookbox` 8 KiB body contract is
not fully satisfiable with the current JSON arena.

The gap must not be closed by weakening the contract, deleting the boundary
test, or editing `stdlib/json.mink` without authorization.

## 10. Recommended next action

The smallest safe step is a separately scoped MINK change to the JSON arena
capacity (with a permanent boundary regression), or an explicit contract
amendment to 4,096 bytes for `hookbox`. Either way, re-run the four existing
boundary cases (4,096 / 4,097 / 8,192 / 8,193) afterwards.

Do not start M6 from this gate. M6 is an external-developer usability stage and
requires its own scope decision, recorded in
[`M5_FINAL_REPORT.md`](M5_FINAL_REPORT.md).
