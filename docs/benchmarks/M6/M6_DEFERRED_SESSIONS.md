# M6-M9 - Programme ledger

**Updated:** 2026-11-04
**Current milestone:** M6 - external developer usability
**Status:** M6 IN PROGRESS; M7, M8, M9 NOT STARTED

## Session status

| Session | Planned work | Status |
| ---: | --- | --- |
| 174 | M6 experiment design and baseline | **COMPLETE** |
| 175 | Installation and environment validation | **COMPLETE** |
| 176 | First program and basic language learning | **COMPLETE** |
| 177 | Language documentation audit | **NOT STARTED** |
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

## Starting point for Session 177

Read:

- [`M6_SESSION_174_TEST_DESIGN.md`](M6_SESSION_174_TEST_DESIGN.md)
- [`M6_SESSION_175_INSTALLATION.md`](M6_SESSION_175_INSTALLATION.md)
- [`M6_SESSION_176_FIRST_PROGRAM_AND_LANGUAGE_LEARNING.md`](M6_SESSION_176_FIRST_PROGRAM_AND_LANGUAGE_LEARNING.md)
- `docs/language/` and `examples/`

Then audit the language documentation against source and test evidence. Use the
Session 176 findings (line-oriented print, Map typing, module/use wording) as
reported observations, not as assumptions, and do not correct the README in
this session.

## Rules for this block

- Do not add language features during M6.
- Do not claim external validation: Session 174 is an internal cold-start
  simulation.
- Do not advance to M7 until M6 has a closure decision.
- Do not push, publish, tag or change the release version.
- Keep pre-existing M3/M4 worktree changes untouched.
