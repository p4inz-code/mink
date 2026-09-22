# Session 118 — Final independent Windows completion audit

**Starting commit:** `86642b4` (Session 117 close; `HEAD == origin/main`, clean tree)
**Ending commit:** the commit carrying this report
**Parity blockers closed:** none needed — P0 = 0 and P1 = 0 were already true; this session
verified them independently and repaired one stale documentation line, plus one harness
defect found while re-running the native suite.
**Linux:** frozen and untouched. No ELF/backend source was modified this session.

## 1. What this session was

The last Windows verification pass: independently re-derive the reported completion state
instead of trusting it, re-run the native Windows PE evidence for every capability that
had been a P1 blocker, re-run the full regression, audit the packaged distribution, and
then freeze Windows if the gate holds. No features were added and no architecture was
redesigned.

## 2. Matrix recomputation (mechanical, from the rows)

Every aggregate in the matrix was recomputed by scanning the 232 capability rows directly
(`| L.. / R.. / S.. / T.. / W.. / P..` table rows), not from the prose totals:

| Check | Result |
|---|---|
| Rows found | **232** (L 73 · R 29 · S 78 · T 16 · W 22 · P 14); no duplicate, missing or extra IDs |
| Column alignment | all 232 rows are 10-cell (the five `\|`-escaped rows parse correctly) |
| Status | VERIFIED 102 · MISSING 68 · PARTIAL 34 · INTENT. DIFF. 13 · N/A 6 · EXECUTION VERIFIED 2 · PLANNED 2 · N/A (INTENT.) 2 · INTENT. DIFF. + PARTIAL 1 · VERIFIED (INTENT. DIFF.) 1 · VERIFIED (partial) 1 — **sums to 232** |
| §10.1 per-domain breakdown | reproduces exactly (e.g. VERIFIED 30/15/35/8/5/9 = 102) |
| Priority | P0 0 · P1 0 · P2 88 (23/9/34/2/17/3) · P3 54 (23/8/14/5/3/1) · no-gap 90 — **sums to 232**, matches §10.2 |
| Difficulty | S-M 6 · S 37 · M 84 · L 30 · XL 14 = **171 work rows** + 61 no-work = 232, matches §10.3 |
| `Blocks = Y` | **0 rows**; `P1` set equals it exactly |
| Wave D/E/F blockers | 0 in every wave |

So the reported "232 rows, P0 = 0, P1 = 0" is confirmed, and §10.1–§10.3 are internally
consistent and arithmetically correct.

## 3. Stale claim found and repaired

**One live contradiction existed inside the matrix.** §3.10 "Language section totals" still
carried Session 107-era numbers and named L34 and L68 as active parity blockers:

- claimed `VERIFIED 29 … MISSING 19`, `P1 2 · P2 22 · no-gap 26`, and
  "**Parity blockers (Blocks = Y): 2 — L34 (exceptions), L68 (packages)**";
- the rows actually show `VERIFIED 30 … MISSING 18`, `P1 0 · P2 23 · no-gap 27`, and
  **0 `Blocks = Y`** — L34 is INTENT. DIFF./P2/N (reclassified Session 111) and L68 is
  VERIFIED/`Blocks = N` (delivered Session 109).

§3.10 was corrected from the rows and annotated with the reason. This is the only stale
parity claim in the live documents; the remaining "18 blockers"/"13 blockers" figures are
inside dated Session 98–109 notes that are explicitly labelled as historical baselines.
The plan's header already carries the "0 parity-blocking P1 gaps" current figure.

Also refreshed to measured values (§2, §11): the npm bundle ships **26** stdlib modules
(not 22/25 — `tls` was missing from the list), the suite is **72 test units** (71
integration targets + lib) running **2 948 passed / 3 ignored / 0 failed**, and the smoke/
CLI/release counts are 13/13 · 74/74 (+1 ignored) · 68/68.

## 4. Re-verification of every previously-P1 Windows capability

Each capability was re-run through the public MINK workflow (compile a real Windows PE with
`mink build`/`mink run` and execute it), never through Rust unit tests alone:

| Capability | Evidence this session |
|---|---|
| R19 threads | `threads_lib` 17/17, deterministic over 5 repeated runs |
| R20 async / event loop | `async_lib` 23/23, deterministic over 5 repeated runs |
| S71 threads + locks | `threads_lib` `s71_*`: mutual exclusion, 4×5000 locked increments = exactly 20000, unlocked control, 200 lock cycles |
| S73 async/await | `async_lib` `s73_*`: nesting, control flow, cross-module async, rejected forms |
| S62 TLS | `tls_lib` 19/19 (+1 ignored) and the opt-in public-endpoint test **passes** |
| P04 site-packages | `package_manager` 23/23 |
| P05 pip-equivalent install | `package_manager` 23/23 |
| P06 resolver | `package_manager` 23/23 |
| P09 virtual environments | `package_manager` 23/23 + packaged external workflow (§6) |

Boundary, invalid-input, failure-path, ownership/cleanup and leak behavior are covered by
the same suites: every native program's exit code 0 *is* the arena leak check, and the
negative cases (missing CA, malformed CA/URL, closed handles, refused connections, E-R06
leak, E-R05 double-collect, E-R13 non-task handle, package conflict/cycle/non-convergence)
are asserted individually.

## 5. Full native regression

All 72 test units were re-run green: **2 948 passed · 3 ignored · 0 failed**. Grouped
invocations (compiler core, language, stdlib, systems, tooling) plus repeated stress runs
of the concurrency, TLS and network suites. Cross-subsystem pairs exercised include async
+ locks, threads + allocator, threads + strings, locks + runtime state, packages + modules,
environments + resolution, TLS + HTTP, ZIP/zlib + filesystem, and SQLite + transactions +
error handling.

## 6. Clean external user workflows (packaged compiler)

Performed from `…/Temp/mink audit π` — a path containing **spaces and Unicode**, outside
the repository, using the tarball produced by `npm pack`:

1. `npm pack` → 28 files (compiler + 26 stdlib modules + manifest); `npm install` into the
   external project; `node_modules/.bin/mink --version` → `mink 1.0.1`.
2. Bundled stdlib resolution: `mod logging;` / `use logging::log_info;` compiled and ran
   against the packaged `stdlib/` with no configuration or copying.
3. `mink init` → `mink.toml`; `mink run` and `mink build` of a project source.
4. Dependency workflow: `mink add helper --path ../helper` → `mink.lock` with a content
   checksum, `.mink/packages/helper`, then `mink run` importing the installed package and
   printing `7`.
5. Environments: `mink env new/use/list/remove` lifecycle, and a run inside the isolated
   environment (plus the expected `E-PKG15` "not installed" report from `install --check`
   inside an environment that has no packages).
6. Async, threads, logging, regex, argv/env/stdin float formatting and threaded examples
   all ran; the shipped `regex_tool` and `env_report` examples built and ran as
   *standalone* executables copied to a directory containing nothing else.
7. HTTPS: a packaged-compiler program fetched `https://example.com/` through the Windows
   system trust store and printed a real `HTTP/1.1 200 OK` response (868 bytes).

## 7. TLS security audit

Beyond `tls_lib`'s deterministic local-server fixtures, the packaged compiler was pointed at
public endpoints to confirm that validation is never weakened in the real world:

| Endpoint | Result |
|---|---|
| `https://example.com/` (public CA, system store) | **ACCEPTED** — 200 OK |
| `https://expired.badssl.com/` | **REJECTED** |
| `https://wrong.host.badssl.com/` (hostname mismatch) | **REJECTED** |
| `https://self-signed.badssl.com/` (untrusted root) | **REJECTED** |
| `https://untrusted-root.badssl.com/` | **REJECTED** |

Wildcard single-label rules, IPv4-literal SAN matching, wrong EKU, closed/invalid handles,
partial reads, a 300 KB transfer and repeated no-leak connections are covered by
`tls_lib`, which re-ran green 3/3 times.

## 8. Concurrency audit

Bounded deterministic stress, no sleeps used as proof: `threads_lib` and `async_lib` 5/5
identical runs; `tls_lib` + `network_lib` + `http_lib` 3/3 identical runs. Lock-guarded
accumulators land on the exact closed-form totals (20000, 4000), unlocked controls are
lossy, and no deadlock, race, hang, use-after-free or shutdown corruption appeared under
repetition.

## 9. Infrastructure findings

### 9.1 `windows_hardening` port contention — root cause fixed

**Reproduction (before the fix):** `cargo test --test windows_hardening` with the default
parallel harness hung; after an external 150 s timeout the harness reported
`17 passed; 1 failed` and the failing test was always
`tcp_mink_server_echo_repeated_connections`. `cargo test --test windows_hardening --
--test-threads=1` passed 18/18 in ~13 s, which located the defect in the harness rather
than in the product.

**Root cause (two harness races, both fixed):**

1. `free_port()` bound `127.0.0.1:0`, read the port and dropped the listener, so the number
   was free again before the test used it. Under a parallel run two tests could be handed
   the same port; one test's server then answered the other test's client, and the client's
   blocking `read` waited forever — a cross-test socket pairing, not a product fault
   (blocking receives are the documented socket contract, the same as Python's `recv`).
2. Even with unique ports, the harness-side client raced the MINK server's `bind`/`listen`
   (the client thread was spawned before the server process started), which turned the
   mispairing into a connect panic once race 1 was removed.

**Fix (harness only, no product change):** `free_port()` now reserves each handed-out
number in a process-wide set (with a bounded scan fallback), so two parallel tests can
never share a port; the HTTP split-server's "connection refused" case now uses a second
reserved port instead of `port + 1`; every harness-side listener/socket is bound in the
test thread *before* the MINK program starts; the one client whose server is the MINK
process retries `connect` against a 20 s deadline instead of a single `unwrap`; and every
helper thread is joined through `join_bounded` (30 s) so a future mispairing fails the test
loudly instead of hanging the target.

**After the fix:** 6/6 consecutive parallel runs pass **18/18** in ~10–13 s (no
`--test-threads=1` workaround needed), and the serial mode still passes 18/18. `cargo fmt
--check` is clean and clippy is back at its documented baseline with **0 new warnings**.

### 9.2 `LNK1104` link failures on generated test executables

Observed once as a storm of `LNK1104: cannot open file …target\debug\deps\<target>.exe`
while several cargo invocations were overlapping and after an interrupted run had left
orphaned test servers holding artifacts. It did **not** reproduce afterwards:
`cargo build --tests` links every target cleanly, and each affected target passed on the
next run. Windows real-time protection is disabled on this machine
(`Get-MpComputerStatus` → `RealTimeProtectionEnabled: False`), so Defender is not the
trigger here; the condition is link-time contention on freshly written test binaries left
over from a killed run. It is infrastructure, not product behavior, and the mitigation is
to run one cargo invocation at a time (and clean up orphaned test servers after an
interrupted run).

## 10. Quality gates

- `cargo fmt --check` — clean.
- `cargo clippy --all-targets` — exit 0; 82 library warnings + 2 test warnings, all
  pre-existing (identical to the pre-audit baseline); **0 new**.
- Debug and release builds — clean.
- Package/npm checks — `npm pack --dry-run` 28 files; the bundled `stdlib/` is
  byte-identical to `stdlib/` (`diff -r`); the committed `npm/mink/bin/mink.exe` still
  corresponds to the current release source (this session changed only a test file, so the
  distributed binary is unaffected; local `cargo build --release` output is not
  bit-reproducible because MSVC embeds timestamps, which is why the on-disk artifact and the
  committed binary differ after a rebuild).

## 11. Freeze decision

Every Windows completion-gate condition holds: 0 P0, 0 P1, all 232 rows reconcile, every
claimed capability has execution evidence, native PE verification covers runtime/compiler/
platform behavior, permanent regression coverage exists, ownership/lifetime checks pass,
concurrency stress passes, clean external workflows pass, the packaged/npm workflow passes,
standalone execution passes, TLS security is verified against both fixtures and public
endpoints, documentation matches implementation, no stale parity claim remains, no Linux
capability work was introduced, `HEAD == origin/main`, and the working tree is clean.

**Windows is frozen at this checkpoint.** The remaining rows are P2/P3 non-blocking
roadmap items (for example HTTP conveniences, wide-API/Unicode console work, FFI,
profiler/debugger, and the intentional-difference rows), and Linux remains frozen.
