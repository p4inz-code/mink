# M6 External Validation Record

**Status: OUTSTANDING — no independent tester has started.**
**Package prepared:** 2026-11-08 (Session 215)
**Protocol:** [`EVALUATION_PROTOCOL.md`](EVALUATION_PROTOCOL.md)

This file is the only authoritative record of external validation for M6. It is
append-only: entries are added with dates, and earlier entries are never
rewritten to look better. Criticism is preserved word for word.

## 1. Tester register

| # | Background (self-described, consented) | Environment | Started | Finished | Protocol tasks completed |
| ---: | --- | --- | --- | --- | --- |
| — | *no tester yet* | — | — | — | — |

## 2. Recruitment status and blocker (Session 215)

**Blocker: no independent developer is available to this work.**

- The evaluation requires a developer who did not build MINK, working without
  coaching. The only person involved in this work is the repository owner, who
  built MINK and is therefore excluded by definition.
- The coding agent executing these sessions cannot act as the tester: it built
  and verified these very sessions, it reads the implementation records, and its
  results would be internal verification presented as external evidence.
- This execution environment provides no channel to an outside developer: no
  mailing list, no issue tracker access, no external identity, and no
  authorization to contact anyone on the owner's behalf.
- Therefore no invitation was sent, and **no external validation occurred**. The
  package and protocol are complete and ready; the human step is the owner's.

Recruitment requires the owner to name a colleague, peer or community member who
has never contributed to MINK and to send them the invitation below. Until that
person completes the protocol and their evidence is recorded here, M6 remains
OUTSTANDING.

### Invitation template (for the owner to send)

> I'm looking for an independent review of MINK, a small native programming
> language for Windows x64 that I've built. I need someone who has never used it
> and hasn't worked on it to try installing it and building a few programs, and
> to tell me honestly where it wastes your time.
>
> Time: 4–6 hours, split however you like. You'd get a package with the compiler,
> documentation, examples and 18 short tasks with expected outcomes. I won't
> coach you through the language — that's the point — but I'll answer access
> questions.
>
> What I record: your general background (however you want it described), your
> Windows environment, your task results, and your feedback verbatim. Nothing is
> published and no account is needed. If a task is confusing or a message is
> bad, say so; negative findings are the useful ones.
>
> Would you be willing to do this? If yes, I'll send the package and protocol.

## 3. Sessions

| Session | Date | Tester | Tasks attempted | Outcome |
| ---: | --- | --- | --- | --- |
| — | — | — | — | *none* |

## 4. Findings

No findings. Nothing has been submitted, so no finding is classified, no
severity is assigned and none is marked fixed. Empty is the honest state; it is
not evidence that MINK has no defects.

| Id | Task | Class | Severity | Reproducible | Status | Verbatim report |
| --- | --- | --- | --- | --- | --- | --- |

## 5. Retest log

| Finding | Fix commit | Retested by | Date | Resolved? |
| --- | --- | --- | --- | --- |

## 6. Assessment against M6 criteria

| Criterion | Status | Basis |
| --- | --- | --- |
| Independent installation success | NOT TESTED | no tester |
| Documentation comprehension | NOT TESTED | no tester |
| Representative programs compile and run | NOT TESTED externally | internal evidence only |
| Standard-library usability | NOT TESTED externally | internal evidence only |
| Diagnostic quality | NOT TESTED externally | internal evidence only |
| Application development experience | NOT TESTED | no tester |
| Reproducible external defects | NONE REPORTED | no submissions |
| Remaining limitations | DOCUMENTED | package §9; internal audit |

**M6 verdict: OUTSTANDING.** A prepared package and a written protocol are
preparation, not validation.

## 7. Session 219 intake — 2026-11-08

Checkpoint before intake:

- Local `HEAD` equals `origin/main` at
  `f57ce47848a2e7325f61230d3de686ee01ad4f64`, verified after `git fetch origin`.
- The worktree contains only the four user-owned untracked files
  (`MINK UI SETS OFFICIAL.zip`, `freebuff-chat-2026-10-01T19-08-00.600Z.md`,
  `nl1.exe`, `nl1.mink`); no modified, staged or stashed content.
- `docs/validation/M6_EXTERNAL/` holds the package README, the protocol and this
  record, and no submission directory.

Intake result: **no evaluation evidence was supplied.** No tester report,
verbatim feedback, transcript, terminal log, screenshot, recording, source
program or environment description exists in the repository or arrived with
this session. The register in §1 therefore stays empty, and §3–§5 stay empty
with it.

Independence screening: not applicable — no candidate submitted anything to
screen, so nothing is asserted about anyone's independence.

Consent: not applicable — no feedback exists to cover.

Information recorded as missing rather than assumed: tester identity and
self-described background, Windows version and architecture, installation path,
`mink --version` output, measured binary checksum, per-task expected versus
actual results, and the transcript itself.

No invitation was sent and no developer was contacted. The session rules forbid
contact without the owner's explicit authorization, and this environment has no
channel to an outside developer.

**M6 verdict at intake: OUTSTANDING.** Sessions 220–228 are not executed
because each is conditional on tester evidence. The blocking human action is
unchanged: the owner sends the invitation in §2 to a developer who has never
contributed to MINK, and the returned material is recorded here verbatim.

## 8. Session 229–238 readiness pass — 2026-11-08

Baseline: local `HEAD` equals `origin/main` at
`1f6f79adc71795d146df3bdf899109925193a812`; the worktree contains only the four
user-owned untracked files. No tester evidence was supplied, so §§1, 3, 4 and
5 contain no tester data; §2 remains the recruitment blocker, and no acceptance
claim is made.

The package was re-reviewed as an evaluator would encounter it. Five task
expectations contradicted the documented API and would have produced false
defect reports:

- **T9** asked the evaluator to append to a file, but the filesystem module has
  no append mode; `fs_write` replaces the contents. The task now uses an
explicit read-modify-write append and says so. Its missing-file expectation was
also wrong: `fs_read` returns an empty string, `fs_file_size` returns `-1`, and
`fs_exists` returns `false`; the process exits 0 unless the program chooses
otherwise.
- **T10** expected malformed JSON to be rejected with a non-zero exit and a
  position. `json_parse` returns `0` and has no position reporting.
- **T12** expected on-disk SQLite persistence. The engine is in-memory and the
  database is an opaque `Str` blob; persistence across processes is not part of
  the contract.
- **T14** expected a closed port to produce a non-zero process exit.
  `net_connect` returns `-1` and `net_last_error` reports the Winsock reason;
  the program chooses its exit code.

T6 now states the intended variant mix (two data-carrying variants plus one
unit variant), T8 distinguishes `rt_map_has` from the documented `E-R11`
failure of `rt_map_get` on an absent key, and T11 states that `csv_parse_row`
is record-oriented and receives the whole record. T9's expected file-handle
wording was replaced with deletion-after-read because the module exposes no
regular-file handle. The protocol now records partial completion explicitly and
states that internal evidence never satisfies an acceptance condition. T17's
required argument, stderr and stdin APIs (`rt_argc`, `rt_argv`,
`rt_stderr_write`, `rt_stdin_read`) were absent from the language guides; a
short verified section now documents their contracts in `LANGUAGE_BASICS.md`
§1.1.

New evaluator materials: `SUBMISSION_TEMPLATE.md` (per-task evidence, consent,
findings and partial-completion sections), `FAQ.md` (verified onboarding and
runtime traps), `ACCEPTANCE_CHECKLIST.md` (condition-to-evidence map and
verdict definitions), and `FINDINGS_TRIAGE.md` (coordinator classification,
severity, status and retest workflow). `README.md` now carries a handoff
manifest, the current build-source statement and the Session 237 clean-clone
result.

Session 237 clean-clone verification at `1f6f79a`: `cargo build --release`
exit 0 in 21.36 s, 60 warnings, 0 errors; `mink 1.0.5`; binary 4,048,384 bytes;
SHA-256 `67450d40347029de902fb921242c42fa2a4281776f27f8ff84771a286e29717f` (a
normal self-build, different from the reference binary by design); `hello` and
a `mod filesystem;` program both ran from the clone root. The reference binary
still matches 4,048,384 bytes and
`87a1cae4946f121e292d1a516e6047ad287b6506c5fe52a16208717dc2b6da23`.

Focused probes: `fs_write` of 3 bytes followed by `fs_write` of 2 bytes read
back as lengths 3 and 2, exit 0, confirming replacement rather than append.
The new CLI example built and ran twice: without arguments it wrote `usage:
tool <name>` to stderr and exited 2; with argument `alice` and piped `abc` it
printed `alice` and `3` and exited 0. A struct/enum probe with a unit variant
built and printed `1`/`0`; a nested JSON document with a boolean, object and
array parsed and serialized back unchanged; a CSV record with a quoted comma
and embedded newline parsed into three fields (`a,b`, length 11, `3`); and the
existing `examples/inventory_db` SQLite example built and ran with exit 0,
printing the expected filtered, updated and final rows. All probes used the
reference binary; they check that the corrected tasks are expressible and are
not external validation.

**M6 verdict remains OUTSTANDING.** No invitation was sent, no developer was
contacted, and no feedback was fabricated. The owner action is unchanged: send
the §2 invitation and return the evaluator's completed submission form.
