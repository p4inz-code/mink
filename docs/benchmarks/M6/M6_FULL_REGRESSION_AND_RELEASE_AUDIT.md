# M6 — Full Regression and Release Audit

Operation `M6-RC1`, Windows x86_64, 2026-11-05.

This record covers the whole-tree verification run behind the 1.0.5 candidate and the
release-readiness audit of the repository state that was pushed.

## 1. Test suite

Command (Git Bash):

```bash
export PATH="/tmp/pyshim:$PATH"   # see §4
cargo test > /tmp/full_test.log 2>&1
```

| Measurement | Baseline (start of operation) | Final (this run) |
| --- | --- | --- |
| Passed | 3,174 | **3,176** |
| Failed | 0 | **0** |
| Ignored | 1 | **1** |
| Suites | 78 | **78** |
| Process exit | 0 | **0** |

The two additional passes are the permanent regressions added during this operation:
`tests/environment_lib.rs::environment_wrappers_release_heap_string_arguments` and
`tests/release.rs::m6_rc_package_manifest_is_complete`. No test was removed, skipped,
weakened or marked ignored to obtain this result.

`grep -c "^test result: FAILED"` over the captured log returns `0`.

The single ignored test is the pre-existing case that requires public internet access.

## 2. What the suite covers

78 test targets, 75 test source files, including: the language surface (parser, type
checker, borrow checker, MIR, backend verifier), the runtime (memory, strings, filesystem,
process, threads, network, TLS, SQLite, zip/zlib, HTTP server), the standard library
modules, the release/packaging guards in `tests/release.rs`, and the Windows-specific
workload suites recovered from Sessions 128–149.

Build check: `cargo build --release` exit 0, 59 pre-existing warnings, 0 errors.

## 3. Formatting and whitespace

| Check | Result |
| --- | --- |
| `git diff --check` | clean (exit 0) |
| `cargo fmt --check` | exit 1 — 49 diffs, all **pre-existing** |

The rustfmt differences are confined to committed files that this operation did not
modify (`tests/http_lib.rs`, `tests/httpserver_lib.rs`, `tests/closures.rs`,
`src/backend/emit/linux_runtime.rs` and others). They existed at `HEAD` before the
recovery, and the repository has never gated on `cargo fmt` — there are no CI workflows
(`.github/` contains only `FUNDING.md`). The files changed by this operation
(`tests/environment_lib.rs`, `tests/release.rs`) are rustfmt-clean. Reformatting
untouched files would have produced a large diff unrelated to the request, so it was not
done; this is reported as a known pre-existing condition, not as a passing check.

## 4. Environment caveats (property of the machine, not the compiler)

- `python3` on `PATH` is the Microsoft Store stub. The TLS/HTTPS tests start a Python test
  server from `PATH`; with the stub, 18 of them fail with
  "bad port line from TLS test server". Running the suite with a real Python 3.11 on
  `PATH` makes all of them pass. The same 19 tests therefore behave identically under the
  published 1.0.4 binary — this is not a regression.
- Consequently the suite is green on this machine only with that `PATH` adjustment; the
  command above records it explicitly rather than hiding it.

## 5. Frozen Linux support

No Linux source file was modified by this operation (`git status --porcelain | grep -i
linux` is empty). The recovered Sessions 128–149 work is Windows-side runtime, standard
library and test material; the Linux runtime backend
(`src/backend/emit/linux_runtime.rs`) is unchanged. No semantics were altered to make
tests or packaging pass.

## 6. Security and hygiene audit of the change set

| Check | Result |
| --- | --- |
| Credentials/secrets in changed or added files | none (only prose mentions of "token" in release instructions) |
| Absolute local paths in records | one found in `M6_REPOSITORY_RECOVERY_AUDIT.md` §1, replaced with a redacted value |
| Generated binaries staged unintentionally | none — only `npm/mink/bin/mink.exe`, which is the package payload |
| Destructive git commands used | none (no `reset --hard`, `clean`, `checkout --`, no force-push) |
| Broad staging | none — every commit staged named paths only |
| Tags created/moved/deleted | none |
| Publish/deploy performed | none |

## 7. Release-readiness summary

| Claim | Evidence | Status |
| --- | --- | --- |
| Version is 1.0.5 in all three manifests | `Cargo.toml`, `Cargo.lock`, `npm/mink/package.json` | verified |
| Compiler binary matches source | release build reproduces `c288115d…479401`, reports `mink 1.0.5` | verified |
| Tarball is complete and correct | 35 files, stdlib 27/27 byte-identical, binary hash equal to release build | verified |
| Tarball installs and works | clean installs in space and non-ASCII paths; cases A–F | verified |
| Application-level behaviour | `invsync` 12/12 via the independent verifier | verified |
| Diagnostics | `E-T02` with types and location | verified |
| `mod environment;` ownership | leak fixed, regression proven to fail without the fix | verified |
| Full suite | 3,176 passed / 0 failed / 1 ignored / 78 suites | verified |
| README states the true release state | registry serves 1.0.4; 1.0.5 local candidate, not published | verified |
| File-type association harness | not run — needs registry access and must not modify a real user's associations | **not verified (deferred)** |
| `npm install -g` | not run — writes to the machine-wide global prefix | **not verified (deferred)** |
| Registry publish | intentionally not performed | **not done (by design)** |

> **Current-status note, 2026-11-06.** The two rows above marked deferred are
> unaffected, but the association harness has since been run against the packaged
> `npm/mink/windows` assets and passed 56/56 (exit 0). That later result is recorded in
> [`M6_RC1_INDEPENDENT_REVERIFICATION.md`](M6_RC1_INDEPENDENT_REVERIFICATION.md); the table
> above remains the accurate record of what this operation itself measured.
