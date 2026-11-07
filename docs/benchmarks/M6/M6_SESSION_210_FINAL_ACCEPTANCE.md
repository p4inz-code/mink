# Session 210 — Final Independent Acceptance and Release-Readiness Audit

Windows x86_64, 2026-11-07. Starting commit
`1c5387d94d4a11c750135500d3c48bf1f0871c77` (Session 209, local == origin/main
verified before any change).

## 1. Repository integrity

- Branch `main`; no merge markers (`git grep` for `<<<<<<<`/`>>>>>>>`: none).
- Worktree clean apart from the four pre-existing untracked user files:
  `MINK UI SETS OFFICIAL.zip`, `freebuff-chat-2026-10-01T19-08-00.600Z.md`,
  `nl1.exe`, `nl1.mink`. They were preserved and never staged.
- Session commits are separate, linear and pushed:
  `c6c6ce7` (205), `f298321` (206), `f2b95e1` (207), `a6f0732` (208),
  `1c5387d` (209).
- No generated executable was added to the repository. Tracked `.exe` files are
  the npm payload and the long-standing `tests/json/*.exe` fixtures.
- Version metadata: `Cargo.toml` 1.0.5 and `npm/mink/package.json` 1.0.5.
  The npm payload `npm/mink/bin/mink.exe` reports `mink 1.0.5`; its hash
  (`c288115d…`) predates Session 206 and differs from the current
  `target/release/mink.exe` (`116a2073…`). No package was published or
  rebuilt, and no publish/tag/release action was performed.
- Private-key hits are limited to the committed TLS test fixtures
  (`tests/tls/*.key.pem`) and the security-architecture document that quotes
  them; they are intentional test material, not credentials.

## 2. Build and regression

| Check | Result |
| --- | --- |
| Debug build | **pass** |
| Full regression, all 84 test targets + lib | **85 suites, 3,262 passed, 0 failed, 9 ignored, 0 non-zero exits** |
| `tls_lib` | 21 passed / 1 ignored (opt-in public internet) |
| Filesystem, large-file, capacity, cross-subsystem, signature rechecks | 6/6, 5/5, 7/7, 4/4, 1/1 |
| `rustfmt --check` on new/changed test files | clean (runtime emitter has its pre-existing line-380 diff only) |
| **Release build** | **FAILED — environment blocker** |

Against the Session 204 baseline (81 suites / 3,231 / 7 ignored), the growth is
+4 suites, +31 passed, +2 ignored: `tls_lib` now actually runs (21 + 1), plus
`large_file` (5), `stdlib_signatures` (1), `cross_subsystem` (4) and the
ignored extended soak. Nothing was removed, skipped or weakened.

Release failure (reproduced after `rustup toolchain install 1.97.1 --force`;
toolchain reported unchanged and intact):

```
rustc ... -C opt-level=3 ... (exit 0xc000001d, STATUS_ILLEGAL_INSTRUCTION)
rustc ... -C opt-level=3 ... (exit 0xc0000374, STATUS_HEAP_CORRUPTION)
rustc ... -C opt-level=3 ... (exit 0xc0000005, STATUS_ACCESS_VIOLATION)
```

It fails on previously passing release targets, with `-j 1`,
`CARGO_INCREMENTAL=0`, `codegen-units=1`, `target-cpu=x86-64` and opt-level 2,
while a trivial release crate compiles and debug builds work. The varying
exception codes point to toolchain/machine memory corruption, not a MINK
diagnostic. Release build and release-mode suites are therefore **not run**.

## 3. Independent reproduction

Re-run after the Session 209 commit, not relying on earlier transcripts:
`fs_read` (locked-file failure, 6/6), `large_file` (4 GiB boundary, 5/5),
`capacity` (1 GiB arena, 1,024 table, generation retirement, 7/7),
`cross_subsystem` (4/4) and `stdlib_signatures` (66 signatures, 1/1). All pass.

## 4. Documentation

Session records 205–209, the roadmap and the readiness audit are updated and
consistent with the measured results. Linux remains frozen; M7 remains
demand-gated; M6 external developer validation is **not** claimed complete.
No changelog structure was invented.

## 5. Release metadata

`Cargo.toml` and npm metadata agree on 1.0.5; the npm `files` allowlist is
`bin/mink.exe`, `stdlib`, `windows`, `LICENSE`, and `os`/`cpu` are win32/x64.
The package remains **unpublished**; no tag or GitHub release exists.

## 6. Final verdict

**NOT READY.** All internal debug evidence is clean and all five prior sessions
pass, but release-mode verification is a required acceptance item and cannot
be produced on this machine: every `rustc` release compile crashes inside the
compiler with three different memory-corruption exception codes, including
after a forced toolchain reinstall. This is an environment blocker, not a
confirmed MINK defect, but it leaves the release build and release-mode suites
unverified. External M6 validation is also still outstanding.

**Next action:** repair or replace the Rust toolchain/host (clean Windows
machine or fresh MSVC toolchain), produce a release build and rerun the
release-mode suites; only then can the verdict advance to READY FOR EXTERNAL
VALIDATION.
