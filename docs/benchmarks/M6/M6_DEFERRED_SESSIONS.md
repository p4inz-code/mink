# M6-M9 - Programme ledger

**Updated:** 2026-11-05
**Current milestone:** M6 - external developer usability
**Status:** M6 PARTIAL (Sessions 178-181 consolidated); M7, M8, M9 NOT STARTED

## Session status

| Session | Planned work | Status |
| ---: | --- | --- |
| 174 | M6 experiment design and baseline | **COMPLETE** |
| 175 | Installation and environment validation | **COMPLETE** |
| 176 | First program and basic language learning | **COMPLETE** |
| 177 | Language documentation audit | **COMPLETE** |
| 178 | Standard-library discoverability | **COMPLETE** |
| 179 | Debugging/testing workflow (consolidated; includes diagnostics) | **COMPLETE** |
| 180 | Cold-start application and verifier (consolidated; absorbs original 182-183) | **COMPLETE** |
| 181 | Cross-cutting usability audit and M6 decision (consolidated; absorbs original 185 decision) | **COMPLETE** |
| 182-183 | Original cold-start implementation/audit plan | **SUPERSEDED** by consolidated 180 |
| 184 | Build, packaging and distribution usability | **NOT STARTED** |
| 185 | M6 final audit and closure | **NOT STARTED** (decision recorded in 181; closure deferred) |
| 186-197 | M7 tooling and ecosystem demand | **NOT STARTED** |
| 198-203 | M8 strategic evaluation and decision | **NOT STARTED** |
| 204-209 | M9 conditional platform expansion evaluation | **NOT STARTED** |

M5 is COMPLETE. Its final report and readiness gate remain authoritative for
the application evidence used by M6.

## Sessions 178-181 consolidated result

The four sessions were executed as one assignment. Scope was remapped: the
original 179 (diagnostics), 180 (testing) and 181 (workload selection) became
one debugging/testing guide; the original 182-183 application work moved into
180; the original 185 closure decision was taken in 181. The original table
rows are preserved above with the remap noted.

M6 is **PARTIAL**, not complete. The deliverables exist and the Session 180
application passes 12/12 verifier cases with the repository 1.0.4 compiler, but
these concrete acceptance gaps remain:

1. `stdlib/environment.mink` cannot be loaded (`E-P01`); `rt_env_*` works only
   as predeclared intrinsics.
2. The `zip` reader leaks in a composite probe (exit 106); the source was not
   isolated.
3. A `Vec<Str>` accumulated inside nested `if`/`else` blocks was corrupted
   (length 16, `E-R04`); no minimal compiler reproduction was produced.
4. Installed compiler 1.0.2 builds the Session 180 application but exits 106 on
   normal input; 1.0.4 exits 0. Cross-version parity is unverified.
5. `README.md` still says stdlib is "imported with `use`" and omits collection
   types; its pre-existing uncommitted changes were preserved, not edited.
6. Collection use-after-free remains undiagnosed (preserved known defect).
7. No exhaustive standard-library coverage claim is made.

Full evidence and file list:
[`M6_CONSOLIDATED_SESSIONS_178_181.md`](M6_CONSOLIDATED_SESSIONS_178_181.md).

Next session: **182 - defect closure** (environment module, zip reader leak,
`Vec<Str>` nested-push reproduction, install/version parity), then a closure
record. Do not advance to M7.

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
