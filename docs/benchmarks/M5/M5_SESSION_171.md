# Session 171 — repository-wide M5 evidence audit

**Date:** 2026-11-03
**Status:** **AUDIT COMPLETE — 163/163 checks green in three consecutive runs; two documentation/fixture defects fixed.**
**Starting HEAD:** `3a06b19` (`benchmarks: record M5 cross-application matrix`).
**Scope:** every M5 application, suite, fixture, golden, oracle, session
record, README and executable claim. No performance measurement, no MINK
source change.

## 1. Checklist

| # | Check | Result | Evidence |
| ---: | --- | --- | --- |
| 1 | Contracts frozen and traceable | PASS | selection document unchanged since `8cdb33e` (blob `01129300cc5db7e282896916d6de32417163bfaf`); sections 5–7 map to the three READMEs |
| 2 | Every GREEN claim has executable evidence | PASS | 50 + 60 + 53 = 163 checks, three runs each, all exit 0 |
| 3 | No BLOCKED item represented as GREEN | PASS | `hookbox` is labelled APPLICATION GREEN / CONTRACT PARTIAL in its README, Session 167 and the Session 170 matrix |
| 4 | No limitation hidden | PASS | all three READMEs have an explicit limitations section; the JSON 4,097–8,192-byte range is called out in four places |
| 5 | No MINK source modifications | PASS | `git diff bcd3fbd..HEAD -- src/ npm/mink/ target/` is empty |
| 6 | No M4 performance result mixed into M5 | PASS | M5 records contain no throughput, speedup or ranking claim; the only “faster” sentence is the selection document's explicit non-goal |
| 7 | Line endings correct | PASS | `git ls-files --eol`: hookbox events LF, hookbox stdout/help CRLF; releasepack manifest LF, `archive.sha256`/stdout/help CRLF; repoaudit CSVs LF, stdout CRLF, summary JSON no final newline; all `attr/-text` |
| 8 | Goldens byte-exact | PASS | every suite compares raw bytes; releasepack and hookbox also hash-check archives/events |
| 9 | Repeated-run claims reproducible | PASS | three full runs of each suite today, identical summaries, no flakes |
| 10 | Standalone claims reproducible | PASS | each suite has an outside-tree executable case; a manual copied-fixture run also passed |
| 11 | Executables independent of repository cwd | PASS | suites copy the executable and fixtures to temporary directories and run from there |
| 12 | Exit codes actually exercised | PASS | repoaudit 0–7, hookbox 0–4 plus HTTP 400/404/405/413/500, releasepack 0–6 |
| 13 | Failure cleanup tested | PASS | no reports after a scan failure; no partial event after a write failure; no extraction on unsafe archives; no output after a build failure |
| 14 | Session records agree with Git history | PASS | `8cdb33e` freeze, `2fe1d21`/`bcd3fbd` repoaudit, `75c3b3b`/`6714c52` hookbox, `4a5b551`/`67a3e38` releasepack, `3a06b19` matrix |

## 2. Defects found and fixed

### D1 — wrong archive digest recorded in `M5_SESSION_168.md`

The Session 168 golden table labelled `d68e6775…` as both the digest of
`expected/basic/archive.sha256` and the recorded archive digest. Only the first
is true; `d68e6775…` is the digest of the golden file itself. The content of
that file, i.e. the deterministic archive digest, is
`4ca56bba39d02103f2d3f23efbe1b5158a7ba2137fe9e1f4a0830dd492400501`.
The record is corrected in this audit commit. The suite already checked the
value correctly, so this was a documentation-only defect.

### D2 — committed `releasepack` fixture had no output parent

`fixtures/basic/config.json` writes `out/release.zip` and `out/manifest.csv`,
and the contract requires output parents to exist. `fixtures/basic/out/` is
not tracked, so copying the fixture and building directly returned exit 4
(`archive directory does not exist`). The test driver creates `out/` in each
temporary copy, which is why the suite never exposed it.

A `.gitkeep` placeholder was tried first, but the build's output assertion
then failed with `unexpected output files` (the placeholder is not an output).
The placeholder was removed and the fixture README now states the manual step
explicitly: `mkdir -p out` after copying `fixtures/basic`. This preserves the
strict output assertion instead of weakening it.

Verified after the fix:

```text
build    exit 0  files=8 bytes=196 archive_bytes=1075
verify   exit 0  verified files=8
extract  exit 0  extracted files=8
```

No application defect was found in this session.

## 3. Artifact and hygiene scan

- No hardcoded developer paths in `benchmarks/m5/**/*.mink`, `*.py` or
  `*.json` (`C:\`, `C:/`, `Users/`, `Admin`, `Desktop` all absent).
- No executable or `build/` content is tracked. Only ignored build outputs
  (`app/*.exe`, `build/`) and Python `__pycache__` remain untracked.
- No `.tmp`, `.log` or scratch file under `benchmarks/m5`; the probe logs used
  during these sessions live under the ignored top-level `tmp/m5probe/`.
- Python test dependencies are the standard library only (`argparse`, `csv`,
  `hashlib`, `json`, `socket`, `subprocess`, `tempfile`, `zipfile`).
- No network access outside `hookbox`'s loopback listener. `repoaudit` and
  `releasepack` import no network module.
- Environment-specific code is limited to Windows by design: the suite's
  sharing-lock failure case and the PE executables.

## 4. Evidence snapshot

| Suite | Runs | Result |
| --- | ---: | --- |
| `repoaudit` | 3 | 50/50 each, exit 0 |
| `hookbox` | 3 | 60/60 each, exit 0 |
| `releasepack` | 3 | 53/53 each, exit 0 |

`git diff --check bcd3fbd..HEAD -- benchmarks/m5 docs/benchmarks/M5` is clean.
The only diff outside M5 since the freeze is none: every changed path is under
`benchmarks/m5/` or `docs/benchmarks/M5/`.

## 5. Remaining limitations

| Limitation | Where recorded | Status |
| --- | --- | --- |
| hookbox valid JSON bodies 4,097–8,192 bytes -> 400 | README, Sessions 166/167/170 | contract PARTIAL, not hidden |
| repoaudit `markers[3..4]` have no report column | README, Session 165 | frozen ambiguity |
| no Unicode path guarantee (all three) | READMEs | scoped by ASCII fixtures |
| repoaudit reports not atomic | README, Session 165 | contract scoped |
| releasepack extraction not transactional | README | documented |
| hookbox sequential networking only | README | contract does not require concurrency |

## 6. Commit

Staged the two fixes (`M5_SESSION_168.md`, releasepack `README.md`), the
removed placeholder (never committed), and this record. No MINK source, no
`repoaudit`/`hookbox` application file, and no pre-existing M3 work was
touched. No reset, clean, stash, rebase, amend, force, push, tag or publish.

## 7. Final status

**AUDIT COMPLETE.** The M5 evidence is internally consistent and reproducible:
163/163 checks three times, byte-exact goldens, no MINK changes, and every
limitation visible. The `hookbox` JSON range remains the only contract-level
gap; Session 172 classifies readiness without hiding it.
