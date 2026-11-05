# Session 144 — M3 P4: thread-local HTTP last-error state

**Scope:** the Windows x86_64 HTTP last-error defect only — `http_last_error()`
was a module-level `let mut` word, so every thread read and wrote the same
slot. The fix introduces one per-thread runtime scratch cell (Windows TLS,
allocated once in `rt_init`) and moves the HTTP error state onto it.

Windows x86_64 is the implemented target. Linux keeps the previous
process-global behavior because its runtime is frozen and has no thread-local
mechanism; this is stated as a limitation, not presented as fixed. Version
stayed **1.0.4**. No push, tag, publish, merge, rebase, reset, clean, stash or
amend. Every pre-existing dirty file was preserved.

**Outcome:** an HTTP operation on one thread can no longer overwrite the
`http_last_error()` value observed by another thread. Four permanent
regressions were added. The deterministic concurrency test failed against the
old implementation (exit code 2: the successful thread read the failing
thread's `-2`) and passes after the fix. Full suite: **3,111 passed, 0 failed,
1 ignored**, exit 0.

## 1. Baseline

| Item | Value |
| --- | --- |
| Branch | `main` |
| HEAD | `6e88a7bcd62b0e853967d3625e125657f2ec4f2e` |
| `origin/main` | `b860837a8a6039f304a503e3d85728a643ab470b` |
| Divergence | ahead 6, behind 1 |
| Staged | none |
| Modified tracked | 36 |
| Untracked | 20 |
| `git diff --check` | clean |
| Version / warnings | **1.0.4** / 59 before this session |
| Prior full suite | **3,107 passed, 0 failed, 1 ignored** (Session 143, shim on `PATH`) |

The working tree carried the uncommitted Sessions 128–143 work. `git status`,
`git diff --stat`, `git diff --check`, `git branch -vv` and the log graph were
inspected before any edit. The pre-existing diffs of every target file were
read; no unrelated hunk was modified.

## 2. Reproduction

### 2.1 Deterministic cross-thread contamination

`tests/http_lib.rs::h51_client_last_error_is_thread_local` runs two workers
against loopback fixtures:

- worker A: `http_client_get` on a released port → connect refused, error `-2`;
- worker B: `http_client_get` on a live one-shot server → completed, error `0`.

Both workers finish their request, rendezvous on a two-arrival mutex-guarded
barrier, and only then read `http_last_error()`. With a process-global word
both reads see whichever request finished last, so at least one worker must
observe the other thread's code — no timing luck is involved.

```
pre-fix:  exit 2   (the successful worker observed -2, the failing worker's code)
post-fix: exit 51  (A observed -2, B observed 0: each read its own)

The pre-fix run above was captured before the final test's fresh-thread
assertion was added; that assertion checks `http_last_error()` is 0 before the
worker's first request, which the old global also satisfied in this run. The
cross-thread overwrite it reports is the defect.
```

The pre-fix failure is the defect itself, not an assertion artifact. It was
observed on the unmodified emitter before any runtime change.

### 2.2 Repetition sample

`h52_client_last_error_stays_thread_local_under_repetition` repeats the same
two operations six times per thread and counts each read that does not match
the code its own request produces. This is a bounded sample, not the proof:
in the pre-fix run it happened to observe zero mismatches (the read follows
its own request too closely), while h51 proves the overwrite deterministically.
Post-fix it prints `0` and exits `52`.

### 2.3 Single-thread sequences

The old global is correct when a thread is alone, so these are guards for the
contract rather than failure reproductions. `h53` runs, on one thread:

```
connect refused           -> -2
connect refused again     -> -2
peer closes, no response  -> -5
completed request         ->  0
connect refused again     -> -2
```

All five must hold before the test exits `53`; they do.

## 3. Root cause and data flow

```
http_client_get / http_send            stdlib/http.mink
  -> http_recv_all sets -4             stdlib/http.mink
  -> http_last_error() reads the binding
  -> module binding `let mut http_last_error_code`
  -> backend LoadStatic / StoreStatic  src/backend/...
  -> one `.data` word in the image     (not per thread)
```

The binding was emitted once per image. Its address is fixed, so every
thread's `http_send` did:

1. store `0` at function entry,
2. store `-2` / `-1` / `-3` / `-5` on the failure path (`-4` from
   `http_recv_all`),
3. read the same word in `http_last_error()`.

Winsock's own error state is per-thread (`WSAGetLastError` reads the calling
thread's slot), which is why `net_last_error()` did not have this problem.
MINK had no equivalent per-thread storage anywhere in the runtime, so the
HTTP layer could not be correct with the storage it had.

## 4. Error-state contract

The fix preserves every existing code and meaning and changes only ownership
of the state:

- `http_last_error()` reports the most recent HTTP operation performed **by
  the calling thread**; it is per-thread, not per-request and not global.
- A thread that has not performed an HTTP operation reads `0`.
- `http_send` clears to `0` at entry. A completed request therefore leaves
  `0`; a failure leaves one of the unchanged codes:
  `-1` socket creation, `-2` DNS/connect, `-3` send, `-4` response exceeds
  the receive budget, `-5` receive failure / no response / incomplete body.
- A request (successful or failed) on another thread cannot change the value
  this thread reads.
- No initialization or ordering is required by the caller: the slot is
  allocated by `rt_init` before `main`, and a fresh thread starts at `0`.
  If the slot could not be allocated, the getter answers `0` and the setter
  is a no-op — defined safe behavior, never an invalid-index call.
- There is no HTTP cleanup API. The TLS index lives for the process; it is
  not heap-managed and is not freed.

Sequential semantics are unchanged: the last operation on the same thread
wins, as before.

## 5. Implementation

### 5.1 Runtime primitives

| Layer | Change |
| --- | --- |
| `src/runtime/intrinsics.rs` | `rt_thread_local_get() -> Int`, `rt_thread_local_set(value: Int)` |
| `src/backend/ir.rs` | `RuntimeService::ThreadLocalGet` / `ThreadLocalSet`, arity 0/1, callable |
| `src/backend/lower.rs` | name → service mappings |
| `src/runtime/abi.rs` | `tls_slot` at BSS offset 200 (8 bytes), `.bss` size 200 → 208 |
| `src/backend/emit/pe.rs` | `TlsAlloc`, `TlsGetValue`, `TlsSetValue` appended to kernel32 (IAT 41/42/43); `.idata` 1536 → 1792 with a debug-build overflow assert |
| `src/backend/emit/runtime.rs` | allocate the index in `rt_init`; the two services |
| `src/backend/emit/linux_runtime.rs` | the two services over the one BSS word (frozen Linux behavior) |
| `stdlib/http.mink` + npm mirror | getter/setter and every error write use the runtime cell |

### 5.2 Windows slot lifecycle

`tls_slot` stores `TlsAlloc()`'s index **+ 1**; `0` means "not allocated"
(TLS index `0` is valid, so the tag is required). `rt_init`:

1. reads the cell; if non-zero, the slot is already allocated (a second
   `Init` cannot leak an index);
2. calls `TlsAlloc` with 32 bytes of shadow space;
3. zero-extends `EAX` (`mov eax, eax`) because the Win64 ABI leaves upper
   `RAX` unspecified;
4. rejects any value above `0x7FFF_FFFF` — the documented
   `TLS_OUT_OF_INDEXES` (`0xFFFFFFFF`) included — leaving the cell `0`;
5. otherwise stores `index + 1`.

`rt_thread_local_get` and `rt_thread_local_set` load the cell, return `0` /
do nothing when it is `0`, subtract the tag, and call
`TlsGetValue(index)` / `TlsSetValue(index, value)`. Every thread, including
the entry thread and threads created by `rt_thread_spawn`, receives
zero-initialized TLS storage from Windows. The slot is written once before
`main` and only read afterwards, so no lock is needed.

No allocation, no heap-liveness entry and no cleanup are involved. The
stdlib's module binding is gone, so there is no static error word left for a
thread to overwrite.

## 6. Regressions

| File | Test | Kind |
| --- | --- | --- |
| `tests/http_lib.rs` | `h51_client_last_error_is_thread_local` | deterministic barrier proof: fresh thread `0`, A `-2`, B `0` |
| `tests/http_lib.rs` | `h52_client_last_error_stays_thread_local_under_repetition` | 2 × 6 alternating failure/success, counts mismatches |
| `tests/http_lib.rs` | `h53_client_last_error_tracks_the_last_operation` | single-thread: repeat, two failure classes, success clears, failure again |
| `tests/threads_lib.rs` | `s144_thread_local_word_is_isolated_per_thread` | direct primitive: fresh `0`, worker `77`, main `5` survives |

Pre-fix evidence (actual runs):

```
h51 pre-fix:   FAILED, exit 2 (successful worker read -2)
h52 pre-fix:   passed in its sample (0 mismatches) — h51 is the deterministic proof
h41 pre-fix:   passed (single-thread failure semantics already correct)
```

Post-fix:

```
h51 = 51, h52 = 52, h53 = 53, s144 = 0,77,5
tests/http_lib:   49 passed, 0 failed
tests/threads_lib: 22 passed, 0 failed
```

No existing test was changed, skipped, weakened or deleted. h44's `-5`
expectation and h45–h50's completion semantics are untouched.

## 7. Verification

| Check | Result | Classification |
| --- | --- | --- |
| `cargo build` | success, 59 warnings (unchanged) | PASS |
| `cargo test --test http_lib` | **49 passed, 0 failed** | PASS |
| `cargo test --test threads_lib` | **22 passed, 0 failed** | PASS |
| Pre-fix h51 control | exit 2 before the fix, 51 after | PASS (evidence) |
| Full suite `cargo test --no-fail-fast`, shim on `PATH` | **3,111 passed, 0 failed, 1 ignored**, exit 0 | PASS |
| `cargo build --release` | success, 59 warnings (unchanged) | PASS |
| Release-compiled h51 | **1 passed, 0 failed** (observed exit 51) | PASS |
| `git diff --check` | clean | PASS |

The suite total is Session 143's **3,107** plus these four tests. The one
ignored test is the pre-existing one.

The shim is the Session 142/143 convention: `python3` resolves to the Windows
Store stub on this machine and the TLS fixture cannot start without the real
interpreter. `tmp/m3_print/bin/python3.exe` is first on `PATH` for the full
run. The TLS fixture result is classified as environmental, as before.

## 8. Audit

| Area | Result |
| --- | --- |
| Thread safety | The HTTP error is no longer process-global on Windows; reads and writes use Windows TLS. No lock, no shared mutable error cell, no cross-thread overwrite. |
| Initialization | `TlsAlloc` runs once in `rt_init` before `main`; index+1 tagging distinguishes "unallocated" from valid index 0; a second `Init` is harmless; failure leaves defined 0/no-op behavior. |
| Getter before init | Returns 0 without calling TLS; tested implicitly by the primitive guard (`s144`) and by every program's entry path. |
| Fresh-thread state | Zero-initialized by Windows; `h51` asserts the fail worker reads 0 before its first request; `s144` asserts a spawned thread reads 0 while main holds 5. |
| Runtime resources | No allocation, no liveness entry, no pointer lifetime; one TLS index per process, not freed (documented). |
| Semantics | All five HTTP codes and the success/clear behavior are unchanged; only the storage's ownership moved from process to thread. |
| ABI / architecture | New services are appended, arity 0/1, declared in the intrinsic table and lowered through IR; `.bss` grew 8 bytes at the tail, every earlier offset unchanged; `.idata` grew with three appended imports. |
| Imports | Names appended after the existing 41, so earlier IAT indices do not move; `.idata` has 231 bytes of headroom and a build-time assert now guards overflow. |
| Tests | Four permanent regressions; the deterministic pre-fix failure is captured; no existing test weakened. |
| Scope | HTTP last-error plus the one runtime primitive it requires. No timeouts, UDP, chunked decoding, server work, print work or parser work. |

## 9. Known limitations

1. **Linux is not fixed.** Its frozen runtime has no thread-local slot, so
   `rt_thread_local_get/set` are one process-global word there. The HTTP
   defect remains on Linux by design; Linux is outside the active platform.
2. The TLS index is never freed (`TlsFree` is not called). It is one
   process-lifetime index, not a per-request resource.
3. The contract is per-thread "last operation", not a per-request error
   object: a later unrelated HTTP call on the same thread replaces the value,
   exactly as before.
4. Concurrency guarantee is state isolation; no ordering between threads is
   promised or tested.
5. The full suite requires the Python shim on this machine.

## 10. Files changed

- `src/runtime/intrinsics.rs` — two per-thread intrinsics
- `src/backend/ir.rs` — two services, arity and callability
- `src/backend/lower.rs` — intrinsic mappings
- `src/runtime/abi.rs` — `tls_slot` at the `.bss` tail
- `src/backend/emit/pe.rs` — three TLS imports, larger `.idata`, overflow assert
- `src/backend/emit/runtime.rs` — `rt_init` allocation and the two services
- `src/backend/emit/linux_runtime.rs` — documented process-global fallback
- `stdlib/http.mink`, `npm/mink/stdlib/http.mink` — last-error on the cell
- `tests/http_lib.rs` — h51, h52, h53
- `tests/threads_lib.rs` — s144
- `docs/implementation/RUNTIME_IMPLEMENTATION.md` — per-thread contract
- this record

## 11. Git

**No commit was created.** The work is verified but cannot be isolated into
one focused commit without either absorbing or partially reconstructing
Sessions 128–143 work:

- `stdlib/http.mink` and its npm mirror carry 261 lines of uncommitted
  Session 129–138 work. Four of the error-state writes changed here live
  inside that work: `HTTP_ERR_TOO_LARGE` (Session 132 receive budget) and the
  `HTTP_ERR_RECV` / incomplete-transfer paths (Sessions 135/137). HEAD's copy
  has only the earlier paths, so a HEAD-based commit would have to include
  those unrelated hunks or commit a half-converted library.
- `tests/http_lib.rs` carries 395 lines of uncommitted tests; `h53` asserts
  the Session 135 `-5` behavior, which does not exist in HEAD.
- The runtime/compiler files (`intrinsics.rs`, `ir.rs`, `lower.rs`,
  `runtime.rs`, `linux_runtime.rs`) carry uncommitted Session 134/141 work.
  The Session 143 reconstruction method is possible in principle, but the
  resulting commit's HTTP library and tests would not be the verified state,
  and the session's primary fix would be absent from it.

The verified work therefore remains in the working tree. No reset, rebase,
stash, checkout, clean, amend, merge, push, tag or publish was performed.

Final state:

- HEAD: `6e88a7bcd62b0e853967d3625e125657f2ec4f2e` (unchanged);
  `origin/main` `b860837a8a6039f304a503e3d85728a643ab470b`; ahead 6, behind 1.
- Staged: none. Modified tracked: **39** (36 baseline + `pe.rs`, `abi.rs`,
  `RUNTIME_IMPLEMENTATION.md`). Untracked: **21** (20 baseline + this record).
- `git diff --check`: clean. Version **1.0.4**.

## 12. Remaining M3 work

1. Socket connect/read timeouts.
2. UDP buffer parity.
3. HTTP chunked decoding.
4. `parse_block_expr` trailing-token recovery (Session 140 §9.3).
5. `rt_print_str` single-write line atomicity (print-adjacent, not this
   defect).
