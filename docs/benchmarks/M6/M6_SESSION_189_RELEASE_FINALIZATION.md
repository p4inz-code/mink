# Session 189 — 1.0.5 Release Finalization

Windows x86_64, 2026-11-06. Starting HEAD
`85713dd472f47944f7928c3491ed2cb7876517d0`.

Scope: bring the verified 1.0.5 candidate to a maintainer-ready release state
without publishing, tagging or releasing anything. The candidate's own
verification was not repeated except where a fresh check was needed; one genuine
defect was found and fixed.

## 1. Baseline

| Item | Value |
| --- | --- |
| Branch | `main` |
| HEAD | `85713dd472f47944f7928c3491ed2cb7876517d0` |
| `origin/main` | same commit, `0  0` |
| `git ls-remote origin refs/heads/main` | same commit |
| Worktree | 4 untracked, 0 modified, 0 staged |
| Tags | `v1.0.0` … `v1.0.4` |

The four untracked entries (`MINK UI SETS OFFICIAL.zip`, the chat export,
`nl1.mink`, `nl1.exe`) were preserved and never staged.

## 2. Release audit

| Check | Result |
| --- | --- |
| `Cargo.toml` version | `1.0.5` |
| `Cargo.lock` (`name = "mink"`) | `1.0.5` |
| `npm/mink/package.json` version | `1.0.5` |
| `package.json` `files` | `bin/mink.exe`, `stdlib`, `windows`, `LICENSE` |
| `package.json` platform gates | `os: ["win32"]`, `cpu: ["x64"]`, `engines.node >= 18` |
| Packaged binary | 4,028,416 bytes, SHA-256 `c288115d…79401` (unchanged) |
| Tarball | 1,449,290 bytes, SHA-256 `cfafc7a8…df6f` (unchanged) |
| Package contents | 35 files, 27 `stdlib/` modules, 5 `windows/` assets |
| Local vs packaged `stdlib/` | byte-identical |
| References implying 1.0.5 is published | none found |
| Changelog file | none exists; the README "What's New" section is the project's release-notes convention |

`npm/mink/` also holds `p4inz-code-mink-1.0.2.tgz` and `…-1.0.3.tgz`. They are
untracked and matched by `.gitignore:11` (`npm/mink/*.tgz`), so they are local
build leftovers, not repository content, and they cannot enter the package
because `files` is explicit.

## 3. npm readiness

| Question | Answer |
| --- | --- |
| Is a valid npm session available? | **No** — `npm whoami` → `E401 Unauthorized` |
| Registry versions | `1.0.1`, `1.0.2`, `1.0.3`, `1.0.4` |
| `latest` dist-tag | `1.0.4` |
| Is `1.0.5` still available? | Yes |
| Does the tarball hash still match? | Yes — `cfafc7a8…df6f` |

No credential, token or credential file was read, printed or modified. Because
authentication is unavailable, publication is blocked independently of the
authorization boundary that forbids it in this session.

The exact maintainer procedure is in
[`M6_1.0.5_MAINTAINER_PUBLICATION_CHECKLIST.md`](M6_1.0.5_MAINTAINER_PUBLICATION_CHECKLIST.md).

## 4. Reproducibility from committed source

Verified by a genuine rebuild, not an incremental no-op. The first attempts
appeared to succeed in 0.01s because cargo re-hardlinked a cached artifact, so
the build cache was cleared (`cargo clean`, build artifacts only) and the crate
was rebuilt from source.

| Observation | Result |
| --- | --- |
| Clean build succeeds | Yes (after retries, see below) |
| Binary size | 4,028,416 bytes — identical to the candidate |
| Binary SHA-256 | `2568bbbf2a0488ed3fd99787a69729d92c52b03e9bc3dccc348265b319ccbae2` |
| Candidate SHA-256 | `c288115d8489571df2384953170e67da9bbcd45c3bedbf569db65714a2479401` |
| `--version` | `mink 1.0.5` |

So the candidate is **functionally** reproducible (same size, same version, same
behaviour) but **not bit-reproducible**. Two of the first clean-build attempts
crashed inside `rustc` with `STATUS_ILLEGAL_INSTRUCTION` (`0xc000001d`) and
`STATUS_ACCESS_VIOLATION` (`0xc0000005`); three subsequent identical runs all
succeeded, and `19.8 GB of 31.7 GB` was free, so this is host/toolchain
instability rather than a source defect. Consequence: a freshly built compiler
must never be copied over `npm/mink/bin/mink.exe`, or the verified artifact
identity is lost. The packaged binary was left untouched and still hashes to
`c288115d…`.

## 5. Defect found and fixed

**Symptom.** The first full-suite run of this session failed:

```text
---- s62_large_transfer stdout ----
thread 's62_large_transfer' panicked at tests\tls_lib.rs:645:23:
attempt to subtract with overflow
```

**Root cause.** `tests/tls_lib.rs` built its file-server fixtures in
`file_root()`:

```rust
if !root.join("big.bin").exists() {
    let big: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
    std::fs::write(root.join("big.bin"), big).expect("failed to write big.bin");
}
```

Fifteen tests share the `"x"` root and run on parallel threads. `fs::write`
truncates before writing, and the guard tests `exists()` rather than
completeness, so a second thread could see the file existing at zero length,
skip creation, and return immediately. `s62_large_transfer` then did
`&big[big.len() - 64..]` on a short buffer and underflowed. The same race
applied to `index.html`, which every caller rewrote unconditionally.

It only appears when the `target/mink-artifacts` directory is being created for
the first time under load — which is exactly what a fresh clone, CI run, or a
`cargo clean` produces. Re-running with the fixture already present always
passed, which is why the earlier 3,176-pass run did not catch it.

**Fix** (`137d270`):

- `FIXTURE_LOCK` serialises all fixture creation.
- `write_if_changed` writes through a private temp file and a rename, and skips
  the write entirely when the bytes already match, so a reader sees either the
  previous complete file or the new complete file, never a truncated one.
- `big.bin` completeness is decided by size (`BIG_BIN_LEN`), not by `exists()`.
- `s62_large_transfer` now fails with a named assertion instead of an arithmetic
  panic, so a future regression is diagnosable.

**Regression test.** `s62_fixture_creation_is_race_free` uses a marker unique to
each run, so the fixture is always created while eight threads are racing to
create it, and asserts the file is never observed at a partial size.

**The test is proven sensitive**: with the fix temporarily reverted, it fails on
every one of three consecutive runs, reporting `big.bin was observed while only
0 bytes were on disk`. With the fix in place it passes on every run.

## 6. Verification

All commands run from the repository root, with `/tmp/pyshim` on `PATH` so the
TLS tests reach the real Python 3.11 instead of the Store stub.

| Check | Command | Result |
| --- | --- | --- |
| Full regression | `cargo test` | exit 0 — **3,177 passed, 0 failed, 1 ignored, 78 suites** |
| Release suite | `cargo test --test release` | 69 passed, 0 failed |
| Environment suite | `cargo test --test environment_lib` | 3 passed, 0 failed |
| TLS suite (clean fixtures, 5×) | `cargo test --test tls_lib` | 20 passed, 0 failed, 1 ignored, every run |
| Association harness | `verify_filetype_association.ps1` | exit 0 — **56 PASS, 0 FAIL**, `RESULT: PASS - 56 checks` |
| Registry restored after harness | key queries | `HKCU\Software\Classes\.mink` absent, `MINK.SourceFile` absent, `HKLM\Software\Classes\.mink` absent |
| README relative links | script | 15 targets, 0 broken |
| README code fences | script | 46 markers (balanced) |
| Whitespace errors | `git diff --check` | exit 0 |
| Changed-file formatting | `rustfmt --check tests/tls_lib.rs` | exit 0 |
| Secret / absolute-path scan | staged diff grep | no matches |

The full-suite count rose from 3,176 to 3,177 because of the new regression
test; nothing else changed. `cargo fmt -- --check` across the whole crate still
reports the 49 pre-existing diffs in files untouched by this session — an
unchanged, documented condition, not a regression.

## 7. Documentation

| File | Change |
| --- | --- |
| `M6_1.0.5_MAINTAINER_PUBLICATION_CHECKLIST.md` | new — exact publication procedure |
| `M6_SESSION_189_RELEASE_FINALIZATION.md` | new — this record |
| `M6_DEFERRED_SESSIONS.md` | publication item points at the checklist |
| `README.md` | **not changed** — its release-state note already states that the registry serves 1.0.4 and that 1.0.5 is not published |

No historical record was rewritten. The README was checked against the five
required distinctions (published version, prepared candidate, publication
status, tag status, remaining maintainer action) and needed no correction; tag
status and the maintainer action are stated in the checklist, which is release
documentation.

## 8. Commits

| Commit | Purpose |
| --- | --- |
| `137d270fe27622905dac0346ec483697c009c631` | `test(tls)` — atomic fixture creation + regression test |
| see `git log` for the docs commit | Session 189 records |

## 9. Deferred, with reasons

- **npm publication** — no valid authentication (`E401`) and no authorization
  in this session.
- **Package README** — the npm package has no `README.md` (`npm view
  @p4inz-code/mink readme` → `No README data found!`), so the registry page has
  no body. Adding one would change the package contents and therefore
  invalidate the verified tarball hash `cfafc7a8…`. Deferred to the next release
  rather than re-verifying the whole candidate for a cosmetic gain.
- **`npm install -g`** — writes the machine-wide global prefix; unchanged from
  the previous session's deferral.
- **Bit-reproducible release builds** — would need a pinned toolchain and
  reproducible-build settings; out of scope here.
- **Pre-existing `cargo fmt` drift** — 49 diffs in untouched files, left alone.

## 10. Status

| Gate | Status |
| --- | --- |
| Candidate internally consistent at 1.0.5 | verified |
| Tarball identity unchanged | verified |
| Package contents complete and correct | verified |
| Release build from committed source | works; **not bit-identical** |
| Full regression green from a clean fixture state | verified (3,177 / 0 / 1) |
| Association harness | verified (56/56) |
| Release documentation states the true state | verified |
| npm authentication available | **no** |
| Published | **no** |
| Tagged / released | **no** |

**Release readiness: complete**, except that publication is separately gated on
maintainer authentication and authorization.

## 11. Exact next action

The maintainer configures a valid npm token for an account with write access to
`@p4inz-code/mink`, copies the verified tarball out of `/tmp`, and follows
[`M6_1.0.5_MAINTAINER_PUBLICATION_CHECKLIST.md`](M6_1.0.5_MAINTAINER_PUBLICATION_CHECKLIST.md).
