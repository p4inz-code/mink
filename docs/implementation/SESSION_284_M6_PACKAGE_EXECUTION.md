# Session 284 — M6 Package Execution and External-Gate Check

Windows x86_64, 2026-11-20. Work performed in `F:\Works\Mink`; baseline commit
`99543ff1abc57ab9cd32e1d3e0bd5869360a6657`, with local `main` equal to
`origin/main` before work began. No MINK language, compiler, runtime or
standard-library source changed in this session. 1.0.5 remains unpublished, M8
remains gated and Linux remains frozen.

## 1. Objective

Execute every part of M6 that does not require an independent developer, confirm
the package works for someone who has never seen MINK, fix confirmed in-scope
defects, and record the honest milestone verdict. The session did **not** invent
a tester, an invitation, an external execution or any feedback.

## 2. Baseline

| Item | Value |
| --- | --- |
| Branch | `main`, tracking `origin/main` |
| HEAD | `99543ff1abc57ab9cd32e1d3e0bd5869360a6657` (matches the expected reference) |
| Remote HEAD | same commit |
| Worktree | clean except the four user-owned untracked files: `MINK UI SETS OFFICIAL.zip`, `freebuff-chat-2026-10-01T19-08-00.600Z.md`, `nl1.exe`, `nl1.mink` |
| M6 state | package, protocol and acceptance checklist ready; no tester, no findings |
| M7 / M8 / Linux | Phases A–E complete and PASS (M7) / gated / frozen |

## 3. External-gate check

`VALIDATION_RECORD.md` §1–§5 contain no tester and no submission, and this
execution environment has no authorised channel to an outside developer (no
mailing list, issue-tracker identity or contact authorization). Therefore:

- no independent developer was identified;
- **no invitation was sent**;
- no independent execution occurred;
- no submission exists.

The invitation in `VALIDATION_RECORD.md` §2 was checked against the protocol and
left in place: it names 18 tasks, the 4–6 hour estimate and the required
minimum set, all matching `EVALUATION_PROTOCOL.md` §6. Submission instructions
already exist as `SUBMISSION_TEMPLATE.md` and were verified present and linked.
M6 is recorded as **BLOCKED / PENDING EXTERNAL EXECUTION**, canonical verdict
**OUTSTANDING**.

## 4. Package execution (Gate C)

All checks used the preserved 1.0.5 reference binary
(`target/s213_clean/release/mink.exe`, 4,048,384 bytes, SHA-256
`87a1cae4946f121e292d1a516e6047ad287b6506c5fe52a16208717dc2b6da23`, checksum
re-measured) so that the run matched Path A of the package.

| Check | Command | Result |
| --- | --- | --- |
| Tutorial set | `python benchmarks/m6/session176/verify.py --compiler target/s213_clean/release/mink.exe` | **15/15 passed**, exit 0 |
| Validation applications, build | reference binary over `validation/{a1,a2,a3,b,c,d,e,f,g,h}*/main.mink` | 10/10 built, exit 0 |
| Validation applications, parity | `python verify.py` in each directory | 10/10 harnesses exit 0; **228 cases, 0 differences** (20/18/35/34/43/10/18/19/18/13) |
| Examples, build | reference binary over `examples/*/main.mink` | 15 build, 7 fail — exactly the seven README §1.2 excludes (`struct_match_*`, `defer_*`) |
| Examples, run | each built `main.exe` from its directory | all 15 run; `compress_util` 2, `regex_tool` 2, `http_client` 1, `tcp_client` 1 (usage), `runtime_error_report` 110 (`E-R10` demo), other 10 exit 0 |
| Module probe | `mod filesystem;` program from the repository root | built, printed `2`, exit 0 |
| Module probe, wrong cwd | same program from a directory without `stdlib/` | `E-S01`, the documented resolution rule, not a defect |
| CLI contract | `--version`, `help`, bare invocation, `explain E-T01`, `check` on a type error | `mink 1.0.5`; help and bare invocation exit 0; `check` exits 1 with `E-T01` and a position |
| Package links | relative-link scan of the seven documents | 0 broken |
| Inventory | `stdlib/`, `examples/`, `validation/`, tutorial, protocol | 27 / 22 / 10 / 10 + 5 variants / 18 tasks, all as documented |
| Clean clone | `git clone` of this repository, `cargo build` | exit 0, `mink 1.0.5`, tutorial 15/15 through the clone's compiler |

Generated `main.exe` files from these runs are git-ignored and none was staged.

## 5. Host-level release-build fault (Gate D limitation)

The release profile could not be built in this session. Nine
`cargo build --release` invocations failed inside `rustc.exe` at opt-level 3
while compiling the library, producing no artifact — including one from a fresh
clone in a new target directory. Recorded codes: 3 × `0xc000001d`
(`STATUS_ILLEGAL_INSTRUCTION`), 5 × `0xc0000005` (`STATUS_ACCESS_VIOLATION`), one
exit code not captured. `-j 1`/`-j 2`, `CARGO_INCREMENTAL=0` and a fresh target
directory made no difference; the debug profile compiled twice with exit 0 and
62 warnings.

This is the transient host fault characterized in
[`M6_SESSION_212_RELEASE_TOOLCHAIN_INVESTIGATION.md`](../benchmarks/M6/M6_SESSION_212_RELEASE_TOOLCHAIN_INVESTIGATION.md),
now reproducing on a 32-logical-core host with 19 GB free. It is not a MINK
defect: debug compiles, the reference binary is intact, and HEAD differs from
the last successful release build (`db696bc`, Session 283) only in documentation
and the bundled npm binary. Recorded as a limitation; the release-path
re-measurement needs a host restart or hardware-stability check.

Because no compiler or runtime source changed, the Session 282/283 regression
evidence (100 binaries, 3,640 passed, 0 failed, 10 ignored; release acceptance
PASS) remains applicable and was not repeated.

## 6. Defects fixed

**Harness dirtied tracked fixtures (S3).**
`validation/h_manifest/fixtures/make_fixtures.py` wrote the config templates
with an absolute `root` computed from the machine it ran on, and the templates
are tracked files. Running the documented `python verify.py` therefore left four
tracked fixtures modified with the runner's local path, and the committed copies
still held the old build machine's path (`C:\Users\Admin\Desktop\Mink\...`).
The generator now writes the repository-relative root
`validation/h_manifest/fixtures/src`, which resolves identically for both sides
because the harness runs each program with the repository root as its working
directory. Retested: `13/13 cases match the Python reference`, exit 0, and a
second run leaves the worktree unchanged. No MINK source changed, so no compiler
regression test applies; the harness itself is the reproducing check.

**Stale warning count (S4).** `docs/validation/M6_EXTERNAL/README.md` §3 stated
the Path B build emits 60 warnings; the current tree emits 62. The line now
gives 62 for the current tree and 60 for the 1.0.5 reference build.

## 7. Verdict

| Item | Status |
| --- | --- |
| M6 | **BLOCKED / PENDING EXTERNAL EXECUTION**; canonical verdict **OUTSTANDING** |
| Package | ready; executed end to end internally, no blocking usability defect found |
| Remaining owner action | send the §2 invitation and return the completed submission form |
| Release-path re-measurement | owed after a host restart (environment fault, not MINK) |
| 1.0.5 / M8 / Linux | unpublished / gated / frozen |

## 8. Files changed

- `docs/validation/M6_EXTERNAL/VALIDATION_RECORD.md` — §11 session record
- `docs/validation/M6_EXTERNAL/ACCEPTANCE_CHECKLIST.md` — operational status
- `docs/validation/M6_EXTERNAL/README.md` — re-run line, warning count
- `docs/roadmap/IMPLEMENTATION_ROADMAP.md` — current next action
- `validation/h_manifest/fixtures/make_fixtures.py` — repository-relative root
- `validation/h_manifest/fixtures/config*.json` — relative `root`, no machine path
- `docs/implementation/SESSION_284_M6_PACKAGE_EXECUTION.md` — this record
