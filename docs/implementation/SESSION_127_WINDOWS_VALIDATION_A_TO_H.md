# Session 127 — Windows practical-parity validation: A3–H complete, 2 more stdlib defects fixed

STATUS: milestone complete. **A1–A3, B, C, D, E, F, G, H all pass their
Python cross-check (228/228 cases)**, plus the memory/scaling battery, the
capability matrix + gap list, and the 13-section final report.

Baseline: HEAD `dd556a692b8290416e14703471307cc311995d2f` (v1.0.4),
`Cargo.toml` `1.0.4` — **no version bump, Linux remains FROZEN, Windows
x86_64 only.**

Deliverables this session:

| Artifact | Content |
|---|---|
| `validation/a3_textgrep/record.md` | A3, 35/35 |
| `validation/b_csvdata/record.md` | B, 34/34 |
| `validation/c_json/record.md` | C, 43/43 |
| `validation/d_sqlite/record.md` | D, 10/10 |
| `validation/e_network/record.md` | E, 18/18 |
| `validation/f_process/record.md` | F, 19/19 |
| `validation/g_threads/record.md` | G, 18/18 |
| `validation/h_manifest/record.md` | H, 13/13 |
| `validation/memory_battery.py` | memory/scaling battery |
| `docs/validation/VALIDATION_MILESTONE_1_CAPABILITY_MATRIX.md` | 20 verified rows, 8 holes, 12 prioritized gaps |
| `docs/validation/VALIDATION_MILESTONE_1_REPORT.md` | 13-section final report |

## 1. Post-sweep re-verification (session 127 opening step)

The full `cargo test` sweep was green **before** this session's work: 3040
passed, 0 failed, 1 ignored (`tls_lib`, live-internet) across 73 integration
targets plus `--lib`. A3 and B were then rebuilt **from the repository root**
after the bool-chunk emitter fix and re-verified (`35/35`, `34/34`).

**Final sweep, after the two stdlib fixes: PASS — 3043 passed, 0 failed,
1 ignored** across all 73 targets + `--lib` (10 batches, `pipefail`). The
3040 → 3043 delta is exactly the three new regression tests. All ten
validation apps were re-run at the same time and are green: **228/228 cases**
(20+18+35+34+43+10+18+19+18+13).

Build reminder: `target/debug/mink.exe build validation/<proj>/main.mink`
**must run from the repo root** — building inside a project directory fails
with `E-S01: module file 'filesystem.mink' not found`.

## 2. Two more stdlib defects fixed (both with failing-first regression tests)

### 2.1 `http_client_get` could not work in a fresh program

`http_send` called `net_tcp_socket()` before any `WSAStartup`, and the
runtime's socket intrinsic deliberately returns `-1` when Winsock is not
initialized, so the first request always failed with an empty response.
Existing tests missed it because `h41` only asserts an *error* result and
`h40`'s in-program server (`net_listen_on`) initializes Winsock first.

- Fix: `stdlib/http.mink` → `http_send` now calls `net_init()` before
  creating the socket (mirrored in `npm/mink/stdlib/http.mink`), matching the
  module header's claim that "a user only needs `mod http;`".
- Regression: `tests/http_lib.rs`
  `h42_client_get_initializes_winsock_standalone` (Rust `TcpListener`, the
  program's only network call is `http_client_get`) — verified failing without
  the fix (`left: 0, right: 42`), passing with it.
- Re-run: `http_lib` 38, `httpserver_lib` 10, `network_lib` 36, `tls_lib` 19
  (+1 ignored) — all green.

### 2.2 The JSON object parser rejected whitespace after `,`

`_parse_object` consumed the member separator with a bare `p = p + 1` and
never called `_skip_ws`, so pretty-printed JSON (what `json.dump(indent=…)`
emits) failed to parse while its compact twin succeeded. Arrays and the `{`/
`:` positions already skipped whitespace; only this site was wrong.

- Fix: `stdlib/json.mink` → skip whitespace after the comma (mirrored in
  `npm/mink/stdlib/json.mink`).
- Regression: `tests/json.rs`
  `object_members_allow_whitespace_between_tokens` (6 pretty/CRLF/tab/nested
  forms) plus `object_members_still_reject_garbage_after_comma` (3 negatives
  proving the parser did not become permissive). `cargo test --test json` →
  **63 passed**.

## 3. Application defects found and fixed (all caught by the harness)

- **A1** — found by the last pass, not by the harness's own exit code:
  `grep_at_start` used `re_is_match` (search-anywhere) where the reference uses
  `re.match` (anchored), so mid-text-only patterns were reported as anchored
  matches; two anchored cases (`2026` → `yes`, `^2026` → `yes`) were added to
  pin the positive direction. Application defect; the MINK regex engine was
  correct throughout.
- **C** — `load_doc` pre-allocated then overwrote a string (leak);
  `find_key` never freed its consuming `Str` parameter (leak);
  `return fail_msg(...)` returned the stderr **byte count** as the exit code
  (observed 42/44 instead of 1/4).
- **D** — a nested `rt_str_concat("sql> ", rt_str_concat(line, ""))` leaked
  the inner temporary on every echoed line (`E-R06`).
- **E** — `run_http` never released the consumed `host` parameter.
- **F** — `rt_str_concat("arg[", rt_str_from_int(i))` leaked the index;
  `rt_env_*` copies were never freed (the intrinsics do **not** consume, unlike
  every stdlib wrapper).
- **G** — no defects: the first version passed 18/18.
- **H** — `rel_child` overwrote its first allocation in the non-root branch
  (one leak per file in a subdirectory); the CLI required two positionals
  while the tool has no command token.

## 4. Memory / scaling battery (`python validation/memory_battery.py`)

Failure modes are **diagnosed, not silent**:

- `E-R03` (allocation table exhausted, exit 103) — `MAX_LIVE_ALLOCS = 1024`
  (`src/runtime/abi.rs:107`). Bisected for A3: **800 input lines pass, 900
  fail**; `b_csvdata agg` fails at 2 000 rows; `d_sqlite` fails at 2 000
  script lines. Any app that materialises a whole document as a `Vec<Str>`
  hits this after ~800–1000 elements.
- `E-R05` (invalid memory access, exit 105) — the JSON serializer writes into
  arena slots 7168..8191 while the parsed image grows from `input_len + 1`, so
  the effective budget is `input + parsed ≤ 7168`: 1 661 B (50 objects) passes,
  5 131 B (150 objects) fails. The header's "~8000-byte input" is optimistic.
- Performance: `d_sqlite` with 500 inserts took **17.9 s** vs Python's 0.17 s
  — every `sql_exec` copies and re-parses the whole database text.
- Comfortable: 1 000 000-item thread pool in 94 ms, 400-file manifest in
  598 ms, both leak-free.

## 5. Unicode path evidence (P0 gap)

First run of H with Unicode names:

- `sub dir/naïve café.txt` was read correctly (CP1252 round-trips it) but the
  path was **reported** as `na?ve caf?.txt` in the CSV;
- `sub dir/nested/日本語.txt` was looked up as `???.txt`, so `fs_file_size`
  returned `-1` and the row read `..., -1, e3b0c44298fc1c14, ...` while
  Python reported `16, 24d22f3d5e722ce4`.

The parity fixture therefore uses ASCII path *names* with UTF-8 *content*, and
the behaviour is registered as gap P0-1 (switch the fs services to the `W`
entry points with UTF-8 input). `F`'s record documents the same ANSI issue for
argv (the reference mirrors it with `mbcs`).

## 6. Known limitations catalogued this session (for the gap list)

- ANSI filesystem: **no Unicode path names**, no long paths (`MAX_PATH` 260).
- `MAX_LIVE_ALLOCS = 1024` — the practical ceiling on simultaneously live
  strings.
- No console control handler (Ctrl+C / `SetConsoleCtrlHandler`) — a graceful
  shutdown proof app cannot be written.
- JSON effective budget `input + parsed ≤ 7168`; ~20 nesting levels.
- SQLite: in-memory blob only (no file persistence), no
  `ORDER BY`/`GROUP BY`/`JOIN`/`LIMIT`, O(n²) per-statement cost.
- Process: Windows-only, fixed capture window, stdin capped at 4096 bytes.
- HTTP: `Connection: close` only, no chunked/redirects, 64 headers, 4 MiB body.
- Threads: no condvars, no thread pool, no cancellation, no race detector.

## 7. Cleanup done

- `validation/scratch_probe/` (all probe programs), `MINK_DBG_CODE` dump,
  `tests/zz_probe_dump.rs`, `probe_*.mink`/`.exe` scratch — removed.
- `validation/battery_tmp/` and each `validation/*/work/` output tree —
  removed before delivery.
- `git worktree remove ../mink-head --force` (stale prunable worktree) — done.
- `docs/implementation/OWNERSHIP_IMPLEMENTATION.md` updated with §3.2
  (branch-local + conservative join, still not path-sensitive) and a §4 entry.

## 8. Handoff: first commands next session

```bash
cargo test --test <target>            # the final full sweep is recorded in
                                      # VALIDATION_MILESTONE_1_REPORT.md §12
python validation/<proj>/verify.py    # should stay N/N
python validation/memory_battery.py   # expect the same E-R03/E-R05 boundaries
```

Priority for the next session: **(1) Unicode filesystem, (2) `MAX_LIVE_ALLOCS`,
(3) long paths, (4) console control handler, (5) JSON arena sizing, (6) the
SQLite per-statement copy cost** — see
`docs/validation/VALIDATION_MILESTONE_1_CAPABILITY_MATRIX.md` §3.
