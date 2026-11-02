# Session 163 — M5 application selection and contract freeze

**Date:** 2026-11-02
**Status:** **COMPLETE — selection and contracts frozen. No application code
was written.**
**Scope:** M5 is three complete Windows x86_64 CLI applications, not another
five-language benchmark suite.

## 1. What was inspected

Before selecting anything, the session reviewed:

- M1: `docs/validation/VALIDATION_MILESTONE_1_REPORT.md`,
  `VALIDATION_MILESTONE_1_CAPABILITY_MATRIX.md`, and the ten
  `validation/*/main.mink` applications;
- M2: Sessions 133 (networking), 134 (server/integrated application), 135
  (real-world workloads), 136 (large-scale limits), 138 (final gap audit);
- M3: Sessions 141 (`net_dial`), 145 (socket timeouts), 146 (UDP),
  147 (HTTP chunked), 148/149 (parser and print fixes);
- M4: `M4_WORKLOAD_SELECTION.md`, `M4_SESSION_162_FINAL_REPORT.md`,
  `M4_RESULTS_SUMMARY.json`, and the five application contracts;
- `stdlib/*.mink`, especially `filesystem`, `json`, `csv`, `re`, `hashing`,
  `zip`, `httpserver`, `process`, `network`, `threads`;
- the existing tests for `httpserver` (10), `zip` (10), `process`, JSON, CSV
  and regex;
- `docs/roadmap/IMPLEMENTATION_ROADMAP.md`;
- the repository for `mink-scout`: **no such implementation or document
  exists in this tree**, so the selection does not depend on it.

## 2. Selection decision

Exactly three applications are frozen in
[`M5_APPLICATION_SELECTION.md`](M5_APPLICATION_SELECTION.md):

| App | Profile | Why it is materially beyond M4 |
| --- | --- | --- |
| `repoaudit` | Offline project inventory/policy audit | Adds schema-driven policy, line/marker analysis, three report artifacts and a violation exit contract; not a JSON-only utility or hasher |
| `hookbox` | Loopback HTTP webhook inbox | Adds an HTTP **server**, routing, JSON body validation and per-event persistence; M4 W2 was client-only |
| `releasepack` | Deterministic release archiver | Adds ZIP create/read/verify/extract, byte-identical archive output and path safety; M5 does not repeat M4's SQL/thread/runtime experiments |

Rejected candidates and reasons are recorded in the selection document. The
main rejection was a process/task orchestrator: it is useful, but the bounded
capture and Windows-only process model make a complete-application claim
fragile, and M4 already excluded process automation for semantic comparability.
It is left as a possible later process-focused stage, not silently dropped.

## 3. Contract freeze summary

Each selected application has the full 13-item freeze in the selection
document: name, purpose, user story, CLI, I/O contract, exit codes, required
features, non-features, test strategy, artifact model, MINK constraints,
comparison-implementation decision and definition of done.

Key boundaries:

- **No cross-language ports in M5.** Python scripts are independent oracles
  and drivers, not shipped competitors. M4 remains the five-language evidence
  set; duplicating it would violate the M5 purpose and inflate scope.
- **No performance measurement in M5.** Build procedure, dependency count,
  artifact size and engineering friction are observed qualitatively; runtime
  numbers remain the M4 measurements.
- **No unsupported capability is assumed.** Fixtures are ASCII; JSON bodies and
  configs stay within the documented arena; the SQLite workload is not
  repeated; HTTP is Content-Length only; ZIP is stored/DEFLATE only with no
  ZIP64.
- **No MINK source changes are planned.** If a genuine defect appears, it must
  be reproduced minimally and recorded; a fix requires a separate isolated
  change with a regression and must not absorb pre-existing M3 work.

## 4. M4 audit

M5 genuinely adds evidence rather than re-running M4:

- `repoaudit` uses W5's walk/CSV experience but adds configuration validation,
  text/marker analysis, policy rules and a violation exit code.
- `hookbox` adds HTTP server parsing/routing/lifecycle; M4 only tested an HTTP
  client.
- `releasepack` adds ZIP creation and verification, deterministic archive
  bytes and extraction safety; M4 only hashed files and wrote a CSV.
- No M5 application is a JSON-only utility, a simple hasher, a synthetic
  thread benchmark, a database benchmark or trivial CRUD.
- M5 does not produce a composite score or a language ranking.

The M4 conclusion ("qualified yes for a defined class") will be tested in M5
through complete application engineering, not through new micro-benchmarks.

## 5. Roadmap handling

The roadmap does **not** contain an `M5 NEXT` state; it describes M5 as a
seven-category competitive shootout and still carries historical M4 status
text. Because the required exact transition does not exist, and because the
roadmap is a pre-existing modified file, Session 163 did not rewrite it. The
M5 scope is frozen here and in the selection document; a dedicated roadmap
edit is the correct place for the editorial change.

## 6. Tests and measurements

- Application tests: none yet — no application code was written.
- MINK compiler/runtime tests: not run; no MINK source was changed.
- Performance measurements: none; explicitly out of scope for M5.
- Documentation check: both new M5 documents were reviewed for the frozen
  contracts and for claims not supported by prior evidence.

## 7. Files changed

```text
docs/benchmarks/M5/M5_APPLICATION_SELECTION.md   (new)
docs/benchmarks/M5/M5_SESSION_163.md             (new)
```

No MINK compiler/runtime/stdlib file, M4 result, fixture, or pre-existing M3
work was modified.

## 8. Git state and commit

HEAD at session start: `02fe23e`. The two M5 documents are the only files
committed in this session's M5 commit. The pre-existing modified/untracked M3
work is untouched. No reset, clean, stash, rebase, amend, force, push, tag or
publish was used.

## 9. Remaining work and exact next session

**Session 164 — M5 Application 1 (`repoaudit`).** Implement the frozen
contract in MINK first, with modules where justified, complete CLI/schema
validation, deterministic fixtures and Python-oracle tests. Do not benchmark.

M5 is not complete: all three applications must be implemented, tested,
distributed and audited in Sessions 164–167.
