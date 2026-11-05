# M6-M9 - Programme ledger

**Updated:** 2026-11-05
**Current milestone:** M6 - external developer usability
**Status:** M6 IN PROGRESS; M7, M8, M9 NOT STARTED

## Session status

| Session | Planned work | Status |
| ---: | --- | --- |
| 174 | M6 experiment design and baseline | **COMPLETE** |
| 175 | Installation and environment validation | **COMPLETE** |
| 176 | First program and basic language learning | **COMPLETE** |
| 177 | Language documentation audit | **COMPLETE** |
| 178 | Standard-library discoverability | **NOT STARTED** |
| 179 | Compiler diagnostics and error recovery | **NOT STARTED** |
| 180 | Testing and debugging workflow | **NOT STARTED** |
| 181 | First external application workload selection | **NOT STARTED** |
| 182 | Cold-start application implementation | **NOT STARTED** |
| 183 | Cold-start application adversarial audit | **NOT STARTED** |
| 184 | Build, packaging and distribution usability | **NOT STARTED** |
| 185 | M6 final audit and closure | **NOT STARTED** |
| 186-197 | M7 tooling and ecosystem demand | **NOT STARTED** |
| 198-203 | M8 strategic evaluation and decision | **NOT STARTED** |
| 204-209 | M9 conditional platform expansion evaluation | **NOT STARTED** |

M5 is COMPLETE. Its final report and readiness gate remain authoritative for
the application evidence used by M6.

## Session 177 result

Session 177 verified the Session 176 findings against source and execution,
corrected the module/import grammar, added
[`docs/language/LANGUAGE_BASICS.md`](../../language/LANGUAGE_BASICS.md), and
added `benchmarks/m6/session177/verify.py`. The full evidence and deferred
issues are in
[`M6_SESSION_177_DOCUMENTATION_AND_LANGUAGE_LEARNING_AUDIT.md`](M6_SESSION_177_DOCUMENTATION_AND_LANGUAGE_LEARNING_AUDIT.md).

## Starting point for Session 178

Read:

- [`M6_SESSION_176_FIRST_PROGRAM_AND_LANGUAGE_LEARNING.md`](M6_SESSION_176_FIRST_PROGRAM_AND_LANGUAGE_LEARNING.md)
- [`M6_SESSION_177_DOCUMENTATION_AND_LANGUAGE_LEARNING_AUDIT.md`](M6_SESSION_177_DOCUMENTATION_AND_LANGUAGE_LEARNING_AUDIT.md)
- [`docs/language/LANGUAGE_BASICS.md`](../../language/LANGUAGE_BASICS.md)
- `stdlib/*.mink` and `examples/`

Then audit standard-library discoverability and practical API contracts. Keep
the collection use-after-free defect and the qualified-path diagnostic as
isolated implementation follow-ups, and correct the README only after its
pre-existing worktree changes are resolved.

## Rules for this block

- Do not add language features during M6.
- Do not claim external validation: Session 174 is an internal cold-start
  simulation.
- Do not advance to M7 until M6 has a closure decision.
- Do not push, publish, tag or change the release version.
- Keep pre-existing M3/M4 worktree changes untouched.
