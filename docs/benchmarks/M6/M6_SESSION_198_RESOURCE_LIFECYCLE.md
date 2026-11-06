# Session 198 — Runtime Resource Lifecycle Audit

Windows x86_64, 2026-11-06. Starting HEAD
`797041beaaf9157290ac662d84229887835b403f` (Session 197, verified on origin
before any change).

## 1. Scope established before auditing

Only facilities that exist were audited. Inventory of every resource the
Windows runtime acquires, with the emitter that owns it
(`src/backend/emit/runtime.rs` unless noted):

| Resource | Acquired in | Released in |
| --- | --- | --- |
| Runtime heap block | `emit_alloc` (first-fit over `.bss` arena + `VirtualAlloc` growth chunks) | `emit_free` (LIFO free list, liveness table) |
| Liveness slot | `emit_alloc` | `emit_free` |
| Vec buffer | `VecNew`, `VecPush` growth, `CollCloneValue` | `VecFree`, `Free` on growth |
| Map buffer | `MapNew`, `MapInsert` rebuild, `CollCloneValue` | `MapFree` |
| Set buffer | `SetNew`, `SetInsert` rebuild, `CollCloneValue` | `SetFree` |
| Nested collection | element/owned-value stores | `CollFreeValue`, `CollCloneValue` |
| File handle | `FsExists`, `FsFileSize`, `FsRead`, `FsWrite` (`CreateFileA`) | same service, before return, on both success and error paths |
| Enumeration handle | `DirOpen` (`FindFirstFileA`) | `DirClose` (`FindClose`); `DirOpen` frees its state block on failure |
| Pipe handles | `ProcessRun` / `ProcessRunInput` (`CreatePipeA` ×2–3) | same service, after the drain loop |
| Process / thread handles | `CreateProcessA`, `CreateThread` | `ProcessRun` closes both; `ThreadJoin` / `TaskRun` close and free the block |
| Capture window | runtime-owned `.bss` region (`ensure_stdio_capture`) | never freed — a borrowed view, not an allocation |
| Lock word | `MutexNew` | `MutexFree`; `rt_lock_*` is a runtime-internal spinlock |

Every `CloseHandle` call site (15) and the single `FindClose` site were read
with its surrounding control flow, plus both `CreateProcessA` failure
directions and `CreateThread`'s failure path.

## 2. Method

Static review alone cannot show that a *repeated* operation leaves nothing
behind, and the MINK leak checker only sees the MINK heap — a leaked Win32
handle is invisible to it, and a program that leaks one still exits 0. So the
audit's instrument is a new test harness, `tests/resource_lifecycle.rs`, that:

1. compiles a MINK program that performs a bounded number of resource cycles,
2. samples the **child's OS handle count** every 10 ms while it runs
   (`OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION)` +
   `GetProcessHandleCount`, each sample's own handle closed immediately),
3. asserts the count never trends upward beyond a small fixed slack, and
4. asserts a clean close-out: exit 0 (so `E-R06`/106 and `E-R04`/104 did not
   fire), empty stderr, and the program's own internal failure counter is 0.

A **negative control** proves the instrument is not blind: a program that opens
400 directory enumerations and never closes them is detected — handle count
22 → 410, exit 106. Without that control the flatness assertions would prove
nothing.

## 3. Findings

### 3.1 No defect in the audited paths (measured)

| Workload | Iterations | Samples | Baseline → peak → final | Exit |
| --- | --- | --- | --- | --- |
| `fs_write` + `fs_read` + `fs_file_size` + `fs_exists` + open/enumerate/close, per iteration | 3,000 | 68 over 700 ms | 62 → 63 → 62 | 0 |
| Same calls on a **missing** path, plus `fs_remove_file`, `fs_dir_open`, `fs_dir_next(null)`, `fs_dir_close(null)` | 3,000 | 20 over 206 ms | 62 → 62 → 62 | 0 |
| `process_run` + capture length | 40 | 31 over 318 ms | 72 → 72 → 72 | 0 |
| thread spawn + join (block allocated and freed per cycle) | 3,000 | 13 over 134 ms | 63 → 63 → 63 | 0 |
| `rt_vec_new`/`push`/`len`/`free` + `rt_alloc`/`rt_free` | 200,000 | — | — | 0, exact total 400,000 |
| Negative control: 400 enumerations never closed | 400 | 4 | 22 → 410 | 106 |

Every failure path also returned its documented value on all 3,000 iterations
(`fs_exists` false, `fs_file_size` −1, `fs_read` empty, `fs_remove_file` 1,
`fs_dir_open` 0, `fs_dir_next` empty, `fs_dir_close` −1), so the error paths
are both leak-free and behaviourally stable.

### 3.2 Free paths reject non-heap pointers (verified)

`rt_str_free` on a runtime-owned capture view (`rt_process_stdout()`) is
`E-R04` / 104 — the release path validates the pointer against the live table
rather than corrupting `.bss`. Freeing an immortal literal is a documented
no-op. A plain double free is 104. This is now a permanent test (`rl07`).

### 3.3 Unverified risk — `rt_fs_read` ignores `ReadFile`'s result and byte count

**Evidence** (`emit_fs_read`, `src/backend/emit/runtime.rs:10125-10150`): the
service allocates a string of `GetFileSize` bytes, calls `ReadFile` with that
length, passes the bytes-read out-parameter to `BSS.bytes_written`, and then
returns the buffer **without reading either the BOOL in RAX or the byte
count**. The returned Str's length prefix remains the requested size.

**Impact if triggered.** If `ReadFile` fails (0 bytes moved), the caller
receives a full-length "file content" made entirely of whatever the reused
heap block previously held — an intra-process disclosure of freed object
contents, reported as data rather than as an error. If the file shrinks between
`GetFileSize` and `ReadFile`, the tail is stale block memory. A successful
full read is unaffected, which is why the entire existing suite passes.

**Why it is not fixed here.** No deterministic trigger exists in MINK's
current API: every failure that makes `ReadFile` fail also makes the preceding
`CreateFileA` fail, and every short read needs a file to shrink mid-call (a
concurrent writer, i.e. a timing race, not a stable regression). Writing the
fix without a failing test would be an unverified change, and the project's
rule is one regression test per confirmed defect. The risk is recorded instead
of silently patched, and the recommended follow-up is: capture RAX and the
byte count, truncate the Str length to the bytes actually read, and return an
empty Str on failure — then add a test once a random-access or
concurrently-truncated file path exists.

**Secondary note.** That byte-count scratch is the shared `.bss` word
`BSS.bytes_written`, also used by the print path. It is harmless today because
`emit_fs_read` never reads it, but it would need to become a stack slot if the
fix above is made, or two concurrent threads could cross their counts.

## 4. Review of the Session 197 change under this lens

Rechecked rather than assumed: freeing a block writes its generation to
`block + 16` only when the block is ≥ 24 bytes, before the block is linked
into the free list; the free list itself only reads offsets 0 and 8, and the
liveness table's live flag is the low byte at `+16`. A freed collection buffer
is ≥ 24 bytes by construction (its header starts at 24), so the lineage word
can never overwrite payload that anything later reads, and a block smaller
than 24 bytes keeps no generation — which is correct, since it cannot hold a
collection. Nothing in the free path can leave the block half-linked: the
sequence is lineage → size → dead flag → head link. Confirmed by the Session
197 suites and by this session's 200,000-cycle allocation loop exiting with an
exact total and no leak report.

## 5. Tests added

`tests/resource_lifecycle.rs` (7 tests, Windows-only, artifacts under
`target/mink-artifacts/s198/`):

| Test | Asserts |
| --- | --- |
| `rl01_filesystem_cycles_do_not_leak_os_handles` | 3,000 mixed fs cycles: values correct, flat handle count, exit 0 |
| `rl02_failed_filesystem_operations_do_not_leak_os_handles` | 3,000 failing fs calls: documented returns, flat handle count, exit 0 |
| `rl03_process_cycles_do_not_leak_os_handles` | 40 spawn/capture cycles: flat handle count, exit 0 |
| `rl04_thread_cycles_do_not_leak_os_handles` | 3,000 spawn/join cycles: flat handle count, exit 0 |
| `rl05_allocation_cycles_leave_no_live_allocations` | 200,000 collection + raw allocation cycles: exit 0, exact total |
| `rl06_detector_sees_a_deliberate_handle_leak` | negative control: 400 unclosed enumerations are detected (peak ≥ baseline + 40), exit 106 |
| `rl07_freeing_a_runtime_owned_capture_view_is_rejected` | freeing a borrowed capture view is `E-R04` / 104 |

## 6. Verification

| Check | Result |
| --- | --- |
| `cargo test --test resource_lifecycle` (debug) | **7 passed, 0 failed** (29.8 s) |
| `cargo test --release --test resource_lifecycle` | **7 passed, 0 failed** (29.3 s) |
| `rustfmt --check tests/resource_lifecycle.rs` | clean |
| Production source changes | none — this session changed only the new test file and documentation |
| Full regression | not run for this session: no runtime, compiler or stdlib source changed, so the Session 197 checkpoint (75 suites, 3,185 passed, 0 failed) still describes the code. Session 199 runs the full sweep with the soak evidence. |
| `tls_lib` | still not runnable here (`python3` is the Microsoft Store alias stub); unchanged environmental limit |

Measurement caveat: the samples are observations of a live process, not
instrumentation inside it. The harness bounds each run with a timeout and fails
if fewer than three samples are taken, so a run that is too short to measure
fails loudly instead of passing vacuously.

## 7. Documentation

- This record.
- `docs/roadmap/IMPLEMENTATION_ROADMAP.md`: dated Session 198 status.
- `docs/audits/SYSTEMS_READINESS_AUDIT.md`: dated Session 198 addendum with the
  measured lifecycle evidence and the `rt_fs_read` unverified risk.

## 8. Verdict

Resource lifecycle audit performed across allocation, collections, filesystem,
processes, threads and concurrency. No confirmed defect in the audited paths;
one code-level risk (`rt_fs_read`) is documented with evidence, impact and a
concrete follow-up rather than silently patched. Reusable, measured
reliability coverage added. M6 external validation remains outstanding, M7
remains gated, Linux remains frozen.
