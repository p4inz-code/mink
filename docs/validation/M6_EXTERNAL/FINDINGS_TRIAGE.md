# M6 External Findings Triage and Closure

**Audience:** the coordinator and repository owner handling returned evaluator
evidence. This is not a task guide for the evaluator.
**Status:** process only; no external finding exists yet.

## 1. Principle

A tester observation is not a confirmed defect. Preserve the report verbatim,
reproduce it against the reference binary, and only then classify it. Never
pre-classify a hypothetical problem as a finding, and never discount criticism
because the expected behavior seems plausible.

## 2. Intake

For each submission:

1. Record the tester's identity or self-description, consent, Windows version,
   installation path, `mink --version` output and measured checksum in the
   tester register in [`VALIDATION_RECORD.md`](VALIDATION_RECORD.md) §1 and in
   the returned submission form.
2. Store the source files and transcript unchanged. Do not edit the tester's
   code or repair their report.
3. Record partial completion explicitly: an absent task is **not attempted**,
   never a pass.
4. Check independence: the tester must not have contributed to MINK or built
   the compiler, and must have used the external package.
5. If independence or consent is missing, record the blocker rather than using
   the evidence for acceptance.

## 3. Evidence sufficiency

A report is reproducible only when it supplies:

- the smallest source file or exact steps;
- the exact command;
- stdout, stderr and exit code;
- the expected result and the document relied on;
- the environment and compiler version/checksum;
- whether it reproduces on a second run and in a fresh directory.

Missing items are recorded as missing. They are not filled in by assumption.

## 4. Classification

Use the existing nine-way classification; do not add a category to avoid an
inconvenient result.

| Class | Test |
| --- | --- |
| Compiler defect | Valid source is rejected, invalid source is accepted, or code generation is wrong. |
| Runtime defect | A generated program crashes, corrupts state, leaks against the documented contract, or returns a wrong result. |
| Standard-library defect | A documented `stdlib` function's result or ownership contract does not hold. |
| Documentation defect | A document states something false, omits a required step, or contradicts actual behavior. |
| Usability issue | The documented path works but costs disproportionate time or invites a predictable mistake. |
| Missing capability | A required outcome cannot be expressed at all; not merely inconvenient. |
| Intentional difference | Behavior differs from another language but matches MINK's documented design. |
| Environment issue | Toolchain, PATH, permissions, terminal, network or machine state, not MINK. |
| Invalid or unreproducible | The report cannot be reproduced from the supplied evidence. |

A documented limitation in the package README §9 is not a defect. A design
choice is not a defect merely because another language behaves differently.

## 5. Severity

| Severity | Meaning | Acceptance effect |
| --- | --- | --- |
| S1 | Blocks installation or any program from running. | Blocks M6 COMPLETE. |
| S2 | Wrong result, crash, corruption, or a documented behavior that does not hold. | Blocks M6 COMPLETE in an attempted task until fixed and independently retested. |
| S3 | Diagnostic or documentation problem with a workaround. | Does not block by itself; documentation defects that blocked a task must be corrected. |
| S4 | Cosmetic or preference. | Recorded only. |

Do not raise severity to force action, and do not lower it to protect the
milestone. If a reported S2 is reproduced as intended behavior, reclassify it
only after citing the document that defines the behavior.

## 6. Status lifecycle

| Status | Meaning | Who may set it |
| --- | --- | --- |
| New | Received, not yet reproduced. | Coordinator |
| Needs evidence | The report is incomplete; the missing items are listed. | Coordinator |
| Confirmed | Reproduced against the reference binary with a minimal case. | Coordinator |
| Not a defect | Intentional, documented limitation, environment issue, or invalid report; reason recorded. | Coordinator |
| Fix in progress | A confirmed defect has a scoped correction. | Owner/implementer |
| Fixed, retest pending | The fix and regression test exist; independent retest is outstanding. | Owner/implementer |
| Closed | The original tester confirms the problem is gone, or the status is explicitly "closed as not a defect". | Coordinator with tester evidence |

Internal regression success moves a finding to **Fixed, retest pending**, never
to **Closed**. A tester unavailable for retest is recorded as a blocker; the
finding is not silently closed.

## 7. Acceptance impact

- Open S1: M6 cannot be COMPLETE.
- Open S2 in an attempted task: M6 is PARTIAL at best.
- Fixed S2 without independent retest: still open for acceptance purposes.
- Documentation defect that blocked a required task: corrected documentation
  and evidence of the correction are required; internal tests do not substitute.
- Missing required task: M6 is PARTIAL or OUTSTANDING, never COMPLETE.
- An S3 with a workaround does not by itself prevent COMPLETE, but it stays
  visible in the findings register.

## 8. Retest request

For each confirmed finding, send the original tester:

- the finding id and the smallest reproducer;
- the fix commit and affected task;
- the exact compiler version and checksum of the retest binary;
- the expected corrected behavior;
- a request to retry only the affected task and state whether the original
  problem is gone.

Record the response verbatim and mark the finding resolved only from that
response. If the tester cannot continue, record the blocker and keep the finding
open.

## 9. Current state

No tester has submitted evidence. The register is empty, no finding is
classified, and M6 remains **OUTSTANDING**. This document defines the process
that will be used when evidence arrives; it is not evidence itself.
