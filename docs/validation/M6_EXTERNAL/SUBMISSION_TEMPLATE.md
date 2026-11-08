# MINK M6 Evaluation Submission Template

**For:** the independent evaluator, or the coordinator recording the evaluator's
returned material
**Protocol:** [`EVALUATION_PROTOCOL.md`](EVALUATION_PROTOCOL.md)

Use this form as-is. A row marked **not attempted** is honest data; it is never a
pass. Do not clean up transcripts or delete failed attempts. If a task was
abandoned, say where and after how long.

## 1. Tester and environment

- Tester identifier or self-description (optional):
- Consent to record this submission verbatim for M6 assessment: yes / no
- Windows version and architecture (for example Windows 11 23H2 x86_64):
- Terminal used (Git Bash / PowerShell / other):
- Installation directory:
- Installation path used: A (supplied binary) / B (built from source)
- `mink --version` output:
- `mink help` output attached: yes / no
- Binary size and SHA-256 (Path A; Path B may report the self-build checksum):
- Repository commit, if the package was a clone:
- Date started / finished:

## 2. Per-task results

| Task | Attempted | Time | Outcome | Expected | Actual | Exit codes | Documentation used | Finding ids |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| T1 |  |  |  |  |  |  |  |  |
| T2 |  |  |  |  |  |  |  |  |
| T3 |  |  |  |  |  |  |  |  |
| T4 |  |  |  |  |  |  |  |  |
| T5 |  |  |  |  |  |  |  |  |
| T6 |  |  |  |  |  |  |  |  |
| T7 |  |  |  |  |  |  |  |  |
| T8 |  |  |  |  |  |  |  |  |
| T9 |  |  |  |  |  |  |  |  |
| T10 |  |  |  |  |  |  |  |  |
| T11 |  |  |  |  |  |  |  |  |
| T12 |  |  |  |  |  |  |  |  |
| T13 |  |  |  |  |  |  |  |  |
| T14 |  |  |  |  |  |  |  |  |
| T15 |  |  |  |  |  |  |  |  |
| T16 |  |  |  |  |  |  |  |  |
| T17 |  |  |  |  |  |  |  |  |
| T18 |  |  |  |  |  |  |  |  |

Outcome values: `completed`, `completed with friction`, `partially completed`,
`blocked`, `not attempted`.

## 3. Findings

Repeat this block for every problem. Do not classify a problem you did not
reproduce; describe what happened and let the coordinator triage it.

```text
Finding id (F1, F2, ...):
Task:
What happened (verbatim message and command):
Expected:
Why you expected that (document and section):
Minimal source file:
Command line:
stdout:
stderr:
Exit code:
Environment:
Reproduces on a second run: yes / no / not tried
Reproduces in a fresh directory: yes / no / not tried
Your suggested classification (optional):
Severity you experienced (S1-S4, optional):
```

## 4. Transcript and attachments

List every file supplied: terminal transcript, notes, screenshots, source
files, generated executables, and any logs. Name them so the coordinator can
match them to task ids. Do not include credentials, personal paths or unrelated
files.

## 5. Overall experience (T18)

Answer in your own words; negative findings are useful. Use the seven T18
questions from the protocol, or attach your notes verbatim.

## 6. Partial or abandoned evaluation

If you stopped early, record:

- last completed task and what you were doing next;
- exact error or blocker;
- whether it was an access problem, a documentation problem, a tool problem,
  or a language problem;
- anything you would need in order to continue.

## 7. Submission

Send the completed form and attachments to the coordinator who gave you the
package. Do not open public issues. The coordinator records the material in
[`VALIDATION_RECORD.md`](VALIDATION_RECORD.md) and applies
[`FINDINGS_TRIAGE.md`](FINDINGS_TRIAGE.md).
