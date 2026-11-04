# M6 Session 174 - external developer experiment design and baseline

**Date:** 2026-11-04
**Status:** **COMPLETE**
**Starting HEAD:** `f57121d` (`docs: close M5 after JSON arena capacity fix`)
**Branch:** `main` (29 ahead / 1 behind `origin/main`; 63 pre-existing worktree
entries unrelated to M6)
**Scope:** define the M6 usability experiment and capture the baseline. No
compiler, runtime, standard-library, documentation or application changes.

## 1. Test objective

Determine whether a developer who did not create MINK can:

1. understand what MINK is and which platform it supports;
2. obtain and configure the compiler;
3. write, compile, run and modify a first program;
4. understand representative compiler errors;
5. discover and use standard-library modules;
6. build a small useful application;
7. write and run tests;
8. produce and run a standalone executable outside the source tree;
9. understand the documented limitations.

The experiment measures MINK's usability, not the participant's programming
ability.

## 2. Evidence mode

**Mode B - internal cold-start simulation.** No independent external developer
participated. This record is therefore **not** external validation, and later
M6 records must keep that label. Every point where repository-internal
knowledge was required must be recorded in the session log.

Mode A (genuine external evaluation) remains open if a participant becomes
available; its results would be recorded separately and would not retroactively
change this simulation's label.

## 3. Persona

- Three or more years with at least one mainstream language (Python,
  TypeScript, C#, Go or Rust).
- Comfortable with a terminal, environment variables and npm.
- Windows 10/11 x86_64 user.
- Has never used MINK and knows nothing about its compiler internals.
- Will use only developer-facing material: root README, `docs/language`,
  `docs/stdlib`, `docs/toolchain`, `docs/testing`, `examples/`, CLI help and
  `mink explain`.
- Will not read `src/`, `tests/`, implementation session records, or ask the
  original developer.

## 4. Baseline documentation inventory

Captured 2026-11-04 at `f57121d`.

| Area | Existing material | Accuracy | Completeness | Verification |
| --- | --- | --- | --- | --- |
| Installation | `README.md` "Install MINK" (Node 18+, npm global, version check, 1.0.3-vs-1.0.4 registry note); `npm/mink/package.json` (`win32`/`x64`, Node >=18) | Accurate and unusually explicit about the unpublished 1.0.4 state | Partial: no clean-machine check, no update/uninstall, no offline install | `mink --version` -> `mink 1.0.4`; npm path not yet exercised (Session 175) |
| First program | `README.md` Quick Start (`hello.mink`, `mink run`, expected `42`) | Verified | Good for one program; no edit/rebuild loop | Ran the documented program locally: stdout `42`, exit 0 |
| Language syntax | `docs/language/CORE_GRAMMAR.md` (frozen grammar + additive session sections); `docs/language/CORE_LANGUAGE.md` | Grammar matches the implemented syntax; `CORE_LANGUAGE.md` is a planning document | Partial: grammar is organized as a session history, not a linear user reference | Grammar inspected; examples not yet compiled (Session 176/177) |
| Types | `docs/language/TYPE_SYSTEM.md` | Mixed: states goals, but several sections carry "Implemented (session N)" notes | Partial: no single type reference; future features and implemented features interleaved | Inspected; not yet verified against the compiler |
| Functions | `CORE_GRAMMAR.md` sections 6, 17, 25; README feature list | Accurate for annotated signatures | Partial: no dedicated functions guide | Inspected |
| Collections | README standard-library table; `stdlib/collections.mink` source | Source is authoritative; no prose reference | Low: no documented signatures or ownership rules | Not yet verified |
| Strings | README table and examples; `stdlib/strings.mink` source | Source is authoritative; README is high-level | Low: no API reference; ownership caveats only in audit records | Not yet verified |
| Standard library | `docs/stdlib/STANDARD_LIBRARY_ARCHITECTURE.md` (planning); 27 `stdlib/*.mink` modules on disk | Architecture document is aspirational, not an API reference | Low: no per-module signatures, return types or error conventions | Module count measured: 27 (README claims 26; one is untracked M5 `httpserver.mink`) |
| Errors | `docs/language/ERROR_SYSTEM.md` (planning); CLI `mink explain` | `mink explain` is implemented and lists 50 `E-*` codes | Partial: no troubleshooting guide; planning doc describes an error model beyond the CLI | `mink explain` executed; 50 codes counted |
| Testing | `README.md` (`mink test`), `docs/testing/MINK_COMPREHENSIVE_VERIFICATION_STANDARD.md` | Verification standard is detailed; no application-test quickstart | Partial: assumes repository test infrastructure | `mink help` shows `test`; not yet exercised (Session 180) |
| Standalone build | `README.md` "Build and Run"; M5 application READMEs | Accurate for `mink build <file>` and direct `.exe` execution | Partial: no clean-directory packaging guide beyond the M5 examples | M5 artifacts exist; not yet rebuilt in M6 (Session 184) |
| Troubleshooting | None | n/a | Absent | n/a |
| Editor/tooling | README says tooling is CLI-only; no extension or LSP documentation | Honest | Complete for the current state | README inspected |

### Baseline observations

| ID | Observation | Severity |
| --- | --- | --- |
| B1 | The root README is the only end-to-end guide from install to first program. | Informational |
| B2 | There is no public standard-library API reference; discovery currently requires reading `stdlib/*.mink`. | High |
| B3 | `docs/language` is mostly planning-stage; only `CORE_GRAMMAR.md` describes implemented syntax, and it is structured as an additive session log. | High |
| B4 | `README.md` "What's Next" still says M1 is complete and M2 is next, while M5 is now complete. | Medium |
| B5 | 15 example directories exist, but only 3 have a README. | Medium |
| B6 | There is no troubleshooting section and no documented diagnostic walkthrough. | Medium |
| B7 | Installation has not been exercised from a clean npm state in M6. | High (for Session 175) |
| B8 | `stdlib/` contains 27 modules while README text says 26; the extra module is the untracked M5 `httpserver.mink`. | Low |
| B9 | `mink explain` provides a real, discoverable list of 50 error codes. | Positive finding |
| B10 | README is explicit that there is no debugger, profiler, LSP or editor integration. | Positive finding |

## 5. Task definitions

Each task is executable and has a binary success test. A task passes only when
it is completed from developer-facing material alone.

| ID | Task | Success test |
| --- | --- | --- |
| T1 | State what MINK is and who it targets | Answer matches README, including Windows x86_64 and native `.exe` output |
| T2 | State supported and unsupported platforms | Windows x64 only; Linux/macOS unsupported; no overclaim |
| T3 | State compiler requirements | Node.js 18+ / npm; no Rust needed at install time |
| T4 | Obtain the compiler | `mink --version` prints a version in a clean prefix |
| T5 | Configure the environment | `mink` resolves on PATH without modifying unrelated toolchains |
| T6 | Write a first program with a variable, function, conditional, loop and output | File compiles |
| T7 | Compile the program | `mink build` exits 0 and produces an `.exe` |
| T8 | Execute the program | Output matches the source's intent |
| T9 | Understand and correct a compiler error | Error code, file, line, cause and fix are identified from the diagnostic and docs |
| T10 | Use a standard-library module | A filesystem or JSON program compiles and runs |
| T11 | Build a useful application | Frozen M6 application satisfies its contract (Session 182) |
| T12 | Write and run tests | `mink test` discovers and runs `fn test_*`; a controlled defect is caught |
| T13 | Build a standalone executable | `.exe` produced with no sidecars |
| T14 | Run outside the source tree | Copied `.exe` runs in a clean directory |
| T15 | Explain limitations | Lists Unicode, tooling, platform and ecosystem limits without overclaiming |

## 6. Measurements

Measurements are recorded per task. Times are wall-clock on the same machine
and are indicative, not comparative.

| Measurement | Definition |
| --- | --- |
| Time to first compilation | Seconds from starting T6 to `mink build` exit 0 |
| Time to first execution | Seconds from starting T6 to first correct program output |
| Error-understanding time | Seconds from first diagnostic to a compiling fix |
| API-lookup time | Seconds from task start to locating the needed function |
| Documentation lookups | Count of documents/sections opened for a task |
| Failed commands | Non-zero exits in the session transcript |
| Undocumented assumptions | Each time repository-internal knowledge is needed |
| Workarounds | Each deviation from the documented path |
| Blockers | Tasks that cannot be completed |
| Implementation-knowledge events | Each use of source/tests/session records to proceed |
| Time to useful application | Seconds across Session 182 |
| Time to standalone executable | Seconds from source-complete to outside-tree run |

No target numbers are set before running the experiment.

## 7. Severity scale

| Severity | Meaning |
| --- | --- |
| Critical | Prevents installation, compilation or useful development |
| High | Substantially obstructs normal development |
| Medium | Causes repeated confusion or avoidable work |
| Low | Minor inconvenience |
| Informational | Improvement opportunity without demonstrated impact |

## 8. Pass/fail criteria

- **Task PASS:** completed using developer-facing documentation and supported
  tooling, without hidden guidance.
- **Task PARTIAL:** completed only after consulting repository internals or
  inventing an undocumented step; the exact point is recorded.
- **Task FAIL:** not completed, or completed only by changing the compiler.
- **Experiment PASS:** all of T1-T10, T12-T15 pass and T11 satisfies its frozen
  contract, with no critical blocker.
- **Experiment PARTIAL/BLOCKED:** otherwise, with the exact failing tasks
  listed.

The experiment never passes by lowering a task's success test.

## 9. Risks

1. The author is simulating a cold start and may unconsciously use insider
   knowledge. Mitigation: record every such event; prefer README/doc commands.
2. One machine, one Windows version, one npm state. No clean-VM evidence.
3. The registry serves 1.0.3 while the repository is 1.0.4; installation may
   legitimately expose the published-package state.
4. Test timing is influenced by familiarity and is not comparable to other
   languages.
5. Documentation may be correct but incomplete; a task can pass for the wrong
   reason. Mitigation: require exact commands and outputs.

## 10. Limitations

- No genuine external developer participated; this is labeled an internal
  simulation.
- No comparative timing against other languages was performed or is planned.
- The baseline inventory verifies document existence and selected commands; it
  does not yet prove that every example compiles (Sessions 176/177).
- The 27-module count includes one untracked module from M5; the packaged count
  remains 26.
- No clean-machine installation was attempted in this session.

## 11. Baseline evidence

```text
git rev-parse HEAD                      -> f57121d
git status -sb                          -> main...origin/main [ahead 29, behind 1]
target/release/mink.exe --version       -> mink 1.0.4
target/release/mink.exe help            -> subcommands and options printed
target/release/mink.exe explain | count -> 50 E-* codes
ls stdlib/*.mink | wc -l                -> 27
ls -d examples/*/ | wc -l               -> 15
ls examples/*/README.md | wc -l         -> 3
mink run tmp/m6_hello.mink              -> 42, exit 0
```

No compiler, runtime, standard-library or documentation file was modified.

## 12. Deliverables and next session

- This record.
- Updated programme ledger: Session 174 COMPLETE; 175-209 NOT STARTED.
- Session 175 starts installation validation using the documented npm path in
  an isolated prefix, then failure scenarios and distribution checks.
