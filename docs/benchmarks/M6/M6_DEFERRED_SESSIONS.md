# M6-M9 - Programme ledger

**Updated:** 2026-11-08 (Session 211 documentation reconciliation)
**Current milestone:** M6 - external developer usability
**Status:** M6 repository acceptance COMPLETE (Sessions 182-183);
published 1.0.4 distribution **BLOCKED** (Sessions 184-185, since resolved by the
2026-09-30 publish of 1.0.4); M6-RC1 repository recovery and 1.0.5 candidate
**COMPLETE**, candidate **NOT PUBLISHED**; M7, M8, M9 NOT STARTED

## Session status

| Session | Planned work | Status |
| ---: | --- | --- |
| 174 | M6 experiment design and baseline | **COMPLETE** |
| 175 | Installation and environment validation | **COMPLETE** |
| 176 | First program and basic language learning | **COMPLETE** |
| 177 | Language documentation audit | **COMPLETE** |
| 178 | Standard-library discoverability | **COMPLETE** |
| 179 | Debugging/testing workflow (consolidated; includes diagnostics) | **COMPLETE** |
| 180 | Cold-start application and verifier (consolidated; absorbs original 182-183) | **COMPLETE** |
| 181 | Cross-cutting usability audit and M6 decision (consolidated; absorbs original 185 decision) | **COMPLETE** |
| 182 | Defect closure (environment module, zip reader, nested Vec, version) | **COMPLETE** |
| 183 | M6 final acceptance and closure | **COMPLETE** (decision: COMPLETE; no external developer study was performed) |
| 184 | Published package, clean installation and distribution integrity | **COMPLETE** (distribution defects verified) |
| 185 | Windows installation, association and CLI first-run audit (original closure slot remains decided in 183) | **COMPLETE** (association blocked from published package) |
| M6-RC1 | Repository recovery, 1.0.5 candidate preparation, full verification and GitHub synchronization (operation ID, not a session number) | **COMPLETE** (candidate verified from its own tarball; registry still serves 1.0.4) |
| 186-197 | M7 tooling and ecosystem demand | **NOT STARTED** |
| 198-203 | M8 strategic evaluation and decision | **NOT STARTED** |
| 204-209 | M9 conditional platform expansion evaluation | **NOT STARTED** |

M5 is COMPLETE. Its final report and readiness gate remain authoritative for
the application evidence used by M6.

*Dated correction, 2026-11-08 (Session 211).* The session-slot column above
reserved **186-197 for M7**, **198-203 for M8** and **204-209 for M9**. Those
slots were not used as planned: sessions **189-210** were consumed by M6 release
finalisation, runtime reliability, capacity, soak and release-readiness work,
recorded as `M6_SESSION_189_…` through `M6_SESSION_210_…`. **M7, M8 and M9 are
still NOT STARTED** and no M7/M8/M9 work exists in the repository. M6 repository
acceptance is complete, but **M6 external developer validation - a developer who
did not build MINK - remains OUTSTANDING**; the internal cold-start sessions
(174-181) do not satisfy that requirement. Linux remains frozen, M7 remains
demand-gated, and MINK 1.0.5 remains unpublished. See
[`M6_SESSION_210_FINAL_ACCEPTANCE.md`](M6_SESSION_210_FINAL_ACCEPTANCE.md).

## Sessions 184-185 result - published 1.0.4 is not repository HEAD

The published artifact was tested as an external developer receives it. It
is the tag `v1.0.4` artifact: **28 files**, no `windows/`, and pre-fix
`strings.mink` / `environment.mink`. The downloaded tarball's SHA-1/SHA-512
match the registry and the release record, so this is not a corrupt upload.

The package installs cleanly, reports `mink 1.0.4`, and runs an intrinsic-only
first program from a path with spaces. It does **not** complete the
standard-library path: `str_trim` over a heap string exits 106 (`E-R06`), and
`mod environment;` fails `E-P01`. The Session 180 verifier stops at its first
case; the same binary with the repository stdlib substituted passes **12/12**.
The repository package has 34 files, includes five `windows/` assets, and
passes the verifier, but that package is **not published**.

The repository bundle guard was fixed by syncing
`npm/mink/stdlib/environment.mink` (commit `b67fda4`), so a future pack is
consistent with `stdlib/`. The already-published 1.0.4 artifact cannot be
repaired in place.

The Windows association itself is safe: the repository harness passed
**56/56** checks, uses per-user `HKCU`, installs no default open verb,
quotes `%1`, and restores the registry on uninstall. The published package
omits `windows/`, so an npm-only user cannot perform the documented
association.

**External distribution gate: BLOCKED.** A corrected release with a new
version must be built from a commit containing the post-tag stdlib and
Windows assets, installed from its tarball, and re-run through the Session
180 verifier plus the association harness. Do not publish from the current
working tree, do not reuse 1.0.4, and do not start M7.

Full evidence:
[`M6_SESSION_184_DISTRIBUTION_VERIFICATION.md`](M6_SESSION_184_DISTRIBUTION_VERIFICATION.md)
and
[`M6_SESSION_185_WINDOWS_FIRST_RUN_AUDIT.md`](M6_SESSION_185_WINDOWS_FIRST_RUN_AUDIT.md).
Installation guide:
[`docs/language/WINDOWS_INSTALLATION.md`](../../language/WINDOWS_INSTALLATION.md).

## Sessions 182-183 result

Session 182 investigated the four blockers and Session 183 judged the result
independently.

- `stdlib/environment.mink` was repaired: top-level `extern fn` declarations
  were replaced with wrappers over the predeclared intrinsics. `mod
  environment;` now loads and the wrappers are covered by
  `tests/environment_lib.rs`.
- The ZIP reader "leak" was an unfreed pass-back string in the probe. A
  seven-case per-call matrix is clean when every owned return is released, and
  `tests/zip_lib.rs` now pins the full reader ownership contract.
- The nested `Vec<Str>` "corruption" was a probe that freed the vector, read
  its length and freed it again. The minimal reproduction shows the stale
  length and E-R04; the corrected lifecycle is covered in
  `tests/collections_lib.rs`.
- The installed 1.0.2 compiler predates the bundled `strings.mink` ownership
  fixes. The registry now serves 1.0.4, which is the minimum verified version;
  1.0.2 is explicitly unsupported.
- README module-loading wording and the collection type list were corrected in
  an isolated 2-line commit without committing its pre-existing worktree edits.

Focused regression: **60 tests passed** (2 environment, 11 zip, 47
collections). Session 180 verifier: **12/12** on 1.0.4. README fences 5/5,
reference fences 12/12, local links 0 broken.

M6 is **COMPLETE**, with accepted and documented limitations: collection
use-after-free is not diagnosed (unsupported contract), 1.0.2 is unsupported,
some modules remain source-audited only, and no external developer study was
performed.

Full evidence:
[`M6_SESSION_182_DEFECT_CLOSURE.md`](M6_SESSION_182_DEFECT_CLOSURE.md) and
[`M6_SESSION_183_FINAL_ACCEPTANCE.md`](M6_SESSION_183_FINAL_ACCEPTANCE.md).

Next session: **corrected release preparation**. Do not advance to M7.

## Sessions 178-181 consolidated result

The four sessions were executed as one assignment. Scope was remapped: the
original 179 (diagnostics), 180 (testing) and 181 (workload selection) became
one debugging/testing guide; the original 182-183 application work moved into
180; the original 185 closure decision was taken in 181. The original table
rows are preserved above with the remap noted.

M6 is **PARTIAL**, not complete. The deliverables exist and the Session 180
application passes 12/12 verifier cases with the repository 1.0.4 compiler, but
these concrete acceptance gaps remain:

1. `stdlib/environment.mink` cannot be loaded (`E-P01`); `rt_env_*` works only
   as predeclared intrinsics.
2. The `zip` reader leaks in a composite probe (exit 106); the source was not
   isolated.
3. A `Vec<Str>` accumulated inside nested `if`/`else` blocks was corrupted
   (length 16, `E-R04`); no minimal compiler reproduction was produced.
4. Installed compiler 1.0.2 builds the Session 180 application but exits 106 on
   normal input; 1.0.4 exits 0. Cross-version parity is unverified.
5. `README.md` still says stdlib is "imported with `use`" and omits collection
   types; its pre-existing uncommitted changes were preserved, not edited.
6. Collection use-after-free remains undiagnosed (preserved known defect).
7. No exhaustive standard-library coverage claim is made.

*Dated correction, 2026-11-06 (Session 195):* item 6 is closed. Collection
handles are validated against the runtime liveness table before their header is
read, so use after free exits 105 (`E-R05`) instead of returning stale bytes.
See [`M6_SESSION_195_COLLECTION_UAF_FIX.md`](M6_SESSION_195_COLLECTION_UAF_FIX.md).

*Dated correction, 2026-11-06 (Session 197):* the freed-then-reused residual
from Session 195 is closed too. Handles carry the allocation generation, so a
stale handle faults `E-R05` even after its block is reused. See
[`M6_SESSION_197_GENERATION_SAFE_HANDLES.md`](M6_SESSION_197_GENERATION_SAFE_HANDLES.md).

Full evidence and file list:
[`M6_CONSOLIDATED_SESSIONS_178_181.md`](M6_CONSOLIDATED_SESSIONS_178_181.md).

The remaining gaps were closed in Sessions 182-183. See
[`M6_SESSION_182_DEFECT_CLOSURE.md`](M6_SESSION_182_DEFECT_CLOSURE.md) and
[`M6_SESSION_183_FINAL_ACCEPTANCE.md`](M6_SESSION_183_FINAL_ACCEPTANCE.md).
Do not advance to M7.

## Session 177 result

Session 177 verified the Session 176 findings against source and execution,
corrected the module/import grammar, added
[`docs/language/LANGUAGE_BASICS.md`](../../language/LANGUAGE_BASICS.md), and
added `benchmarks/m6/session177/verify.py`. The full evidence and deferred
issues are in
[`M6_SESSION_177_DOCUMENTATION_AND_LANGUAGE_LEARNING_AUDIT.md`](M6_SESSION_177_DOCUMENTATION_AND_LANGUAGE_LEARNING_AUDIT.md).

## Starting point for Session 178

Read:

- [`M6_SESSION_176_FIRST_PROGRAM_AND_LANGUAGE_LEARNING.md`](M6_SESSION_176_FIRST_PROGRAM_AND_LANGUAGE_LEARNING.md)
- [`M6_SESSION_177_DOCUMENTATION_AND_LANGUAGE_LEARNING_AUDIT.md`](M6_SESSION_177_DOCUMENTATION_AND_LANGUAGE_LEARNING_AUDIT.md)
- [`docs/language/LANGUAGE_BASICS.md`](../../language/LANGUAGE_BASICS.md)
- `stdlib/*.mink` and `examples/`

Then audit standard-library discoverability and practical API contracts. Keep
the collection use-after-free defect and the qualified-path diagnostic as
isolated implementation follow-ups, and correct the README only after its
pre-existing worktree changes are resolved.

## Rules for this block

- Do not add language features during M6.
- Do not claim external validation: Session 174 is an internal cold-start
  simulation.
- Do not advance to M7 until M6 has a closure decision.
- Do not push, publish, tag or change the release version.
- Keep pre-existing M3/M4 worktree changes untouched.

## Recorded deviations (M6-RC1)

The three rules above that constrain distribution were relaxed once, by explicit user
request for the recovery operation, and only in the ways listed here:

- **Push:** the recovered commits were pushed to the existing `main` of `origin`. No tag
  was created, moved or deleted, and no force-push was used.
- **Version:** the manifests were raised to the unpublished candidate 1.0.5. Nothing was
  published; the registry still serves 1.0.4 on 2026-09-30.
- **Worktree:** the pre-existing untracked files were preserved, not discarded
  (`MINK UI SETS OFFICIAL.zip`, the chat export, `nl1.mink`, `nl1.exe`).

Records: [`M6_REPOSITORY_RECOVERY_AUDIT.md`](M6_REPOSITORY_RECOVERY_AUDIT.md),
[`M6_RELEASE_CANDIDATE_VERIFICATION.md`](M6_RELEASE_CANDIDATE_VERIFICATION.md),
[`M6_FULL_REGRESSION_AND_RELEASE_AUDIT.md`](M6_FULL_REGRESSION_AND_RELEASE_AUDIT.md),
[`M6_GITHUB_SYNCHRONIZATION.md`](M6_GITHUB_SYNCHRONIZATION.md).

## Deferred, not completed by M6-RC1

- **File-type association harness** (`validation/windows_filetype/verify_filetype_association.ps1`)
  is no longer deferred: it was run on 2026-11-06 against the **packaged**
  `npm/mink/windows` assets and passed **56/56** with exit 0, including uninstall
  restoration, per-user `HKCU` only, and no `HKLM` write. Its own restore pass and
  independent key queries confirmed every MINK-owned entry was removed afterwards.
  See [`M6_RC1_INDEPENDENT_REVERIFICATION.md`](M6_RC1_INDEPENDENT_REVERIFICATION.md) §7.
- **`npm install -g`** was not exercised: it writes to the machine-wide global prefix.
  Installs were verified from the tarball into isolated prefixes instead, including paths
  with spaces and non-ASCII characters.
- **Publishing 1.0.5** is a maintainer action requiring a valid npm token. Re-checked
  on 2026-11-06 (Session 189): `npm whoami` still returns `E401 Unauthorized`, the
  registry still serves 1.0.4 as `latest`, and 1.0.5 is still unclaimed. The verified
  tarball (`cfafc7a8…df6f`) is unchanged. Exact procedure:
  [`M6_1.0.5_MAINTAINER_PUBLICATION_CHECKLIST.md`](M6_1.0.5_MAINTAINER_PUBLICATION_CHECKLIST.md).
- **Pre-existing `cargo fmt` drift** (49 diffs in files untouched by this operation) was
  left as found rather than reformatting unrelated code.
