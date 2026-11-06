# Session 190 — Standard-Library Leak Enforcement

Windows x86_64, 2026-11-06. Starting HEAD
`076cf7b1e39d38827b9c8ae6bc2b90f99127d923` (after Session 189).

## 1. Why this milestone

The roadmap's M7 (ecosystem/tooling) is explicitly gated on demonstrated demand,
and M6 needs an outside developer, so neither was open to a code milestone.
Rule 3 of the roadmap — "root-cause fixes before workarounds" — pointed at the
backlog instead. `docs/audits/SYSTEMS_READINESS_AUDIT.md` §7 records a concrete
open defect:

> Input ownership is incomplete across the standard library … The affected
> suites tolerate exit 106, which is why it persisted.

That is a memory-safety hole in a systems language, and the tolerance is what
kept it invisible. The milestone is to restore the ownership invariant in the
test suites and fix whatever it exposes.

## 2. Audit before implementation

`exit 106` is the runtime's `E-R06` leak report: the program finished, but live
allocations remained. Nine suites carried the tolerance:

| Suite | `== 106` tolerances before |
| --- | --- |
| `encoding_lib` | 36 |
| `hashing_lib` | 24 |
| `crypto_lib` | 18 |
| `strings_lib` | 10 |
| `math_lib` | 4 |
| `collections_lib` | 1 |
| `filesystem_lib` | 1 |
| `random_lib` | 1 |
| `time_lib` | 1 |
| **total** | **96** |

Two distinct defects were found by inspecting the assertions rather than the
function names.

**Defect 1 — the tolerance also disabled the checks.** In `encoding_lib` the
value assertions were nested inside `if code == 0`:

```rust
assert!(code == 0 || code == 106, "{name}: exit {code}");
if code == 0 {
    assert_eq!(val, expected, ...);   // never runs when the program leaked
}
```

So a leaking program skipped its correctness assertion entirely. This is how two
wrong expectations survived in the suite (see §3).

**Defect 2 — the audit's own limitation was stale.** The audit named
`hex_encode`, `base64_encode` and `str_trim` as leaking their input. All three
already release it: `hex_encode`, `hex_encode_upper`, `base64_encode` and
`base64_url_encode` each end with `rt_str_free(s)`, and `str_trim` returns
`str_sub(...)`, which releases its input. The real remaining leaks were in the
**test programs**, which dropped owned return values.

## 3. Defects found and fixed

Running the suites with strict assertions surfaced 92 failures. Root causes:

1. **Unfreed owned return values in test programs.** `hex_encode`,
   `base64_encode`, `url_encode`, `hash_sha256` and the `*_decode_alloc`
   tuple functions all return owned `Str` values. The probes printed them and
   never freed them, so the programs leaked. Fixed by releasing every owned
   value, in the shared helpers where possible.
2. **Two assertions that had never been checked.** With the leak fixed the
   values were finally compared:
   - `e20_b64url_encode` expected `base64_url_encode("Hello?")` → `SGVsbG8/`.
     The URL-safe alphabet writes value 63 as `_`, so the correct result is
     `SGVsbG8_`. Confirmed with `base64.urlsafe_b64encode(b"Hello?")`.
   - `e25_b64url_decode` expected 7 bytes from `"SGVsbG8/"`; it decodes to
     `"Hello?"`, which is 6 bytes. Confirmed with Python.

   The stdlib was right in both cases; the expectations had been wrong since
   they were written, and the 106 tolerance guaranteed nobody found out.
3. **`rt_str_concat` does not consume its inputs.** The `_pad2` helper in
   `time_lib` built two numbers and returned `rt_str_concat(t, o)` without
   releasing `t` and `o`. A direct probe confirmed the contract — printing the
   concatenation and freeing only the result still exits 106 — so `_pad2` now
   releases both temporaries after the concat.

Method note: an early bulk edit inserted `rt_str_free(h)` after a
`rt_print_str(h)` that was already followed by a free. The compiler rejected the
double free at build time, which is direct evidence that the ownership checker
enforces what these tests had been opting out of.

## 4. Result

Strictly enforced — exit code must be exactly 0, which means the runtime leak
scan found nothing:

| Suite | Tests | Status |
| --- | --- | --- |
| `crypto_lib` | 18 | strict, green |
| `encoding_lib` | 75 | strict, green |
| `hashing_lib` | 29 | strict, green |
| `math_lib` | 106 | strict, green |
| `random_lib` | 15 | strict, green |
| `time_lib` | 29 | strict, green |
| **total** | **272** | **strict, green** |

## 5. Verification

| Check | Command | Result |
| --- | --- | --- |
| Full regression | `cargo test` | exit 0 — **3,177 passed, 0 failed, 1 ignored, 78 suites** |
| Strict suites | `cargo test --test <suite>` | 18 / 75 / 29 / 106 / 15 / 29 passed, 0 failed |
| Remaining tolerances in the six suites | `grep -c '== 106'` | 0 in every one |
| Formatting | `rustfmt --check` on the changed files | clean |
| Whitespace | `git diff --check` | exit 0 |
| Assertions removed | review of the diff | none; only the `|| 106` clauses and reformatting |

The full-suite total is unchanged at 3,177 because this milestone changed
assertions and probe programs, not the number of tests.

## 6. Deferred — the next part

`strings_lib`, `collections_lib` and `filesystem_lib` keep their tolerances;
they were measured under strict enforcement first:

| Suite | Failing under strict | Tolerances kept |
| --- | --- | --- |
| `strings_lib` | 3 | 10 |
| `collections_lib` | 21 | 1 |
| `filesystem_lib` | 24 | 1 |

`strings_lib` is not mechanical: its failing tests (`s70`, `s72`, `s73`) parse
JSON into an arena, and the comments assert that values returned by
`json_as_str` / `str_index_of` are arena-owned and must **not** be freed with
`rt_str_free`. Whether the runtime's leak scan should count arena strings is an
ownership-contract question, not a missing `free`. Answering it needs the arena
and `strings` contracts reviewed together, so it is recorded as the next part
rather than guessed at under time pressure.

`collections_lib` and `filesystem_lib` look mechanical by comparison — every
failure reported exit 106 with correct output — and are the natural first half
of that part.

## 7. Commits

| Commit | Purpose |
| --- | --- |
| `6c273215d0af35d2b9ac7c3dda737be0b7d91ff7` | six suites enforce leak-free exits; wrong base64 expectations corrected |
| see `git log` for the docs commit | this record and the audit correction |

## 8. Status

| Item | Status |
| --- | --- |
| Six suites enforce E-R06 | verified |
| Real leaks behind the tolerance | fixed (test programs, not stdlib) |
| Assertions that were silently skipped | now execute; two wrong expectations corrected |
| `rt_str_concat` ownership documented in a test helper | verified by probe |
| `strings` / `collections` / `filesystem` | deferred, measured, next part |
| Full regression | green, 3,177 / 0 / 1 |
| Platform scope | unchanged — Windows x86_64 only, no Linux work |

## 9. Exact next action

Continue the same milestone for `collections_lib` and `filesystem_lib` (45
failures, all exit 106 with correct output), then finish with `strings_lib` once
the arena-ownership contract for `json_as_str` results is settled.
