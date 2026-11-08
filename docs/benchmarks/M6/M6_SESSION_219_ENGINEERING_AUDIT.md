# M6 Session 219 — Internal Engineering Audit (Sessions 219–228)

**Date:** 2026-11-08
**Starting HEAD:** `4d65e5b044117137165f99ead771daa0d7f258d3`
**Scope:** compiler architecture, language semantics, diagnostics, standard
library, resource lifecycle, practical workloads, project workflow, CLI, and
final integration review.
**Status of M6:** **OUTSTANDING.** This audit produced internal evidence only.
No independent developer supplied feedback, and nothing here is external
validation.

## 1. Why this record exists

The previous ten-session brief made Sessions 220–228 conditional on tester
evidence and stopped after the Session 219 intake. This run replaced that
workflow with a general engineering audit. It did not contact a tester, invent
feedback, publish a package, create a tag, or change Linux.

## 2. Session outcomes

| Session | Objective | Result |
| ---: | --- | --- |
| 219 | Compiler architecture and fragile boundaries | Reviewed token/parser/type/ownership/MIR/backend layout, CLI dispatch and diagnostics. No new compiler-source defect was confirmed. Undocumented but intentional edges are recorded in §6. |
| 220 | Semantics and type-system reliability | Probed i64::MIN and out-of-range literals, hex boundaries, arity, return mismatch, missing returns, `if` conditions, tuple destructuring, duplicate definitions and duplicate discriminants. No invalid-program acceptance beyond the documented missing-return rule. |
| 221 | Diagnostics and error recovery | Verified codes, spans and explanations for syntax, type, name, arity, tuple and enum errors; `mink explain` lists 50 documented codes and correctly omits runtime codes. |
| 222 | Standard library correctness | Confirmed four contract defects (§3). Probed strings, CSV, JSON, filesystem, collections and SHA-256 against documented behaviour and known vectors. |
| 223 | Memory, ownership and resource lifecycle | Failure-path probes (missing file, malformed JSON, empty CSV record, invalid numeric text) all exit 0 with their inputs released; missing-key `rt_map_get` exits 111 (`E-R11`), now documented. |
| 224 | Real-world workloads | `examples/freq_count` runs correctly; CSV and JSON round-trip workloads and a filesystem round-trip produce the documented results. |
| 225 | Project and build workflow | A fresh directory builds and runs a standalone program; a `mod filesystem;` program builds from the repository root and fails with the documented `E-S01` elsewhere. |
| 226 | CLI and tooling reliability | Help, version, build, run, check, test, explain, invalid command/path/target all match the documented exit contract; `check --json` parses as valid JSON. |
| 227 | Cross-subsystem regression | Focused suites in debug and release (§4). No compiler or runtime source changed, so no broader suite rerun was warranted. |
| 228 | Final audit | Worktree, staged paths, roadmap and audit reconciled; M6 remains outstanding; no publication action. |

## 3. Confirmed findings and fixes

| Id | Finding | Class | Severity | Status |
| --- | --- | --- | --- | --- |
| F1 | `str_trim`, `str_trim_start` and `str_trim_end` trimmed only space, tab, LF and CR. The reference promises "ASCII whitespace", and the module's own `_is_whitespace` helper already included VT (0x0B) and FF (0x0C). | Standard-library defect | S2 (documented behaviour did not hold) | Fixed in `stdlib/strings.mink`; regression `s37b_trim_all_ascii_whitespace`. |
| F2 | The reference said `str_replace` replaces every occurrence. It replaces the first; `str_replace_all` is the repeated variant. The empty-needle cases were undocumented. | Documentation defect | S3 | Reference corrected; regression `s51b_replace_first_occurrence_only` pins both behaviours. |
| F3 | The map reference did not state that `rt_map_get` on a missing key is `E-R11` (exit 111) rather than a sentinel. | Documentation defect | S3 | Reference and exit-code table corrected; behaviour verified by probe. |
| F4 | `encoding.byte_is_whitespace` returned false for VT/FF while documented as an ASCII whitespace check. | Standard-library defect | S3 (no internal callers) | Fixed in `stdlib/encoding.mink`; regression `e38b_is_whitespace_all_ascii`. |

No compiler, MIR, backend, ownership or runtime source was changed.

## 4. Verification evidence

| Check | Configuration | Result |
| --- | --- | --- |
| `cargo test --test strings_lib` | debug | 90 passed, 0 failed, 0 ignored, exit 0 |
| `cargo test --release --test strings_lib` | release | 90 passed, 0 failed, 0 ignored, exit 0 |
| `cargo test --test encoding_lib` | debug | 76 passed, 0 failed, 0 ignored, exit 0 |
| `cargo test --release --test encoding_lib` | release | 76 passed, 0 failed, 0 ignored, exit 0 |
| `cargo test --test stdlib_signatures` | debug | 1 passed, exit 0 |
| `cargo test --test stdlib_docs` | debug | 1 passed, exit 0 |
| `mink check --json` on a type error | debug binary | valid JSON, `success: false`, exit 1 |
| SHA-256 of `""`, `"abc"`, quick-brown-fox | debug binary | matches the published FIPS 180-4 vectors |
| JSON parse/serialize round-trip | debug binary | exact input reproduced; malformed input returns 0 |
| CSV quoted comma/newline round-trip | debug binary | exact round-trip; no leak (exit 0) |
| Filesystem documented return table | debug binary | all values match, handles released |
| Fresh-directory build/run | debug binary | exit 0; `mod filesystem;` follows the documented lookup |

## 5. Artifact state

- Compiler version: `mink 1.0.5`.
- Reference binary: 4,048,384 bytes, SHA-256
  `87a1cae4946f121e292d1a516e6047ad287b6506c5fe52a16208717dc2b6da23`
  (`target/s213_clean/release/mink.exe`), unchanged.
- No tag, GitHub release or npm publication was created.
- The two stdlib fixes are `.mink` sources loaded when a program is compiled,
  so they change program behaviour without changing the compiler binary.

## 6. Observations that are not defects

- A function that declares a return type may fall off the end. This is
  intentional and explicitly covered by `tests/function_annotations.rs`; it was
  not changed.
- Shift counts at or above 64 are masked by the machine instruction
  (`1 << 64 == 1`). The reference pins operands to `Int` but does not define
  out-of-range counts; this is recorded as an unspecified edge, not fixed.
- `mink explain` deliberately omits runtime codes; `E-R04`, `E-R06` and
  `E-R11` are documented in the debugging guide instead.
- The repository compiler resolves `stdlib/` from the working directory; the
  npm layout is the supported installed distribution. This is documented and
  unchanged.

## 7. M6 consequence

None. M6 stays **OUTSTANDING** until an independent developer completes the
required protocol tasks in `docs/validation/M6_EXTERNAL/EVALUATION_PROTOCOL.md`.
The package remains ready for evaluation, and 1.0.5 remains unpublished.
