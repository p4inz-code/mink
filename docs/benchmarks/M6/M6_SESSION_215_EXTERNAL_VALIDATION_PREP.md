# Session 215 — External Validation Preparation

Windows x86_64, 2026-11-08. Starting commit
`e309fd410d0c8ff5320ac733c2ac2845d24a3600`, local `main` == `origin/main`
verified before work began.

## 1. Objective

Prepare a self-contained package and a structured evaluation so that an
independent developer can install, learn and exercise MINK without private
project knowledge, then recruit and onboard that developer.

## 2. What was inspected first

- `README.md` — install paths, published-state caveat, limitations.
- `docs/language/` — installation, language basics, grammar, type system,
  standard-library reference, error system, debugging and testing.
- `examples/` — 15 single-file practical programs.
- `benchmarks/m6/session176/` — ten progressive tutorial programs.
- `validation/` — ten application-sized programs with Python references.
- `mink --help` on the 1.0.5 release binary — command surface, exit codes.

The documentation and examples already existed and are good enough to be the
package's core. The package therefore **indexes** them instead of duplicating
them, which also keeps a single source of truth for anything corrected later.

## 3. Package produced

`docs/validation/M6_EXTERNAL/`:

| File | Contents |
| --- | --- |
| `README.md` | package index, two install paths (supplied binary, build from source), exact version and SHA-256, quick start, reading order, example map, test instructions, known limitations, bug-reporting format and severity scale, consent terms |
| `EVALUATION_PROTOCOL.md` | rules for coordinator and tester, per-task record fields, 18 tasks with expected outcomes, coverage map, retest rule, M6 acceptance thresholds |
| `VALIDATION_RECORD.md` | tester register, recruitment blocker, invitation template, append-only finding and retest tables, criterion-by-criterion status |

Design choices:

- **Outcomes, not solutions.** Each task states observable behaviour, so any
  correct program passes and the tester is not handed the implementation.
- **Time boxes.** A tester who is stuck records the blockage and moves on; that
  is the most valuable data and should not be lost to perfectionism.
- **No implementation records.** The package explicitly excludes
  `docs/implementation/`, `docs/benchmarks/` and `docs/audits/`; reading them
  would invalidate the documentation-usability result.
- **Version honesty.** The package states that 1.0.5 is unpublished and that the
  registry serves 1.0.4, and tells the tester not to install from npm. This
  avoids a false failure caused by the wrong artifact.
- **Checksum verification.** A tester can prove which binary they ran, so a
  defect report is attributable to a version.

## 4. Recruitment

**No independent developer was identified, invited or onboarded.** The blocker
is recorded in `VALIDATION_RECORD.md` §2:

- the owner built MINK and is excluded by definition;
- the executing agent built these sessions and has read the implementation, so
  its own results are internal verification, not external evidence;
- this environment exposes no channel to an outside developer and no authority
  to contact anyone on the owner's behalf.

An invitation message is prepared for the owner to send. No invitation was sent
automatically, and no feedback was invented.

## 5. What this session does not claim

- It does not claim M6 is complete, partially complete, or started.
- It does not claim any developer has seen MINK.
- It does not convert internal tests, prior practical validation (M2, 228/228) or
  the 3,262-test regression into external evidence. Those are internal results
  and are recorded as such.

## 6. Gate result

| Gate item | Result |
| --- | --- |
| Validation package prepared | PASS (`docs/validation/M6_EXTERNAL/`, 3 documents) |
| Evaluation protocol documented | PASS (18 tasks, coverage map, thresholds) |
| Independent developer identified and invited | **NOT DONE — no available developer; blocker recorded** |
| Package delivered | NO — nothing to deliver to |
| Project-side work committed and pushed | Yes (see §7) |
| M6 status after this session | **OUTSTANDING** |

Per the session's own rule, preparation is not validation. M6 remains open.

## 7. Commit

Commit message: `docs: add the M6 external validation package and protocol`.
Pushed to `origin/main`; local and remote verified equal after the push.
