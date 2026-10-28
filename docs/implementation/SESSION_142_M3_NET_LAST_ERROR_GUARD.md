# Session 142 — M3: `net_last_error()` before `net_init()`

**Scope:** one small M3 defect only — `net_last_error()` called before `net_init()` dereferenced
the null Winsock function table and crashed the process. Found and documented in Session 141.

Windows x86_64 only. Linux untouched. Version stayed **1.0.4**. No push, tag, publish, merge,
rebase, reset, clean, stash or amend. Every pre-existing dirty file was preserved.

**Outcome:** the getter now returns **0** before initialization (and after `net_cleanup`) instead
of calling through a null pointer; the initialized path is unchanged. Two permanent regressions
were added; the pre-fix control fails with `STATUS_ACCESS_VIOLATION`, the fixed build passes.

## 1. Baseline

| Item | Value |
| --- | --- |
| Branch | `main` |
| HEAD | `1d6fa24896c106c2ef7213d841e66eb6e3938b76` |
| `origin/main` | `b860837a8a6039f304a503e3d85728a643ab470b` |
| Divergence | ahead 4, behind 1 |
| Staged | none |
| Modified tracked | 34 (27 pre-existing + the 7 Session 141 files) |
| Untracked | 20 (19 pre-existing + the Session 141 record) |
| `git diff --check` | clean |
| Version / warnings | **1.0.4** / 59 warnings |

The working tree also carries the Session 141 M3 P2 changes. They were inspected, not modified.

## 2. Reproduction

```
fn main() -> Int { let e = net_last_error(); rt_print_int(e); return 0; }
```

Built with the stdlib and run without calling `net_init()`:

```
Segmentation fault
exit 139
```

Deterministic (reproduced in Session 141's audit and again here). At the Windows level the exit is
`0xC0000005` (`STATUS_ACCESS_VIOLATION`, reported by Rust as `-1073741819`).

## 3. Root cause

`src/backend/emit/runtime.rs`, `emit_net_wsa_last_error`:

```
code.mov_r_rip(Reg::Rax, PatchKind::Bss(NET_FUNC_TABLE + 16)); // WSAGetLastError
code.call_rax();
```

`NET_FUNC_TABLE` lives in `.bss`, zero-initialized. Its `WSAGetLastError` slot is resolved by
`emit_net_wsa_startup` (`GetProcAddress`) and is null until `net_init()` runs successfully.
The getter called the slot unconditionally, so the first call before initialization jumped to
address `0`. The owning layer is the runtime emitter: the standard-library wrapper is a direct
forwarder, and every other `net_*` entry point that needs the table already checks
`NET_INIT_FLAG` (`net_socket`, `net_connect`, `net_bind`, `net_send`, …).

## 4. Chosen behavior before `net_init()`

The API did not previously define the answer; the existing conventions resolve it:

- `WSAGetLastError()` itself reports **0** when no error has been recorded — and before any
  Winsock call, none can have been.
- The Linux implementation of `rt_net_wsa_last_error` is a fixed **0**.
- The runtime's existing "not initialized" answers are `0` for `rt_net_wsa_cleanup` and `-1` for
  operations that need a socket (`rt_net_socket`); a getter has no operation to fail.

**Defined behavior: return 0.** It is asserted by the regression and documented in
`stdlib/network.mink`. The same guard also makes the call after `net_cleanup()` (which clears the
flag) return 0 rather than reading a stale table entry.

## 5. Implementation

`src/backend/emit/runtime.rs` — `emit_net_wsa_last_error` checks `NET_INIT_FLAG` and skips the
`WSAGetLastError` call when it is clear, returning 0:

```
let skip = code.label();
code.mov_r_rip(Reg::Rax, PatchKind::Bss(NET_INIT_FLAG));
code.test_rr(Reg::Rax, Reg::Rax);
code.jcc_label(0x84, skip); // jz: not initialized
code.mov_r_rip(Reg::Rax, PatchKind::Bss(NET_FUNC_TABLE + 16));
code.sub_rsp(32); code.call_rax(); code.add_rsp(32);
code.bind_label(skip);
code.leave_ret();
```

No new intrinsic, no initialization redesign, no change to any other networking entry point.
`stdlib/network.mink` and its npm mirror document the pre-init result.

## 6. Regressions

`tests/network_lib.rs` §13:

| Test | Asserts |
| --- | --- |
| `n107_net_last_error_before_net_init_is_zero_and_does_not_crash` | pre-init → `0`, `net_init()` → `0`, after `net_cleanup()` → `0`, process survives |
| `n108_net_last_error_still_reports_failures_after_init` | initialized refused connect still leaves `10061` for the getter |

Control run against the pre-fix emitter: **n107 FAILED** with the child process exit
`-1073741819` (`0xC0000005`); n108 passed (it only exercises the initialized path). With the guard
both pass and `cargo test --test network_lib` is **43 passed, 0 failed** (41 + 2).

## 7. Verification

| Check | Result | Classification |
| --- | --- | --- |
| `cargo build` | success, 59 warnings — unchanged | PASS |
| `cargo test --test network_lib` | **43 passed, 0 failed** | PASS |
| Pre-fix control (guard reverted, then restored) | n107 fails with `0xC0000005`; n108 passes | PASS (evidence) |
| Broader networking — `http_lib` 46, `httpserver_lib` 10 | 0 failed | PASS |
| Full suite `cargo test --no-fail-fast`, shim on `PATH` | **3,103 passed, 0 failed, 1 ignored**, exit **0** | PASS |
| `cargo build --release` | success, 59 warnings | PASS |
| Release compiler (`mink 1.0.4`) on the pre-init reproducer | prints `0`, exit 0 (was 139) | PASS |
| `git diff --check` | clean | PASS |

The full-suite total is Session 141's **3,101** plus these two tests → **3,103**.

## 8. Audit

| Area | Result |
| --- | --- |
| Runtime — null state | The only table access in the getter is behind `NET_INIT_FLAG`; before init and after cleanup the call is skipped. |
| Runtime — initialized path | Unchanged: the same slot, same call, same return register. n108 and n80 pin it; the whole networking suite passes. |
| Runtime — state/leaks | The guard reads the existing flag and writes nothing; no allocation, no ownership change. |
| Runtime — other entries | Other table-using intrinsics (for example `rt_net_htons`) were **not** changed; they are outside this defect's scope and remain as they were. |
| Tests | Two permanent regressions; the pre-fix failure is the real crash, not a missing assertion; no existing test changed, skipped or weakened. |
| Scope | One getter guard, its two tests, the stdlib comment and this record. No other M3 item touched. |

## 9. Files changed

- `src/backend/emit/runtime.rs` — `emit_net_wsa_last_error` init guard
- `stdlib/network.mink` — `net_last_error` contract comment (0 before `net_init`)
- `npm/mink/stdlib/network.mink` — byte-identical mirror
- `tests/network_lib.rs` — `n107`, `n108`
- `docs/implementation/SESSION_142_M3_NET_LAST_ERROR_GUARD.md` — this record

## 10. Remaining M3 work

1. `rt_print_int` / float shared-buffer issue.
2. Thread-local HTTP last-error.
3. Socket connect/read timeouts.
4. UDP buffer parity.
5. HTTP chunked decoding.
6. `parse_block_expr` trailing-token recovery (Session 140 §9.3).

## 11. Git

- Fix, tests and this record are in the working tree.
- No source commit is included in the Session 141 work; this session's status is stated in §12.
- No push, tag, publish, merge, rebase, reset, clean or amend was performed.

## 12. Final state

- HEAD `1d6fa24896c106c2ef7213d841e66eb6e3938b76`; `origin/main`
  `b860837a8a6039f304a503e3d85728a643ab470b`; still ahead 4, behind 1.
- Working tree: 0 staged; 34 modified tracked files; 20 pre-existing untracked entries plus this
  record. `git diff --check` clean.
- Version **1.0.4**.
- Commit status: see §13.

## 13. Commit

The fix is runtime-only and independent of Sessions 128–141, so it was isolated into one commit
that contains **only** this session's work: the emitter guard, the two tests and this record (plus
the one-line standard-library contract comment in both mirrors). The commit was built from the
HEAD blobs plus those hunks, so no Sessions 128–141 content entered it; the working tree still
carries all of their uncommitted changes. The exact hash and the final `git status` are in the
session handoff.
