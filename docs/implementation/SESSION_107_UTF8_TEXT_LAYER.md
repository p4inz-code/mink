# Session 107 (continuation) — Windows Python Capability Parity: UTF-8 Code-Point Layer (L05)

This report covers the second half of Session 107. The first half — typed dynamic
collections + `str_split`/`str_join` — is recorded in
`docs/implementation/SESSION_107_TYPED_COLLECTIONS_STRING_SPLIT_JOIN.md` and was already
committed and pushed (`0c0ab7a`, `48ed337`) when this half began. Because that work had
already landed, this half (a) verified and reconciled it, and (b) implemented one new
coherent parity tranche: **the UTF-8 code-point layer (L05)**.

## 1. Starting commit

`48ed337` — "test+docs: Session 107 permanent regression tests, parity matrix, session
report" (local `HEAD` == `origin/main` == `48ed337` at session start). The Session 106
close (`568dc04`) is an ancestor two commits back; the mission's expected "starting commit
`568dc04`" was already superseded by the first half of Session 107.

## 2. Ending commit

`HEAD == origin/main` after the final push; the exact hashes are recorded in §20 and §21
(feature `5f2a935`, formatting `c3b8091`, documentation commit as listed).

## 3. Selected parity tranche

**UTF-8 code-point layer (matrix row L05 — "Strings as Unicode text")**, plus the two
latent UTF-8 defects found while building it.

## 4. Reason for selecting it

From the Phase 2 criteria and the user's direction:

1. **Closes a genuine P1 parity blocker.** L05 was `INTENT. DIFF. + PARTIAL`, `P1`,
   `Blocks = Y`, and its gap text named exactly what was missing: "No UTF-8
   validation/decoding, no code-point iteration, ASCII-only case ops, length is bytes not
   code points." Wave B/H share it, and it is the dependency for Unicode-aware text work.
2. **Manageable dependencies.** The whole tranche is stdlib code over the existing
   `rt_str_*` / `rt_vec_*` primitives. No new intrinsics, no IR/emitter/runtime change, so
   no calling-convention, register or PE risk.
3. **Real usability.** Byte-length-vs-code-point bugs are the classic failure mode for any
   text tool; decode/encode/char indexing make non-ASCII input safe.
4. **No Linux development required**, and nothing Windows-specific was introduced.
5. **Does not destabilize verified functionality** (only `stdlib/encoding.mink` changed;
   regression evidence in §13).
6. **Completely verifiable** under the Comprehensive Verification Standard: native PE
   execution, exact output/exit codes, boundary/invalid input, and explicit leak checks.

Foundational capabilities were preferred over convenience features, and the tranche was
chosen from the actual row data rather than from the stale headline (see §16).

## 5. Original capability state

`stdlib/encoding.mink` already had `utf8_validate(s) -> Bool` and
`utf8_char_count(s) -> Int`, both tested by `tests/encoding_lib.rs` e26–e31. The matrix row
for L05 nevertheless said "No UTF-8 validation/decoding", which was partly stale — but the
existing functions had **two real defects** and the rest of the layer was genuinely absent:

| Capability | Before |
|---|---|
| `utf8_validate` | Present but accepted overlong 2-byte forms (`C0 80`, `C1 BF`) and UTF-16 surrogates (`ED A0 80` … `ED BF BF`); leaked any heap argument |
| `utf8_char_count` | Present; leaked any heap argument; counted code points with an advance rule that did not match malformed input |
| Decode bytes → code points | Missing |
| Encode code points → bytes | Missing |
| Code-point indexing / byte-offset mapping | Missing |
| Code-point slicing | Missing |

The leak was invisible to the existing tests because the harness accepts `exit 106`
(E-R06 "memory leak: live allocations remain at exit") as success and then skips the
assertion entirely — `if code == 0 { assert_eq!(...) }`.

## 6. Implementation changes

All changes are in the standard library and its tests.

`stdlib/encoding.mink` (mirrored into `npm/mink/stdlib/encoding.mink`):

- **Private helpers** over plain `Int` bytes (no ownership involvement):
  - `_utf8_seq(b, b2, b3, b4) -> (Int, Int)` — the single decoder used by every public
    function. Returns `(advance, code_point)` for a well-formed sequence, and `(1, -1)`
    for anything invalid, so one invalid byte is consumed alone and progress is
    guaranteed. Rejects `C0`/`C1`, 3-byte overlong, surrogates, 4-byte overlong and
    `> U+10FFFF`.
  - `_utf8_fix(cp) -> Int` — maps a code point to its encodable value (`U+FFFD` when
    negative, a surrogate, or above `U+10FFFF`).
  - `_utf8_enc_len(cp) -> Int` — UTF-8 length after `_utf8_fix`.
- **Fixed** `utf8_validate` — now built on `_utf8_seq` and frees its consumed `Str`.
- **Fixed/redefined** `utf8_char_count` — defined as the length of `utf8_decode(s)`, so the
  count and the decode agree for every input (including malformed), and `s` is freed.
- **New** `utf8_decode(s) -> Vec<Int>` — code points with `U+FFFD` replacement; capacity is
  clamped to at least 1 because `rt_vec_new(0)` raises E-R08.
- **New** `utf8_encode(codes: Vec<Int>) -> Str` — two-pass (size, then write); unencodable
  code points become `U+FFFD`; the `Vec` is consumed and freed.
- **New** `utf8_char_at(s, index) -> Int` — code point at a code-point index, or `-1`.
- **New** `utf8_byte_index(s, char_index) -> Int` — byte offset of a code point;
  `char_index == count` yields the byte length (end iterator); otherwise `-1`.
- **New** `utf8_slice(s, start, end) -> Str` — code-point slice, clamped; only whole code
  points, so the result is valid UTF-8 whenever the input is.

`npm/mink/stdlib/strings.mink` and `npm/mink/stdlib/encoding.mink`:

- Re-synced from `stdlib/`. The bundled `strings.mink` had been stale since Session 99 and
  **did not contain `str_split`/`str_join`**: the installed npm package never received the
  Session 107 string functions even though the matrix reported the row execution-verified.

Tests:

- `tests/encoding_lib.rs` — 18 new strict (`exit 0` required) `s107_utf8_*` regressions
  covering ownership, validation boundaries, decode/encode, round-trip, indexing and
  slicing; `e28` corrected (see §8 B4).
- `tests/release.rs` — `s107_npm_stdlib_bundle_matches_repo_stdlib`, a permanent guard that
  every `stdlib/*.mink` is byte-identical to its `npm/mink/stdlib/` counterpart.
- `tests/cli.rs` — an `#[ignore]`d regression recording a pre-existing compiler defect
  (§8 B6), so the reproduction is preserved rather than lost.

## 7. Architecture changes

**None to the compiler or runtime.** The UTF-8 layer is stdlib-only, built from
`rt_str_len`, `rt_str_byte`, `rt_str_set_byte`, `rt_str_alloc`, `rt_str_free` and
`rt_vec_new`/`rt_vec_push`/`rt_vec_len`/`rt_vec_get`/`rt_vec_free`. Consequences:

- No new intrinsic, IR node, emitter path, calling-convention or PE change, so the verified
  Windows runtime surface is untouched.
- Ownership follows the established stdlib convention: a `Str` parameter is consumed and
  freed by the callee (the pattern `str_split`/`str_join` already use). Freeing an
  immutable image literal is a no-op, so literals and heap strings are both safe.
- Windows-agnostic by construction: there is no Windows-only abstraction, so a future
  Linux backend can use the same module unchanged (Linux itself remains frozen — §19).

## 8. Bugs discovered

| # | Bug | Severity | Introduced by |
|---|---|---|---|
| B1 | `utf8_validate` / `utf8_char_count` consumed a heap `Str` without freeing it → E-R06 leak on every heap input | Real ownership defect (pre-existing) | Session 55 |
| B2 | `utf8_validate` accepted overlong 2-byte forms (`C0 80`, `C1 BF`) and encoded surrogates (`ED A0 80` …) as valid UTF-8 | Real correctness defect (pre-existing) | Session 55 |
| B3 | `utf8_decode` called `rt_vec_new(0)` for empty input → E-R08 | Defect introduced *and* fixed inside this session | This session |
| B4 | Existing test `e28_utf8_invalid_continuation` used `C8 80`, which is **valid** UTF-8 (U+0200), and its assertion never ran because the function leaked | Broken test / masked verification | Session 55 |
| B5 | `npm/mink/stdlib/strings.mink` drifted from `stdlib/` → installed users never received `str_split`/`str_join` | Distribution defect (real) | Session 107 (first half) |
| B6 | Compiler fails to resolve later statements in programs that import a module and call its functions repeatedly (`E-M04`, and `E-T05` on a nested-call variant) | Real compiler defect, **pre-existing** (reproduces in shipped 1.0.1) | pre-Session 107; **not fixed** |

Documentation defects found and fixed while reconciling the matrix: five rows were missing
their trailing `|` (L08, L11, L12, S11, S16), the §5.4 `S18`/`S19`/`S20` rows had been
merged into one line, and the aggregate tables (§10) plus the §3.10 language totals did not
match the rows.

## 9. Root causes

- **B1** — `encoding.mink`'s UTF-8 helpers returned scalars and simply dropped the consumed
  string; `strings.mink`'s documented convention (return the string, or free it) was not
  followed. The test harness could not see it: exit 106 was accepted and the assertion was
  then skipped.
- **B2** — the validator applied overlong checks only to 3-byte and 4-byte forms, and never
  checked the surrogate range at all; 2-byte overlong forms were not checked.
- **B3** — `rt_vec_new` requires a positive capacity (E-R08); `utf8_decode` sized the vector
  from the string's byte length, which is 0 for an empty string.
- **B4** — the test's bytes were chosen as "an invalid continuation" but `C8 80` is a valid
  minimal 2-byte sequence; the disabled assertion hid the mismatch.
- **B5** — the stdlib exists twice (repository + npm bundle) with no sync mechanism, no
  packaging script and no test; any stdlib edit silently fails to reach installed users.
- **B6** — not root-caused. Characterized by probing: identical program shape with a
  locally-defined function compiles; a minimal custom module reproduces nothing; the
  trigger is content-dependent (which module is imported) and call-count dependent; the
  failure is deterministic for a given input; and the shipped `npm/mink/bin/mink.exe` 1.0.1
  fails the same input, so it is not a Session 107 regression. Diagnosing it was out of
  scope for this tranche.

## 10. Fixes

- B1: `rt_str_free(s)` before returning in `utf8_validate`; `utf8_char_count` now delegates
  to `utf8_decode`, which frees.
- B2: single `_utf8_seq` decoder with `b >= 194` for 2-byte (rejecting `C0`/`C1`) and an
  explicit surrogate exclusion (`cp < 55296` / `cp > 57343`) for 3-byte.
- B3: capacity clamped to 1; with the loop guarded by the byte length the empty case yields
  an empty vector.
- B4: `e28` now uses `C8 20` (a genuine invalid continuation), requires a clean exit, and no
  longer skips its assertion.
- B5: both bundled modules re-copied from `stdlib/`, plus a permanent byte-equality guard.
- B6: **not fixed** — recorded as an `#[ignore]`d test with the exact reproduction (§8).
- Docs: rows repaired, aggregates recomputed from the rows (§16).

## 11. Native execution evidence

Every probe was compiled to a standalone Windows PE with `target/debug/mink.exe build` and
executed natively; exit codes and stdout are recorded exactly.

Validation (Python 3.11 `bytes.decode("utf-8")` used as the ground truth for the ten
boundary cases):

| Input | Python | MINK (probe `p_t2`, `p_u4`) |
|---|---|---|
| `C0 80` (overlong NUL) | invalid | `0` |
| `C1 BF` (overlong U+007F) | invalid | `0` |
| `C2 80` (U+0080) | valid | `1` |
| `ED A0 80` (U+D800) | invalid | `0` |
| `ED 9F BF` (U+D7FF) | valid | `1` |
| `EE 80 80` (U+E000) | valid | `1` |
| `E0 80 80` (overlong) | invalid | `0` |
| `F0 90 80 80` (U+10000) | valid | `1` |
| `F4 90 80 80` (U+110000) | invalid | `0` |
| `F0 80 80 80` (overlong) | invalid | `0` |

Capability probes (all exit `0`, i.e. leak-free):

| Probe | Input / call | Exact result |
|---|---|---|
| `p_t1` | `utf8_validate` of `héllo` bytes `68 C3 A9 6C 6C 6F` | `1` |
| `p_t1` | `utf8_char_count` of the same | `5` (code points, not 6 bytes) |
| `p_t1` | `utf8_validate("")` (literal) | `1` (literal free is a no-op) |
| `p_t9` | `utf8_char_count(F0 9F 98 80)` (U+1F600) | `1` |
| `p_t7` | `utf8_decode(héllo)` | length `5`, then `104 233 108 108 111` |
| `p_t8` | `utf8_decode(C0 80 FF)` | length `3`, then `65533 65533 65533` |
| `p_t4` | `utf8_encode([104, 233, 108])` | length `4`, then `104 195 169 108` |
| `p_t4` | `utf8_encode([55296, -1, 1114112])` | length `9`, then `EF BF BD` three times |
| `p_t10` | `utf8_encode([128512, 2048])` | length `7`, then `F0 9F 98 80 E0 A0 80` |
| `p_t5` | `utf8_char_at(héllo, 1)` / `(…, 9)` | `233` / `-1` |
| `p_t5` | `utf8_byte_index(héllo, 5)` | `6` (byte length; char count is 5) |
| `p_t6` | `utf8_slice(héllo, 1, 4)` | length `4`, then `195 169 108 108` |
| `p_t6` | `utf8_slice(héllo, 3, 1)` | length `0` |

Negative controls (proving the new tests detect the defects):

- Removing the `rt_str_free(s)` fix from `utf8_validate` reproduces
  `exit 106` and fails `s107_utf8_validate_heap_input_is_leak_free`.
- Restoring the old validation bounds fails both validator tests, printing `1 1 1` for
  (`C0 80`, `C1 BF`, `C2 80`) and `1 1 1 1` for the four surrogate/boundary cases — direct
  execution evidence that the pre-session validator accepted `C0`/`C1` and surrogates.
- Appending a byte to the bundled `strings.mink` fails
  `s107_npm_stdlib_bundle_matches_repo_stdlib` with the exact module named.

## 12. Test counts

| Target | Count |
|---|---|
| `tests/encoding_lib.rs` | 75 passed (57 pre-existing + 18 new `s107_utf8_*`) |
| `tests/release.rs` | 67 passed (66 + the bundle drift guard) |
| `tests/cli.rs` | 74 passed, 1 ignored (the defect-record test was added) |
| Session 107 continuation native probes | 10 programs, all pass / all leak-free |

## 13. Regression results

The full `cargo test --no-fail-fast` run exceeded the environment's 10-minute command cap,
so it was executed as one 10-minute run plus grouped reruns of the outstanding targets. All
targets were executed; the evidence is the per-target summaries.

- Timed full run: **48 target summaries, 2 033 passed**, 1 failed (see §15).
- Remaining 9 targets run separately: `strings_lib` 73, `struct_destructure` 28,
  `sum_types` 44, `time_lib` 16, `try_operator` 12, `tuple_destructure` 40, `tuples` 34,
  `typecheck` 163 — all passed; `windows_hardening` 18 passed when run in groups/isolated
  (see §15).
- Totals: **2 461 passed, 0 product failures, 2 ignored** (the pre-existing `adversarial`
  test and the new recorded defect), across the library unit tests, main, and all 56
  integration targets.
- Compiler re-verification after the formatting commit: `--lib` 62, `parser` 46,
  `typecheck` 163, `mir` 25, `hir` 34, `semantics` 98, `ownership` 75, `backend` 43 — all
  pass.
- Previously-verified areas re-exercised and green: strings, allocator, filesystem,
  process, environment, argv, stdin/stdout/stderr, time, random, crypto, TCP, UDP, HTTP
  GET/POST, CLI, module resolution, Map/Set/Vec, R06 locations, home directory, JSON,
  hashing, math, encoding.

## 14. Leak/ownership evidence

- The runtime exits **106** (E-R06) when any allocation is live at exit, so every test
  asserting `exit 0` is a leak check. The 18 new `s107_utf8_*` tests all require `exit 0`,
  including heap-owned inputs to every function.
- Ownership contract verified: `utf8_validate`, `utf8_char_count`, `utf8_decode`,
  `utf8_char_at`, `utf8_byte_index` and `utf8_slice` free their consumed `Str`;
  `utf8_encode` frees its consumed `Vec<Int>`; returned `Vec<Int>`/`Str` values are owned by
  the caller (the tests free them).
- Immutable image literals are accepted by every entry point without double-free
  (`p_t1`, `p_u2`) because freeing a non-heap string is a no-op.
- The negative control in §11 shows the leak test fails (exit 106) without the fix.

## 15. Flaky-test analysis

- **Known TCP timing tests** (`tcp_mink_client_checksum_multirecv`,
  `tcp_mink_server_echo_repeated_connections`, `tcp_repeated_connect_close_cycles`, plus
  `udp_loopback_roundtrip_mink_to_rust`): running the whole `windows_hardening` target in
  parallel exceeded the 10-minute cap, while each of the 18 tests passes when the target is
  run in groups or individually (6 allocator, 3 http, 5 fs/strings/env/network, 4 network).
  The other 14 pass in parallel. Classification: **infrastructure-only** (parallel
  loopback port contention), consistent with the Session 106 finding. No TCP/UDP code was
  touched by this session.
- **New observation: `linux_h01_http_execution_verified` (in `tests/process_lib.rs`)** failed
  during the timed parallel full run (`exit 1`, empty stdout/stderr) and **passed 3/3 times
  in isolation** afterwards. It builds a Linux ELF HTTP client and runs it under WSL against
  a loopback server; the failure mode is contention on the shared WSL/loopback port under
  load. Classification: **infrastructure-only, not a product regression** — it is not
  affected by a stdlib change, and it is Linux-frozen code that this session did not touch.
  The exception is documented here and not broadened; it was not classified as product
  flakiness without evidence.

## 16. Parity matrix changes

All matrix edits are evidence-backed; the aggregate counts were recomputed from the rows
with a field-exact scan after repairing the malformed rows.

Row changes:

| ID | Before | After | Evidence |
|---|---|---|---|
| L05 | `INTENT. DIFF. + PARTIAL`, `P1`, `Blocks = Y` | same status, `P2`, `Blocks = N` — **P1 blocker closed** | the UTF-8 layer above; 18 strict tests + 10 native probes |
| S01 | `EXECUTION VERIFIED`, `P1`, `Blocks = Y` | `P2`, `Blocks = N` | Session 107 first half implemented `split`/`join`; remaining items are P3 partition and L05 Unicode-aware split |
| L08 | `EXECUTION VERIFIED`, `P1`, `Blocks = N` | `P2`, `Blocks = N` | stale `P1` after the Session 106/107 closure; remaining items are P3 ergonomics |
| S04 | gap "No UTF-8 encode/decode" | gap narrowed to "UTF-8 covered; no UTF-16/latin-1 codecs" | this session |
| R06 | `VERIFIED`, `P1`, `Blocks = Y` | `P2`, `Blocks = N` | the row's own Session 100 evidence |
| W14 | `VERIFIED`, `P1`, `Blocks = Y` | `P2`, `Blocks = N` | the row's own Session 100 evidence |
| P02 | `MISSING`, `P1`, `Blocks = Y` | `VERIFIED`, `-`, `Blocks = N` | bundled `npm/mink/stdlib` + the new drift guard |
| T06 | `PARTIAL`, `P1`, `Blocks = Y` | `VERIFIED`, `-`, `Blocks = N` | `src/driver.rs` `resolve_module_path` (the mechanism behind the VERIFIED L67) |

Aggregate corrections (§3.10, §10.1–§10.4, §10.6, §10.7):

- Status: **73 VERIFIED · 2 EXECUTION VERIFIED · 1 VERIFIED (INTENT. DIFF.) · 1 VERIFIED
  (partial) · 13 INTENT. DIFF. · 1 INTENT. DIFF. + PARTIAL · 37 PARTIAL · 3 PLANNED ·
  6 N/A · 2 N/A (INTENT.) · 93 MISSING** = 232.
- Priority: **P1 26 · P2 91 · P3 54 · no-gap 61** = 232 (P1 now equals the `Blocks = Y`
  set exactly).
- **Parity blockers reduced from 44 (Session 98 baseline) / 43 (Session 106 headline) to
  26**, all P1, by wave: A 1 (S74) · B 4 (L34, S02, S06, S36) · C 2 (S28, S69) · D 4 (S41,
  S42, S44, S62) · E 4 (R19, R20, S71, S73) · F 6 (L68, P03–P06, P09) · G 5 (R01, S75,
  S78, T04, T07).
- The reduction is not new work alone: it removes rows that **prior sessions had already
  delivered but never removed from the blocker list** (R06, W14, P02, T06, L67, L10–L12),
  plus this session's L05 and the previously-flipped L08/S01. The exact `Blocks = Y` ID set
  is now stated in §10.4 so the arithmetic is auditable; no percentage is claimed.
- Rows requiring work: 171 (15 XL · 30 L · 83 M · 37 S · 6 S-M); 61 rows need none.
- Repaired defects: the merged `S18`/`S19`/`S20` line and five rows missing a trailing `|`.

Windows base platform remains **COMPLETE / STABLE** (0 P0, 0 P1 on the base rows).

## 17. Remaining gaps

- **L05 residuals** (P2/P3, not blocking): non-ASCII-aware case conversion and the Unicode
  database (categories/normalization/collation) — the letters' `str_to_upper`/`str_to_lower`
  remain ASCII, and the full DB is matrix row S03 (P3). `rt_str_len` remains a byte length
  by design; code-point length is `utf8_char_count`.
- No `maxsplit`, no regex-based split, no case-folding: `str_split` splits on a literal
  delimiter only (S02 regex is still a blocker).
- The blocker pool is unchanged otherwise: L34 exceptions, S02 regex, S06 numeric parsing,
  S28 directory traversal, S36 CSV, S41 SQLite, S42/S44 compression/zip, S62 TLS, S69
  time formatting, S71/S73 concurrency/async, S74 logging, S75/S78 test framework, T04
  REPL, T07 runner, L68 packages, P03–P06/P09 packaging.
- **B6 (compiler call-resolution defect) is open** and unrelated to this tranche.

## 18. Known limitations

- The stdlib is duplicated (`stdlib/` and `npm/mink/stdlib/`) with no generator; it is kept
  honest only by the new drift-guard test.
- `utf8_decode` allocates a `Vec<Int>` sized by the byte length (one word per code point at
  worst), and `utf8_char_count` delegates to it — correct and leak-free, but not the
  zero-allocation path a hot loop would want.
- Malformed-input replacement follows MINK's documented one-invalid-byte-at-a-time rule; for
  some malformed byte runs the *grouping* of `U+FFFD` may differ from CPython's codec, which
  groups maximal invalid subsequences. Valid input matches Python exactly.
- `utf8_slice` is O(n) over the string (no cached index).
- The recorded compiler defect (§8 B6) means programs that import a module and call its
  functions many times can fail to compile; it is documented, not fixed.

## 19. Linux freeze confirmation

**Linux remains FROZEN.** This session changed no Linux behavior:

- No changes to any `src/**` compiler/runtime/emitter file except the pure-formatting
  `cargo fmt` commit (§20), which cannot alter behavior.
- No Linux tests added or modified; no Linux behavior was exercised as a development
  target. The one Linux test that ran (`linux_h01_http_execution_verified`) is a pre-existing
  WSL-frozen test and passed in isolation (§15).
- The new UTF-8 layer is platform-neutral stdlib code over `rt_str_*`/`rt_vec_*`, so it does
  not introduce a Windows-only abstraction and does not obstruct future Linux support.

## 20. Commit list

| # | Commit | Description |
|---|---|---|
| 1 | `5f2a935` | `fix:` UTF-8 code-point layer, leak + validator fixes, npm stdlib re-sync, 18 strict tests, drift guard, recorded compiler defect |
| 2 | `c3b8091` | `style:` `cargo fmt` on pre-existing formatting violations (formatting only) |
| 3 | documentation commit | `docs:` this report, the reconciliation of `WINDOWS_PYTHON_OFFICIAL_CAPABILITY_PARITY.md` and `WINDOWS_PYTHON_PARITY_IMPLEMENTATION_PLAN.md`, and the completion of the first-half `SESSION_107` report |

## 21. Push verification

Each unit was committed and pushed. Verified after the feature and formatting pushes:
`git status -sb` showed `## main...origin/main` with no ahead/behind marker and
`git rev-parse HEAD` == `git rev-parse origin/main` == `c3b8091`. The documentation commit
is the session's final commit; the push checkpoint after it reported no ahead/behind
marker with `git rev-parse HEAD` == `git rev-parse origin/main` (`AHEAD == 0`,
`BEHIND == 0`), and its hash is the session's ending commit as recorded in git history.

## 22. Pause-safety status

**PAUSE SAFETY: PASS.** All completed work (feature, formatting, documentation) is
committed and pushed; the working tree is clean apart from the explicitly documented
generated artifacts removed in this session, and the repository is recoverable with
`git pull --ff-only origin main`. No work remains only locally.

---

*End of Session 107 (continuation) report.*
