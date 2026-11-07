# Session 201 — Standard-Library Surface and Developer Experience Review

Windows x86_64, 2026-11-07. Starting HEAD
`d3f5dd904e318b6ce000cd7d9f9fa3db3f1889c0` (Session 200, local == origin/main
verified before any change).

## 1. Scope established before auditing

Two questions a developer meets before writing a program:

1. **Can I find the function I need?** — does the standard-library reference
   cover the functions the compiler actually accepts?
2. **Does the CLI fail correctly?** — help, unknown commands, missing files.

No library code was changed in this session; the audit is about the documented
surface and the CLI contract.

## 2. Surface census

`stdlib/*.mink` declares **686** top-level functions. Splitting by the naming
convention used throughout the library:

| Kind | Count |
| --- | --- |
| Public (no leading `_`) | **448** |
| Private helpers (`_`-prefixed) | 238 |

Matching each public name against
`docs/language/STANDARD_LIBRARY_REFERENCE.md` (whole-word, so `log_log` cannot
satisfy a search for `log`) found **385 covered (85.9%)** and **63 missing**:

| Module | Missing public functions |
| --- | --- |
| math | 20 |
| http (+ networking helpers) | 13 |
| time | 12 |
| log | 7 |
| byte | 7 |
| rt (string intrinsics re-exported by `strings`) | 2 |
| str (`str_is_space`) | 1 |
| net (`net_set_last_error`) | 1 |

Every one is reachable from a program: a documented API that silently omits
`math_float_ceil`, `math_float_pow`, `time_hour`, `log_warning`,
`http_url_host`, `byte_is_alpha` and 57 more is a real developer-experience
gap, but not a behaviour defect — the functions work and are tested by their
module suites.

## 3. Fix — appendix plus a permanent guard

* **`docs/language/STANDARD_LIBRARY_REFERENCE.md` §26** now lists all 63 with
  their source signatures, grouped by module (byte/strings, math, time,
  logging, HTTP/net, runtime string intrinsics). Signatures are copied from
  `stdlib/*.mink`.
* **`tests/stdlib_docs.rs`** scans every `stdlib/*.mink` for top-level `fn`
  declarations, skips `_`-private helpers, and fails with the missing names if
  any public function is absent from the reference. It also asserts the scan
  saw more than 400 public functions, so a parsing change cannot make the
  guard pass vacuously.

Result after the fix: **448 checked, 0 missing**.

## 4. CLI contract (verified, no change needed)

| Invocation | Exit | Output |
| --- | --- | --- |
| `mink help` | 0 | full command list |
| `mink` (no arguments) | 0 | usage |
| `mink bogus` | 1 | `unknown command 'bogus'` + `Run 'mink help' for usage.` |
| `mink run missing.mink` | 1 | `failed to read 'missing.mink'` |
| `mink build missing.mink` | 1 | same diagnostic |
| `mink check missing.mink` | 1 | same diagnostic |

Measurement note: the first probe piped output through `head` and read `$?`
from the pipeline, which reported `head`'s 0. Capturing `mink`'s own status
shows the correct 1. The CLI was never wrong; the measurement was.

## 5. Verification

* `cargo test --test stdlib_docs`: **1 passed** (448 public functions checked,
  0 missing).
* No compiler/runtime/stdlib source changed, so the Session 200 full
  regression still stands; the suite count for the release audit becomes
  **79** with this new guard.
* `rustfmt --check` clean on the new test.

## 6. Residual

* The guard is a **mention** check, not signature verification: a function can
  be listed with a stale signature and the test still passes. §26 signatures
  were reviewed against the declaration lines when written.
* §24 ("What is not verified") still applies: trigonometry, logarithms and
  most HTTP/net helpers are source-documented, not executed by this audit.
* Documentation is Windows x86_64 only, matching the reference's own scope.

## 7. Reproduce

```
cargo test --test stdlib_docs
```

## 8. Verdict

**PASS — no code defect; one documentation gap (63 public functions) closed
with an appendix and a permanent guard.** The CLI's help/error contract was
verified correct.
