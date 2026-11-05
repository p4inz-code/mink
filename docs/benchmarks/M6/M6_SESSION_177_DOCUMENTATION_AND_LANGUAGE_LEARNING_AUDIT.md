# M6 Session 177 - documentation and language-learning audit

**Date:** 2026-11-05
**Status:** **COMPLETE**
**Starting HEAD:** `3f4a7ee14812693eb27393dd1f78200a12dcd53b`
**Branch:** `main` (34 ahead / 1 behind `origin/main`)
**Origin reference:** `b860837a8a6039f304a503e3d85728a643ab470b`
**Evidence mode:** repository-internal audit; no external developer

## A. Session identity

| Item | Value |
| --- | --- |
| Session | 177 - Documentation and language-learning audit |
| Next session | 178 - Standard library discoverability and practical API audit |
| Final status | **COMPLETE** |
| Compiler under test | `target/release/mink.exe` and installed `@p4inz-code/mink@1.0.4` |
| Version | `mink 1.0.4` (both binaries) |

The deliverable is a verified correction of developer-facing documentation,
not a behavior change. No compiler, runtime, standard-library, Linux-frozen,
packaging, or version file was changed.

## B. Baseline

| Item | Value |
| --- | --- |
| Starting HEAD | `3f4a7ee14812693eb27393dd1f78200a12dcd53b` |
| Branch | `main` |
| `origin/main` | `b860837a8a6039f304a503e3d85728a643ab470b` |
| Divergence | 34 ahead / 1 behind |
| Starting worktree | 63 entries: 37 modified, 26 untracked |
| Ending worktree | Same 63 pre-existing entries preserved, plus this session's new and changed paths |
| Pre-existing preservation | No pre-existing file was reverted, staged, or committed |

The handoff state was accurate. The pre-existing entries are the M3/M4 work
recorded in earlier sessions, including `README.md`, `LICENSE`, the backend and
runtime sources, `stdlib/http.mink`, `stdlib/network.mink`, their `npm/mink`
copies, and the untracked Session 128-149 records and `httpserver` work.

`README.md` had substantive uncommitted edits at baseline. It was deliberately
not modified, because committing it would mix pre-existing work into this
session's patch. The README contradictions found below are recorded as
deferred instead of being hidden.

Files that were clean at baseline and are safe for this session:

- `docs/language/CORE_GRAMMAR.md`
- `docs/language/CORE_LANGUAGE.md`
- `docs/benchmarks/M6/M6_DEFERRED_SESSIONS.md`
- new `docs/language/LANGUAGE_BASICS.md`
- new `benchmarks/m6/session177/verify.py`
- new session record

## C. Investigation

### C.1 Documentation inventory

| Material | Role | Findings |
| --- | --- | --- |
| `README.md` | Install, build, run, feature and library overview | No line-oriented print contract, no map typing, no cleanup rules, no filesystem return contract, no compiler/stdlib resolution; says stdlib is "imported with `use`" |
| `docs/language/CORE_GRAMMAR.md` | Syntax authority | Accurate syntax; sections 28-30 described `use`-based module visibility that the implementation does not require |
| `docs/language/CORE_LANGUAGE.md` | Design direction and historical decisions | Section 26 still said bindings have no annotations and there is no borrow syntax |
| `docs/language/TYPE_SYSTEM.md` | Type-system design | No practical collection reference |
| `docs/language/ERROR_SYSTEM.md` | Error-system design | No public runtime-error table or exit-code guidance |
| `docs/core/MINK_MASTER_SPEC.md` | Product and architecture thesis | Not a developer reference |
| `docs/implementation/*` | Internal implementation records | Authoritative for behavior, but not the material a beginner is pointed to |
| `stdlib/*.mink` | De facto standard-library API reference | Function comments carry ownership and return conventions; no single index |
| `examples/*/main.mink` | 15 complete programs | Compile-only sweep: 15/15 build with the repository compiler |
| `benchmarks/m6/session176/` | Ten exercises, three programs, verifier | 15/15 with installed compiler; 13/15 with repository compiler |

Canonical source of truth after this session:

- Syntax: `CORE_GRAMMAR.md`.
- Practical language and runtime behavior: `LANGUAGE_BASICS.md`.
- Implementation detail: `docs/implementation/`.
- Standard-library functions: `stdlib/*.mink` comments (a consolidated API
  index remains Session 178 work).

### C.2 Coverage matrix

| Topic | Documented? | Implementation verified? | Contradiction? | Action |
| --- | --- | --- | --- | --- |
| Program entry point | Yes (README) | Yes (`main`, exit code, `mink run`, `mink check`) | No | None |
| Variables and types | Partial (README, grammar) | Yes | README type list omits collections | Documented in guide; README deferred |
| Expressions and operators | Yes (grammar) | Yes | No | None |
| Conditionals | Yes (grammar) | Yes | No | None |
| Loops | Yes (grammar) | Yes | No | None |
| Functions | Yes (grammar) | Yes | No | None |
| Return behavior | Yes (grammar, README) | Yes | No | None |
| Strings | Partial (README example) | Yes | No | Guide section 5 |
| Arrays | Yes (README, grammar) | Yes | No | None |
| Maps | No | Yes | Session 176 implied no supported cross-function use | Guide section 3; verifier |
| Modules | Yes (grammar) | Yes | Yes, `use` wording | Grammar corrected; guide section 4 |
| Filesystem | Partial (README table) | Yes | No | Guide section 6 |
| Memory cleanup | Partial (README claims, stdlib comments) | Yes | No | Guide section 5 |
| Printing/output | Partial (examples only) | Yes | No | Guide section 2 |
| Errors | Partial | Yes | No | Guide section 8 |
| Standard-library discovery | Partial (README) | Yes | Yes, import wording | Guide section 7; README deferred |
| Compilation and execution | Yes (README) | Yes | No | Guide section 1 |

### C.3 Findings A-G

#### Finding A - print behavior

**Original observation:** print intrinsics are line-oriented and cannot build
one line across calls.

**Sources inspected:** `src/backend/ir.rs` (`PrintStr`, `PrintInt`,
`PrintFloat`, `PrintChar`), `src/runtime/intrinsics.rs`,
`src/backend/lower.rs` (intrinsic routing), `src/backend/emit/runtime.rs`.

**Tests/evidence:** Session 176 cases 02, 08 and 10 failed on output shape;
Session 177 `verify.py` cases `print_line_shape`, `compose_line`,
`format_line`, `float_format`; byte-level probe `od -c`.

**Reproduction:**

```mink
fn main() {
    rt_print_str("a=");
    rt_print_int(7);
}
```

Output bytes: `a=\r\n7\r\n`. `rt_print_str("")` writes a blank line. A string
containing `\n` produces an extra break because the intrinsic appends CRLF.
There is no same-line print intrinsic. One line requires one `Str` built with
`rt_str_concat`, `rt_str_from_int`, `rt_str_from_float`, `rt_str_from_bool`,
or `rt_str_format`.

**Actual behavior:** each call writes its value plus CRLF.

**Confidence:** high (source and bytes agree).

**Documentation status:** missing from README and `CORE_GRAMMAR.md`.

**Classification:** VERIFIED + DOCUMENTATION GAP.

**Action:** `LANGUAGE_BASICS.md` section 2; verifier cases.

#### Finding B - map handle typing

**Original observation:** a map passed through an `Int`-typed function produced
`E-T01 expected Map<unresolved, unresolved>, found Int`, and Session 176
concluded that map handles could not cross function boundaries.

**Sources inspected:** `src/parser/mod.rs` (`parse_type` generic application),
`src/typecheck/checker.rs` (`Map` and `Set` resolution, collection key rules),
`src/typecheck/ty.rs` (`TypeKind::Map`), `src/ownership/mod.rs` (collection
borrow/consume rules), `src/runtime/intrinsics.rs` (map signatures).

**Tests/evidence:** probes `p2_map.mink`, `p7_map_infer.mink`,
`p7_map_bad.mink`, `p7_map_move.mink`, `p10_map_ret.mink`,
`p15_unresolved.mink`; verifier case `map_crossing`.

**Actual behavior:**

- `fn total(m: Map<Int, Int>) -> Int` accepts a map.
- `fn build() -> Map<Int, Int>` returns a map.
- An unannotated parameter used with `rt_map_get` is inferred as a map.
- Passing a map to an `Int` parameter is
  `E-T01 expected Int, found Map<Int, Int>`.
- A partially inferred parameter can report `Map<unresolved, Int>`.
- A map can be passed to a user function more than once, then freed.
- Keys may be `Int`, `Bool`, `Char`, `Str`, or a unit-variant-only enum.
- `rt_map_insert` consumes and returns the handle; `rt_map_get`,
  `rt_map_has`, `rt_map_len` borrow it; `rt_map_remove` consumes and returns
  it; `rt_map_free` releases it.

Session 176's workaround (keep maps in `main`) was unnecessary for correct
programs. The limitation was documentation and diagnosis, not the type
system.

**Confidence:** high.

**Documentation status:** missing; the only public mention of map typing was
the error text.

**Classification:** PARTIALLY VERIFIED (the diagnostic is real; the
"cannot cross functions" conclusion is not) + DOCUMENTATION GAP.

**Action:** `LANGUAGE_BASICS.md` section 3; verifier.

#### Finding C - module and import wording

**Original observation:** `CORE_GRAMMAR.md` says module items become visible
through `use`, while examples call `fs_*` functions after only `mod
filesystem;`.

**Sources inspected:** `src/driver.rs` (`resolve_module_path` search order),
`src/semantics/analyzer.rs` (`resolve_imports`), `src/ast/mod.rs`
(`UseDecl`), `src/parser/mod.rs` (`parse_use_decl`).

**Tests/evidence:** probes `p3_mod_only.mink`, `p3_use.mink`,
`p3_use_private.mink`, `p3_no_mod.mink`, `p3_use_only.mink`,
`p7_qualified.mink`; verifier cases `module_unqualified`,
`module_private_unqualified`, `module_use_private`.

**Actual behavior:**

- `mod helper;` loads `helper.mink` (or `helper/mod.mink`) and makes its
  top-level items callable unqualified. `use` is not required.
- Private items are also reachable.
- `use helper::double;` binds one item, but does not load the file. Without
  `mod helper;`, the call fails `E-S01 cannot find name double`.
- `use helper;` is a no-op.
- Qualified calls (`helper::double(4)`) are not supported. The current
  diagnostic is a misleading `E-T15` that describes `helper` as a key type.
- `pub` is parsed but not enforced.

**Confidence:** high.

**Documentation status:** contradiction in `CORE_GRAMMAR.md` sections 28-30
and in `README.md` line 452 ("imported with `use`").

**Classification:** VERIFIED + DOCUMENTATION CONTRADICTION.

**Action:** `CORE_GRAMMAR.md` corrected; `LANGUAGE_BASICS.md` section 4;
README correction deferred (pre-existing worktree edits).

#### Finding D - ownership and cleanup

**Original observation:** returned `Str` and map handles need explicit frees,
but the rules were only visible in examples.

**Sources inspected:** `docs/implementation/OWNERSHIP_IMPLEMENTATION.md`,
`src/ownership/mod.rs`, `stdlib/filesystem.mink` (V1 ownership contract),
`src/runtime/intrinsics.rs`, `src/backend/emit/runtime.rs` (exit/leak check).

**Tests/evidence:** probes `p4_leak`, `p4_double_free`,
`p4_use_after_free`, `p4_literal_free`, `p6_own`, `p6_own2`, `p11_borrow`,
`p11_map_str`, `p12_compose`, `p16_eq`; verifier cases `compose_line`,
`borrow_string`, `leak_exit`.

**Established rules:**

- String literals are immutable; allocating calls return owned strings.
- Read-only intrinsics observe the value: `rt_str_len`, `rt_str_byte`,
  `rt_print_str`, `rt_str_eq`.
- `rt_str_concat` reads its arguments and allocates a new result; both owned
  inputs still need their own free.
- `rt_str_free` consumes the binding. Double free or later use is `E-S10`.
- A user-function call consumes an owned `Str`; `&Str` plus `*s` is the
  working read-only call pattern.
- Freeing a literal is legal and does nothing.
- Omitting a free leaves a live allocation: `E-R06` on stderr and exit code
  106. Output written before the check is kept.
- Returning a string transfers ownership to the caller.
- `rt_map_free` / `rt_vec_free` / `rt_set_free` release collection handles;
  read-only intrinsics borrow them, and insert/push/remove consume and return
  a handle. Inserting an owned `Str` moves it into the collection.
- **Defect found:** a collection handle used after its free call is not
  diagnosed. `rt_map_free(m); rt_map_get(m, 1);` compiles and runs (observed
  value `42`). This is an ownership-checker gap, not a supported contract.

**Confidence:** high for `Str`; high for collection free, with the
use-after-free gap confirmed by execution.

**Documentation status:** gap; rules existed only in implementation records and
stdlib comments. The collection use-after-free gap is undocumented.

**Classification:** VERIFIED + DOCUMENTATION GAP; new IMPLEMENTATION DEFECT
(collection use-after-free, deferred).

**Action:** `LANGUAGE_BASICS.md` section 5, including the limitation.

#### Finding E - filesystem error conventions

**Original observation:** `fs_write` to a missing directory and
`fs_file_size` on a missing file return `-1`; `fs_read` on a missing file
returns an empty string.

**Sources inspected:** `src/backend/emit/runtime.rs`
(`emit_fs_file_size`, `emit_fs_read`, `emit_fs_write`),
`src/backend/ir.rs` (filesystem service contracts),
`stdlib/filesystem.mink` (wrappers and ownership comments).

**Tests/evidence:** probes `p5_fs2.mink`, `p13_fs.mink`; verifier case
`filesystem_returns`.

**Observed results:**

| Call | Observed |
| --- | --- |
| `rt_fs_write("ok.txt", "abcd")` | `4` |
| `rt_fs_file_size("ok.txt")` | `4` |
| `rt_fs_file_size("missing.txt")` | `-1` |
| `rt_fs_read("empty.txt")` length | `0` |
| `rt_fs_read("missing.txt")` length | `0` |
| `rt_fs_write("no_dir/x.txt", "y")` | `-1` |
| `rt_fs_write("ok.txt", "")` | `0` |
| `rt_fs_file_size("ok.txt")` after empty write | `0` |
| `rt_fs_exists("missing.txt")` | `false` |
| `rt_fs_create_dir` / `rt_fs_remove_file` / `rt_fs_remove_dir` | `0` on success |
| `fs_copy_file` / `fs_move` | `0` on success |
| `fs_dir_open` / `fs_dir_close` | handle / `0` |
| `fs_dir_close(0)` | `-1` |

Missing and empty files are indistinguishable through `fs_read` alone. Use
`fs_exists` or `fs_file_size` (`-1` vs `0`). The wrappers expose no last-error
code.

**Confidence:** high; every row was executed.

**Documentation status:** missing.

**Classification:** VERIFIED + DOCUMENTATION GAP.

**Action:** `LANGUAGE_BASICS.md` section 6; verifier.

#### Finding F - floating-point printing

**Original observation:** `rt_print_float` exists in runtime source and tests
but had no developer-facing reference.

**Sources inspected:** `src/runtime/intrinsics.rs` (public intrinsic table),
`src/backend/ir.rs` (`PrintFloat` comment), `src/backend/lower.rs`,
`docs/implementation/SCALAR_TYPES_IMPLEMENTATION.md`,
`docs/implementation/RUNTIME_IMPLEMENTATION.md`.

**Tests/evidence:** probes `p1_print.mink`, `p7_float.mink`,
`p10_api.mink`; verifier case `float_format`.

**Actual behavior:** `rt_print_float(Float)` is a predeclared, callable
intrinsic that appends CRLF. It prints up to 17 significant digits with
trailing zeros removed:

| Input | Output |
| --- | --- |
| `2.5` | `2.5` |
| `1.0` | `1` |
| `1.0 / 3.0` | `0.33333333333333331` |
| `0.1 + 0.2` | `0.30000000000000004` |
| `1e20` | `1e+20` |
| `1.0 / 0.0` | `Inf` |
| `-1.0 / 0.0` | `-Inf` |
| `0.0 / 0.0` | `-NaN` |
| `-0.0` | `-0` |

`rt_str_from_float(Float) -> Str` is also public and returns an owned string.
`rt_print_char(Char)` appends CRLF.

**Confidence:** high for the observed values; the exact dtoa algorithm was not
re-derived.

**Documentation status:** missing from public docs. Internal documentation is
inconsistent: `src/backend/ir.rs` says "up to six fractional digits", while
`src/runtime/intrinsics.rs` and the observed 17-digit output describe the
exact expansion.

**Classification:** VERIFIED + DOCUMENTATION GAP; internal DOCUMENTATION
CONTRADICTION deferred (source comment, outside this patch).

**Action:** documented the verified public behavior in
`LANGUAGE_BASICS.md` section 2. The internal comment is recorded for a future
source-comment cleanup, not changed here.

#### Finding G - standard-library resolution

**Original observation:** installed compiler passed 15/15 example
compilations; repository `target/release/mink.exe` passed 13/15, with
`10_imports` and `program_c_modular_utility` failing
`E-S01 module file 'filesystem.mink' not found`.

**Sources inspected:** `src/driver.rs` (`resolve_module_path` search order:
sibling file, sibling directory package, `<exe>/../stdlib`, `<cwd>/stdlib`,
project package roots), `src/package/mod.rs` (`module_roots`).

**Tests/evidence:**

- Verifier in disposable work dirs with the repository compiler: **13/15**.
- Same verifier with the installed npm compiler: **15/15**.
- Repository compiler run from the repository root on a source under `tmp/`:
  **build passed** (cwd-relative `stdlib/` found).
- Installed compiler run from an unrelated temp directory: **build passed**
  (`<package>/stdlib` found through the executable path).

**Root cause:** the repository binary lives in `target/release/`, so
`<exe>/../stdlib` is `target/stdlib`, which does not exist. Its remaining
repository option is `<cwd>/stdlib`. The npm layout puts `stdlib/` beside the
package root, so `<exe>/../stdlib` resolves. This is the documented
development-checkout layout, not a compiler defect.

**Confidence:** high; controlled working directories isolate the variable.

**Documentation status:** the README says the bundled library works in any
project without configuration, which is true for the installed package but not
for the repository binary outside the repository root.

**Classification:** VERIFIED (expected behavior) + DOCUMENTATION GAP.

**Action:** `LANGUAGE_BASICS.md` section 7. No compiler change.

### C.4 Broader documentation audit

1. All five complete MINK fences in `README.md` compile (5/5). Successful
   compilation does not prove runtime behavior, so only examples with
   exercised output were treated as verified.
2. All 15 `examples/*/main.mink` programs build with the repository compiler
   from a temporary copy (15/15). They were not executed in this session.
3. `README.md` line 452 says standard-library files are "imported with `use`".
   A `use` declaration does not load a file. This is a real contradiction; it
   was not edited because `README.md` has pre-existing uncommitted changes.
4. The README type list omits `Vec<T>`, `Map<K, V>` and `Set<T>`, although
   they are built-in types. Deferred with the README edit.
5. `CORE_LANGUAGE.md` section 26 said bindings have no annotations and there
   is no borrow syntax. Those statements describe Session 06; later sessions
   added annotations and references. A historical-scope note was added.
6. `use` without `mod` is silently ignored during import resolution and then
   fails at the call site with `E-S01 cannot find name`. Documented.
7. Qualified module calls are unsupported, but the diagnostic is
   `E-T15 ... is not a valid key type: cannot find type helper`, which points
   at the wrong concept. Documented as unsupported; diagnostic left unchanged
   as an implementation follow-up.
8. Session 176's exercise 10 failure path calls
   `rt_print_str("write-failed\n")`. Because print appends CRLF, that path
   would emit an extra blank line. The passing path is unaffected and the
   original evidence was not altered.
9. The Session 176 README's default verifier command reports 13/15 with the
   repository compiler because every case is copied to a disposable directory
   and the repository binary does not resolve `<cwd>/stdlib` from there. The
   installed-compiler command reports 15/15. This is Finding G, not a test
   regression.
10. No developer-facing standard-library API index exists. `stdlib/*.mink`
    comments are the practical reference. This remains Session 178 scope.

### C.5 Beginner learning-path audit

The ten Session 176 exercises and three programs were reviewed without
changing them.

- The exercises are minimal, each isolates one construct, and every expected
  output is reproducible: 15/15 with the installed compiler.
- Prerequisites are under-explained: the exercises assume the compiler's
  standard library is resolvable and do not say that the repository compiler
  must run from the repository root. The README command therefore looks like a
  false failure.
- Exercise 09 teaches map creation, insertion, lookup and free, but keeps all
  map work in `main`; it does not teach the now-verified `Map<K, V>` parameter
  or return type. A corrected follow-up belongs with Session 178's API work.
- Exercise 10 and Program C exercise filesystem failure returns, but the
  expected `-1`/empty-string behavior is never explained; the learner sees it
  only in the verifier.
- Program B's scalar helper functions are clear. Its map workaround is no
  longer technically required.
- The `E-T01 Map<unresolved, ...>` diagnostic is understandable only after
  the developer knows that annotations exist. The guide now explains it.
- No original Session 176 file was modified.

## D. Evidence

### D.1 Source files inspected

- `src/driver.rs` (module discovery and resolution order)
- `src/package/mod.rs` (`module_roots`)
- `src/semantics/analyzer.rs` (`resolve_imports`)
- `src/parser/mod.rs` (`parse_type`, `parse_use_decl`)
- `src/typecheck/checker.rs` (`Map`/`Set` resolution, key restrictions)
- `src/typecheck/ty.rs` (`TypeKind::Map`)
- `src/ownership/mod.rs` (collection borrow/consume rules)
- `src/runtime/intrinsics.rs` (intrinsic table and signatures)
- `src/backend/ir.rs` (runtime service contracts)
- `src/backend/lower.rs` (intrinsic routing)
- `src/backend/emit/runtime.rs` (print, exit/leak, filesystem services)
- `stdlib/filesystem.mink` (wrapper contracts)
- `docs/implementation/OWNERSHIP_IMPLEMENTATION.md`
- `docs/implementation/SCALAR_TYPES_IMPLEMENTATION.md`
- `docs/language/CORE_GRAMMAR.md`, `CORE_LANGUAGE.md`, `TYPE_SYSTEM.md`,
  `ERROR_SYSTEM.md`
- `README.md`

### D.2 Tests and verifiers

- `benchmarks/m6/session176/verify.py`: repository compiler **13/15**
  (exit 1, two module failures); installed compiler **15/15** (exit 0).
- `benchmarks/m6/session177/verify.py`: repository compiler **11/11 cases,
  8/8 guide fences** (exit 0); installed compiler **11/11, 8/8** (exit 0).
- README MINK fences: **5/5** build.
- `examples/*/main.mink`: **15/15** build in a temporary copy.
- Markdown local links in `docs/language/`: **none broken**.
- `git diff --check`: **exit 0**.

### D.3 Exact commands

```bash
git rev-parse HEAD
git status --short
git rev-list --left-right --count HEAD...refs/remotes/origin/main
./target/release/mink.exe --version
./tmp/m6_install_175/node_modules/@p4inz-code/mink/bin/mink.exe --version
python benchmarks/m6/session176/verify.py
python benchmarks/m6/session176/verify.py \
  --compiler tmp/m6_install_175/node_modules/@p4inz-code/mink/bin/mink.exe
python benchmarks/m6/session177/verify.py
python benchmarks/m6/session177/verify.py \
  --compiler tmp/m6_install_175/node_modules/@p4inz-code/mink/bin/mink.exe
./target/release/mink.exe build <probe>.mink
./target/release/mink.exe run hello.mink
./target/release/mink.exe check hello.mink
git diff --check
```

Individual probes were created under the ignored `tmp/s177/` tree and are
reproducible from the source snippets above.

### D.4 Limitations

- No external developer; this is an internal audit.
- Human learning time was not measured.
- Runtime behavior was verified on Windows x86_64 only.
- The exact dtoa algorithm was not re-derived; only observed output is
  documented.
- `fs_dir_next` ownership was taken from the wrapper contract, not separately
  exercised in this session.
- The full Rust test suite was not run because no compiler, runtime or
  standard-library source changed.

## E. Changes

| File | Change |
| --- | --- |
| `docs/language/LANGUAGE_BASICS.md` | New developer reference: build/run, line-oriented output and float formatting, collections and `Map<K,V>` crossing functions, module semantics and lookup, ownership/cleanup with leak behavior, filesystem return conventions, compiler/stdlib resolution, diagnostics |
| `docs/language/CORE_GRAMMAR.md` | Corrected sections 28-30: `mod` loads and exposes items unqualified in the current implementation; `use` does not load; qualified expression paths unsupported; `pub` not enforced |
| `docs/language/CORE_LANGUAGE.md` | Added a historical-scope note to section 26 so its Session 06 statements are not read as MINK 1.0.4 behavior |
| `benchmarks/m6/session177/verify.py` | New verifier for the documented behavior (11 cases) and compilation of every complete MINK fence in the guide (8 fences) |
| `docs/benchmarks/M6/M6_SESSION_177_DOCUMENTATION_AND_LANGUAGE_LEARNING_AUDIT.md` | This record |
| `docs/benchmarks/M6/M6_DEFERRED_SESSIONS.md` | Session 177 marked COMPLETE; Session 178 starting point |

Intentionally unchanged:

- `README.md` (pre-existing uncommitted edits; contradictions recorded as
  deferred).
- All compiler, runtime, standard-library, Linux-frozen, npm and version files.
- Session 176 evidence and exercises.
- `docs/language/TYPE_SYSTEM.md` and `ERROR_SYSTEM.md` (design-level documents;
  no verified contradiction within their stated scope).
- `src/backend/ir.rs`'s stale PrintFloat comment (source comment, deferred).

## F. Verification

| Check | Result |
| --- | --- |
| Session 177 verifier, repository compiler | **11/11 cases, 8/8 guide fences**, exit 0 |
| Session 177 verifier, installed compiler | **11/11 cases, 8/8 guide fences**, exit 0 |
| Session 176 verifier, repository compiler | 13/15, exit 1 (Finding G, expected) |
| Session 176 verifier, installed compiler | **15/15**, exit 0 |
| README MINK fences | 5/5 build |
| Examples compile sweep | 15/15 build |
| Markdown links in `docs/language/` | No broken local links |
| Code fences in edited/new docs | Balanced |
| `git diff --check` | Exit 0 |
| New guide ASCII | Yes |
| Compiler/runtime/Linux changes | None |
| Pre-existing worktree entries | Preserved |

Adversarial review found and fixed one defect in this session's own work: the
first composition example called `rt_str_concat("total=", rt_str_from_int(55))`
and freed only the result. The verifier caught the leaked intermediate string
at exit (`E-R06`, code 106). The example and its test now bind, print, and free
both owned strings. This is recorded rather than hidden because it is exactly
the class of ownership error the guide warns about.

The review also confirmed:

- Every new behavioral claim has a source path or an executed probe.
- Every complete code fence in the guide compiles with both compilers.
- The guide distinguishes public intrinsics from implementation detail and
  flags the collection use-after-free limitation.
- The module instructions match the corrected grammar and the probes.
- No pre-existing file was staged or reverted.
- No source change slipped into the patch.

## G. Remaining issues

1. **Collection use-after-free is not diagnosed.** `rt_map_free(m);
   rt_map_get(m, 1);` compiles and returns a stale value. This is an ownership
   checker defect, not a documentation defect. It is documented as a 1.0.4
   limitation and deferred to an isolated ownership session.
2. **README contradictions remain.** Line 452 says stdlib is imported with
   `use`; the type list omits collections. `README.md` had substantive
   uncommitted edits at baseline, so it was not touched. A follow-up should
   correct it once the pre-existing changes are committed or otherwise
   resolved.
3. **Internal float comment is stale.** `src/backend/ir.rs` says "up to six
   fractional digits"; the intrinsic prints up to 17 significant digits. This
   is a source comment, not public documentation; deferred.
4. **Qualified module-call diagnostic is misleading.** The unsupported form
   `helper::double(4)` reports `E-T15 ... not a valid key type`. Behavior is
   documented as unsupported; the diagnostic is an implementation follow-up.
5. **No standard-library API index.** Discoverability is still source-comment
   based; this is the Session 178 mission.
6. **Session 176 verifier command is misleading with the repository compiler.**
   Its README does not explain the cwd/stdlib dependency. The original evidence
   was preserved; Session 178 or a documentation follow-up should explain it.
7. **Session 176 map workaround is obsolete.** The exercises do not yet teach
   `Map<K, V>` parameters and return types. A corrected follow-up exercise
   belongs with Session 178.
8. **`Map<unresolved, ...>` wording is inference-dependent.** Verified forms
   include `Map<unresolved, Int>`; the guide says "a map component could not be
   inferred" rather than promising an exact two-parameter spelling.

## H. Time accounting

- Earliest probe artifact: 2026-11-05 (Unix `1793867696`).
- Final verification pass: Unix `1793868512`.
- Measured artifact window: **816 seconds (about 14 minutes)**.
- Baseline, documentation reading and reasoning before the first probe were not
  separately instrumented.
- The 60-minute session target was **not met** by the measured window. No idle
  time or artificial work was added to inflate it. All acceptance-gate work
  items were completed and verified.

## I. Next action

**Session 178 - Standard Library Discoverability and Practical API Audit.**

Start from:

- `docs/language/LANGUAGE_BASICS.md` (especially section 6 and section 7).
- `M6_SESSION_176_FIRST_PROGRAM_AND_LANGUAGE_LEARNING.md` and
  `M6_SESSION_177_DOCUMENTATION_AND_LANGUAGE_LEARNING_AUDIT.md`.
- `stdlib/*.mink`, especially `filesystem.mink`, `strings.mink`,
  `collections.mink`.

Priorities:

1. Inventory every public standard-library function and its ownership/return
   contract; produce one authoritative discoverability index.
2. Resolve the Session 176 exercise 10 verifier-command ambiguity and the
   obsolete map workaround with corrected follow-up exercises.
3. Correct `README.md` line 452 and the type list once its pre-existing
   worktree changes are resolved.
4. Keep the collection use-after-free defect and the qualified-path diagnostic
   as isolated implementation follow-ups; do not mix them into documentation.
