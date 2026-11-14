# Session 279 — M7 Phase C: access control, constraints design, diagnostics, native codegen

**Date:** 2026-11-14
**Programme:** M7 — Compiler Maturity & Language Completeness, Phase C
**Starting HEAD:** `5ee91babc1f9d3d40ce9f86033f0f56575a6029b` (equal to `origin/main`)
**Branch:** `main`
**Status:** Phase C complete — **PASS**

Gates A–D were executed against `5ee91ba`: visibility/access control (A),
traits and constraints (B), machine-readable diagnostics (C), and native
codegen/ABI hardening (D), plus cross-gate integration. Three confirmed defects
were fixed at the root cause, each with a permanent conformance corpus. Gate D
produced no defect; it produced evidence. No language semantics were invented,
no partial trait system was built, and no version, tag, or publication changed.

---

## 1. Repository verification

| Item | Result |
| --- | --- |
| Starting HEAD | `5ee91ba` — matched the reported HEAD and `origin/main` exactly |
| Branch | `main` |
| Worktree | clean apart from four untracked user files, preserved untouched: `MINK UI SETS OFFICIAL.zip`, `freebuff-chat-2026-10-01T19-08-00.600Z.md`, `nl1.exe`, `nl1.mink` |
| Package version | 1.0.5 (`Cargo.toml`), `mink --version` → `mink 1.0.5` — unchanged |
| Dependencies | 0 (the JSON schema is hand-rolled on purpose; the crate stays dependency-free) |
| Baseline regression, re-measured | `cargo test --all-targets`: **104 result blocks, 3,529 passed, 0 failed, 9 ignored**, exit 0 (`tmp/s279/baseline_debug.log`) |

Nothing was reset, cleaned, stashed, or overwritten. No `git add -A`, no
force-push, no history rewrite.

## 2. Gate A — visibility and access control

### 2.1 Audit

The documented model is: file modules loaded by `mod name;`, one flattened
namespace, `use module::item;` as a name-bringing alias, and `pub` parsed but
not enforced (`CORE_GRAMMAR.md` §28–§30,
`MODULE_SYSTEM_IMPLEMENTATION.md`).

| # | Probe | Observed before | Kind | Disposition |
| --- | --- | --- | --- | --- |
| A1 | `use helper::nope;` (item does not exist) | accepted; nothing checked the import | **Defect** | Fixed: `E-S01` at the import |
| A2 | `use nowhere::thing;` (module never declared) | accepted | **Defect** | Fixed |
| A3 | `use helper::open::extra;` (3 segments) | accepted, third segment ignored | **Defect** | Fixed |
| A4 | `pub mod helper;` | the module file was **never loaded**; the call failed with `cannot find name \`helper_ok\`` | **Defect** | Fixed: `pub mod` behaves as `mod` |
| A5 | `pub use helper::nope;` | accepted | **Defect** (same root cause as A1) | Fixed: validated like a plain import |
| A6 | `mod gone;` with no file | reported at `main.mink:1:1` (synthetic empty span) | **Defect** (misattribution) | Fixed: reported at the `mod` name |
| A7 | private `fn`/`struct` used across modules | allowed | **Design decision** (documented) | Pinned by `A-V3`; no change |
| A8 | `pub fn`/`pub struct`/`pub const` across modules | allowed | Works as documented | Pinned by `A-V2` |
| A9 | struct field `pub x: Int` | `E-P02` | Correct — fields have no visibility | Pinned by `A-V7` |
| A10 | same name in two modules | `E-S02 duplicate definition` | Correct — one flattened namespace | Pinned by `A-V6` |
| A11 | `mod a; mod a;` | accepted, loaded once | **LOW** (undocumented acceptance) | Documented, not changed: the duplicate is harmless |
| A12 | `pub` enforcement itself | not enforced | **Design decision**, not a bug | Documented with the reason (below) |

**Why A7/A12 are a design decision, not a defect.** The shipped standard
library declares **0 `pub fn`** across 27 modules (686 top-level `fn`
declarations), while every documented public API is reached through `use`.
Enforcing `pub` today would reject the documented standard-library usage and
the language as it is taught. Enforcement needs a visibility model with
re-exports, per-module name spaces (the language has one flattened namespace),
and a stdlib migration to `pub` — a milestone, not a phase fix. This is recorded
in `CORE_GRAMMAR.md` §30 so the next reader does not re-open it as a bug.

### 2.2 Root cause and fix

`discover_modules_recursive` collected `mod` declarations into the module list
but the entire import-validation path was dead: `ModuleRegistry`,
`analyze_with_registry`, and `resolve_imports` had **zero call sites** in `src/`
and `tests/`, and flattening stripped every `use` before semantics ever ran. The
fix adds `validate_imports(&[ModuleSource])`, called before flattening in
`check_multi_module`, and two structural helpers:

- `item_kind_through_pub` — `pub` neither declares nor loads anything, so
  discovery and validation look through a `Pub` wrapper (this is what made
  `pub mod` skip its file and `pub use` escape validation);
- `flattened_item` — one rule for both root and child modules: drop `mod`/`use`
  (and their `pub` forms), keep everything else without the `pub` qualifier.
  Before, the root kept its `Pub` wrappers while children were unwrapped, so the
  same declaration reached later stages in two shapes.

Missing module files now carry the span of the `mod` name that requested them
(`discover_modules_recursive` takes `Option<(String, Span)>`) instead of a
synthetic `SourceId(0)` span that rendered as `main.mink:1:1`.

### 2.3 Regression corpus

`tests/access_control.rs` — **26 cases**, `A-V1`…`A-V7`, covering same-module
private access, cross-module public and private access (pinned, deliberately),
import validation (item, module, three segments, `pub use`, no-op forms), the
missing-module-file location, nested modules, `pub mod`, duplicate names across
modules with both files named, field-visibility rejection, and an executed
cross-module call. `tests/phase_c_cross_gate.rs` (`X-*`) adds the combinations
with Gates C and D.

## 3. Gate B — traits and constraints

The gate asked for a design, not an implementation, and no trait, bound, `impl`,
or `dyn` syntax was added. The design record is
[`TRAITS_AND_CONSTRAINTS_DESIGN.md`](TRAITS_AND_CONSTRAINTS_DESIGN.md).

### 3.1 Audit findings

| # | Finding | Kind | Disposition |
| --- | --- | --- | --- |
| B1 | A generic call whose type arguments could not be inferred (`let p = P{..}; identity(p)`) was reported as `cannot find name \`identity\`` — a false statement, with no hint about the fix | **Defect** (misleading diagnostic) | Fixed: `E-T44` at the call |
| B2 | No trait/constraint syntax; `trait`/`impl` are reserved and rejected by the parser; `<T: Bound>` is a parse error | **Design decision** — correctly not implemented | Documented (`TYPE_SYSTEM.md` §12–§13, grammar) |
| B3 | Generic bodies are checked per instantiation, not against an abstract `T` (`addg(true,false)` → `E-T02`; `getx::<P>(p)` with `p.x` is accepted) | Soundness property, verified | Pinned by `B-CHECK` |
| B4 | An uninstantiated generic body is never checked (`fn f<T>(x:T){ x.y }` passes if never called) | **Known limitation** (not unsound: the body is uncallable) | Documented and pinned by `B-GAP` |
| B5 | Type-argument inference covers literals, typed locals, and calls with declared return types only | Documented limitation | Constraint: the diagnostic must say so (B1) |

### 3.2 Fix and mechanism

The monomorphizer had a comment promising that "the driver then reports it as an
uninferable generic call instead of a misleading unresolved name" — no such
driver path existed. `Monomorphizer` now records `(name, callee span)` for every
generic call it cannot instantiate; `monomorphize_reported` returns them; both
pipelines turn each into `TypeError::cannot_infer_type_arguments` (`E-T44`), and
the duplicate `UnresolvedName` error at that exact span is suppressed
(`explained_by_inference`) so one mistake produces one diagnostic instead of two
contradictory ones. Message:

```
error[E-T44]: cannot infer the type arguments of `identity` from this call;
              write `identity::<Type>(...)` or pass a literal or typed variable
  --> main.mink:3:41
```

### 3.3 Regression corpus

`tests/constraints_conformance.rs` — **15 cases**: inference failure (E-T44, no
E-S01), typed-local and literal inference, explicit type arguments, per-
instantiation checking including member access, mismatched instantiations,
generic struct/enum instantiation, the uninstantiated-body gap, and the four
unsupported-syntax cases. `tests/phase_c_cross_gate.rs` covers a generic
function in a module called from the root.

## 4. Gate C — machine-readable diagnostics

### 4.1 Audit findings

| # | Finding | Kind | Disposition |
| --- | --- | --- | --- |
| C1 | Every diagnostic was rendered against the **root** source file: `helper.mink:2:19` printed as `main2.mink:3:12`, in human and JSON output alike | **Defect** (misattribution) | Fixed: each span resolves its own `SourceId` |
| C2 | `mink check --json` printed **nothing** on stdout for failures that are not front-end diagnostics (unreadable file, directory argument, backend rejection) — only prose on stderr | **Defect** (contract gap) | Fixed: JSON with `"code": null`, real `stage` |
| C3 | `files_checked` counted the root only; `files` did not exist | **Defect** (inaccurate field) | Fixed: `sources.len()` and a `files` array |
| C4 | No `schema_version`, no `start_byte`/`end_byte`, related locations carried no file | Contract gap | Fixed: schema version 1 (superset of the old schema; nothing renamed or removed) |
| C5 | Diagnostic ordering was stable only by accident | Risk | Fixed: `sort_errors` sorts by `(file id, start byte, code)` |
| C6 | A module file that cannot be read reported the root at 1:1 | **Defect** | Fixed (shared with A6) |
| C7 | Usage errors (`mink check` with no path) stay human | Limitation | Documented: the arguments are never parsed far enough to know JSON was wanted |
| C8 | `suggestions` and `warnings` are always empty | Not implemented | Documented as present-for-compatibility, not as computed |

### 4.2 The contract

`docs/compiler/DIAGNOSTICS.md` documents schema version 1 field by field:
`schema_version`, `success`, `exit_code`, `files_checked`, `files`,
`token_count`, `errors[]` (`code`, `severity`, `stage`, `message`, `span`,
`related[]`, `suggestions[]`), and `warnings[]`; stage names (`lexing`,
`parsing`, `semantics`, `typecheck`, `hir`, `mir`, `backend`, `driver`); 1-based
line/byte-column and 0-based half-open byte offsets; the path form; determinism;
and the limitations above. The human renderer and the JSON renderer read the
same `CheckReport`, as `ERROR_SYSTEM.md` §25 requires.

### 4.3 Regression corpus

`tests/diagnostics_json.rs` — **14 cases**: success shape, stage and code for
syntax/semantic failures, byte-offset slicing against the real source text,
multi-module attribution (human and JSON), two errors in one run, byte-identical
determinism, missing file, directory argument, spaces and non-ASCII paths,
`files_checked == 2`, and unchanged human output. The file contains a small JSON
validator so "machine-readable" is checked against the grammar instead of a
substring search (the crate has no dependencies). `tests/cli.rs`'s determinism
test now substitutes the two source paths, because schema version 1 reports
which file was checked; everything else in that comparison is still byte-exact.

## 5. Gate D — native codegen and ABI hardening

### 5.1 Audit

The backend defines its own convention (stack arguments at `[rbp+16+8k]`, result
in `rax`, 16-byte alignment at every call, `rbp`/`rsp` callee-saved). The only
platform-ABI boundary is `kernel32` (with explicit 32-byte shadow space at every
imported call: `GetStdHandle`, `WriteFile`, `Sleep`, filesystem, process,
threads) and the PE entry stub. Session 278 had already added page probing for
frames larger than 4 KiB.

| # | Probe | Result |
| --- | --- | --- |
| D1 | 12 and 20 parameters | correct sums (`78`, `120`) |
| D2 | mixed `Int`/`Bool` parameters | correct slots |
| D3 | `Float` argument and return | `2.5` round-trips |
| D4 | 64 KiB frame in `main`, 160 KiB frame in a callee | run (probing holds beyond the entry function) |
| D5 | 16 KiB frame with `defer` | block runs at scope exit, value correct |
| D6 | 200-level recursion with 512-`Int` frames | stack overflow (see D8) |
| D7 | 1000-level recursion | `1000` |
| D8 | struct return (3 fields; array field) | all fields intact |
| D9 | 50 iterations of 3- and 5-argument calls followed by a runtime call | correct total (`3100`) — an alignment fault would crash |
| D10 | nested calls keeping a live frame | no callee-saved clobber |
| D11 | entry stub: `Int` result, `Bool` result, unit `main` | exit 7, 1/0, 0 after printing |
| D12 | recursion beyond the 1 MiB stack reserve | process dies with the Windows stack-overflow exception (`0xC00000FD`, shell exit 253) |

**No defect was found.** D6/D12 are OS behaviour, not a compiler defect: the PE
header requests the Windows-default 1 MiB reserve with a 4 KiB commit
(`emit/pe.rs`), and the depth limit is reserve ÷ frame size. There is no
compiler-inserted depth check and no handler that would turn the exception into
a diagnostic; that is documented rather than claimed as handled. The Gate D
cases were written to stay inside the reserve, and the 200-level/4 KiB variant is
deliberately *not* a test, because it would encode an exception code as a
language guarantee.

### 5.2 Regression corpus

`tests/abi_conformance.rs` — **16 executed cases** (D-ARGS, D-FRAME, D-ENTRY).
They run real binaries because the properties under test (alignment, guard-page
probing, slot arithmetic, register preservation) corrupt the process instead of
producing a diagnostic.

## 6. Cross-gate integration

`tests/phase_c_cross_gate.rs` — **7 cases**: an import error in JSON keeps the
import's span; a cross-module duplicate names both files, including the related
location's own file; a generic function in a module runs from the root through
the backend; an `E-T44` from a module file is attributed to that module and
carries `stage: typecheck`; inference failure is never doubled with `E-S01`;
`pub mod` reports `files_checked: 2` and runs; a missing module file's JSON span
is the `mod` name at line 1 column 5.

## 7. Verification

| Check | Result |
| --- | --- |
| Debug regression, `cargo test --all-targets` | **109 result blocks, 3,608 passed, 0 failed, 9 ignored**, exit 0 (`tmp/s279/after_debug.log`) |
| Release regression, `cargo test --release --all-targets` | **109 result blocks, 3,608 passed, 0 failed, 9 ignored**, exit 0 (`tmp/s279/after_release.log`) |
| Delta over baseline | +79 tests (78 new integration cases + 1 renderer unit test), 0 failures |
| `cargo fmt --check` on changed files | clean. Pre-existing diffs in `src/backend/emit/{runtime,linux_runtime}.rs` and `src/runtime/{abi,allocator}.rs` were left untouched: they are not this milestone's code and reformatting them would bury the real diff |
| `cargo clippy --all-targets` | 0 errors; no warning in any changed file (checked by path), so no new lint debt was introduced |
| Release acceptance, `python tools/release_acceptance.py --session 279 --label m7-phase-c` | **PASS** — build 29.9 s, test-compile 14.7 s, regression 117.1 s, 99 suites, 3,608 passed, 0 failed, 9 ignored; artifact 4,330,496 bytes, SHA-256 `ed08e351…` (run `20261114T134742Z-00518eb-m7-phase-c`) |
| Worktree at acceptance | clean except the four preserved untracked user files (reported as a limitation by the runner, by design) |

No test was weakened. The single modified existing test
(`tests/cli.rs::check_json_deterministic`) asserts the same property as before —
identical output for identical input — after normalising the two files' paths,
which schema version 1 legitimately reports.

## 8. Documentation

| Document | Change |
| --- | --- |
| `docs/compiler/DIAGNOSTICS.md` | **New**: the schema-1 contract, stages, span semantics, attribution, determinism, limitations |
| `docs/implementation/TRAITS_AND_CONSTRAINTS_DESIGN.md` | **New**: Gate B design record, options, decision, preconditions for revisiting |
| `docs/language/CORE_GRAMMAR.md` §29–§30 | Import validation; what `pub` does and why enforcement is deferred (including the 0-`pub fn` stdlib fact) |
| `docs/language/TYPE_SYSTEM.md` §12–§13 | Implemented status, `E-T44`, the per-instantiation checking model, the deferral |
| `docs/language/ERROR_SYSTEM.md` §25 | Points at the contract; `null` code/span rule for non-diagnostic failures |
| `docs/implementation/MODULE_SYSTEM_IMPLEMENTATION.md` | Updated limitations: design-decision `pub`, `pub use`, duplicate `mod`, synthetic spans |
| `docs/implementation/NATIVE_BACKEND_IMPLEMENTATION.md` | Gate D conformance section and the 1 MiB stack-reserve behaviour |
| `docs/roadmap/IMPLEMENTATION_ROADMAP.md` | Dated Session 279 status entry |
| `src/diagnostics/mod.rs`, `src/driver.rs`, `src/monomorphize/mod.rs`, `src/cli.rs` | Module docs for the schema, import validation, inference reporting, and per-span attribution |

## 9. Limits and remaining gaps

Language-level, deliberate:

- `pub` is not enforced, and enforcing it would break the standard library as
  shipped (§2.1). Visibility and constraints should be designed together.
- No traits, bounds, `impl`, or dynamic dispatch (§3). Design recorded, not
  implemented.
- One flattened namespace: modules do not isolate names; duplicates are `E-S02`.
- Uninstantiated generic bodies are never checked; a generic function cannot use
  an operation that only some instantiations support unless every instantiation
  does.
- Type-argument inference is literal/typed-local only; anything else needs
  `::<Type>` (`E-T44` says so).

Diagnostics-level, deliberate:

- Usage errors stay human; no suggestions; no warnings; byte columns rather than
  character columns; no SARIF or LSP output.
- `validate_imports` checks existence, not visibility, because visibility is not
  a rule that exists.

Process-level:

- Stack exhaustion is OS behaviour, not a diagnostic (§5).
- Pre-existing `cargo fmt` drift in four untouched files was not fixed. It is
  recorded here rather than silently reformatted.

## 10. Release safety

No npm publication, no version bump (1.0.5 before and after), no npm credential
change, no release tag, no GitHub release, no public binary distribution, no
Linux work, no M6 or M8 work. The npm bundle was not rebuilt because nothing in
its published contract changed and this milestone explicitly excludes
publication.

## 11. Verdict

**M7 Phase C: PASS.**

- Gate A: one defect class (unvalidated imports, plus `pub mod` not loading its
  file and `pub use` escaping validation) — **fixed**, 26-case corpus.
- Gate B: one defect (misleading unresolved-name diagnostic for uninferable
  generic calls) — **fixed** as `E-T44`; traits/constraints correctly **not
  implemented**, design recorded, 15-case corpus.
- Gate C: one defect class (all diagnostics attributed to the root file; JSON
  missing for non-diagnostic failures) — **fixed**, schema version 1 documented,
  14-case corpus.
- Gate D: **no defect found**; convention, probing, aggregate returns,
  alignment, and entry stub confirmed by 16 executed cases; limits documented.
- Cross-gate: 7 cases green.
- 3,608 tests pass in debug and release; release acceptance **PASS**.
