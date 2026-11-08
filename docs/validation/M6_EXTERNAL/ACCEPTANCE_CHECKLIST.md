# M6 Acceptance and Closure Checklist

**Canonical thresholds:** [`EVALUATION_PROTOCOL.md`](EVALUATION_PROTOCOL.md) §6.
This checklist does not lower or replace them.

## 1. Canonical verdicts

- **COMPLETE** — every condition in §2 is satisfied by independent evidence.
- **PARTIAL** — evaluation began, but a required task is missing or an open S2
  defect remains in an attempted task.
- **OUTSTANDING** — no independent developer has completed the protocol.

Descriptive states used in reports:

- **Failed** — evaluation was attempted but cannot satisfy acceptance without
  reopening work (for example an open S1, or a confirmed open S2 in an
  attempted task). It is recorded as PARTIAL, not as a fourth acceptance
  verdict.
- **Inconclusive** — evidence exists but is insufficient to decide a condition
  (missing consent, missing task record, unreproducible report). The affected
  condition stays NOT TESTED, so the verdict stays PARTIAL or OUTSTANDING.

A ready package is not acceptance. Internal tests, agent runs and repository
audits are never external evidence.

## 2. Condition-to-evidence map

| # | Condition | Required evidence | Current status |
| ---: | --- | --- | --- |
| C1 | At least one independent developer completed T1, T2, T3, T6, T8, T9, T17 and T18. | Tester register (§1), per-task submission form, transcript, and a completed row for each required task. | **NOT TESTED** — no tester |
| C2 | No open S1 defect, and no open S2 defect in a task that was attempted. | Findings register with each finding's class, severity and status; no open S1/S2. | **NOT TESTED** — no findings reported |
| C3 | Any S2 defect found was fixed and independently retested. | Fix commit, permanent regression test, and a tester retest entry stating the original problem is gone. | **NOT TESTED** — no S2 found |
| C4 | Documentation defects that blocked a task are corrected. | Documentation commit showing the correction, plus the tester's finding identifying the blocked task. A retest is requested but is not a separate threshold. | **NOT TESTED** — no tester |
| C5 | Tester verbatim feedback is recorded and consent covers its use. | Signed/recorded consent and verbatim feedback in the validation record. | **NOT TESTED** — no tester |

## 3. Independence and consent gate

Before any condition can pass, record:

- the tester did not contribute to MINK;
- the tester did not build the compiler or implementation;
- the tester used the external package;
- the tester consented to the recorded environment, transcript and feedback;
- the coordinator did not coach the language or edit the tester's code.

If any item is missing, the submission is evidence of an attempted evaluation,
not acceptance evidence.

## 4. Required task coverage

C1 needs all eight required tasks: **T1, T2, T3, T6, T8, T9, T17, T18**. Optional
tasks add confidence but cannot replace a missing required task. A task marked
"not attempted" is missing, even if every other task passed.

## 5. Findings and retest gate

- Open S1: C2 fails.
- Open S2 in an attempted task: C2 fails; M6 is PARTIAL at best.
- S2 fixed internally: C3 is **Fixed, retest pending**, not passed.
- Tester confirms the original problem is gone: C3 can pass for that finding.
- Tester unavailable: record the blocker; C3 stays open.
- Documentation defect that blocked a task: fix it and record the correction
  (C4); request a retest, but do not treat the retest as a separate threshold.
- S3/S4 with workarounds: recorded, but do not block COMPLETE by themselves.

## 6. Current assessment

| Condition | Status |
| --- | --- |
| C1 independent required-task coverage | NOT TESTED |
| C2 no open S1/S2 | NOT TESTED |
| C3 S2 fixes independently retested | NOT TESTED |
| C4 blocked-task documentation corrected | NOT TESTED |
| C5 verbatim feedback and consent | NOT TESTED |

**M6 verdict: OUTSTANDING.** No independent developer has started the
evaluation. The package is ready; the owner action is to send the invitation and
return the completed submission form.

## 7. Closure sign-off (to be completed only with evidence)

- Tester and consent recorded: ______
- Required tasks completed: ______
- Findings register closed or explicitly accepted: ______
- Retests recorded: ______
- Documentation blockers corrected: ______
- Final verdict: COMPLETE / PARTIAL / OUTSTANDING
- Evidence location: `docs/validation/M6_EXTERNAL/VALIDATION_RECORD.md`
