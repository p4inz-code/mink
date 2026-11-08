# Session 217 — Documentation Version and Claim Consistency

Windows x86_64, 2026-11-08. Starting commit
`28886a21d5999a4f9ea69f02c8090b7c18988f81` (Session 216, local `main` ==
`origin/main` verified before work began). Worktree clean except the four
pre-existing untracked user files, which were not staged or modified.

## 1. Scope

Audit active documentation for version drift and contradictory product claims,
with particular attention to material that described the repository build as
1.0.4 when the manifests and compiler are 1.0.5. The session was documentation
only: no compiler, runtime, standard-library, test or package behavior was
changed.

Documents inspected: `README.md`, the eight pages in `docs/language/`, the
canonical roadmap, the brand page, the validation package, and the release and
audit pages that carry current-version statements.

## 2. Classification of version references

| Class | Examples | Finding |
| --- | --- | --- |
| Current version claim | README badge and install-state note; `Cargo.toml` 1.0.5; `npm/mink/package.json` 1.0.5; roadmap dated statuses; readiness audit addendum | Correct for 1.0.5 and the unpublished state |
| Historical release reference | README "MINK 1.0.4" section; `docs/implementation/SESSION_*`; M4/M5 records; `VALIDATION_MILESTONE_1_*` baseline `dd556a6`; M6 Sessions 184-196 | Accurate at their recorded time; preserved verbatim |
| Registry availability statement | README install note; `WINDOWS_INSTALLATION.md` install commands; M6 package §1/§3/§9; roadmap §8 | Correct: npm `latest` is 1.0.4, published 2026-09-30; 1.0.5 is unpublished |
| Explicit compatibility statement | `LANGUAGE_BASICS.md` "In 1.0.4..." ownership and `pub` statements; `STANDARD_LIBRARY_REFERENCE.md` audit table | Version-specific statements retained; current build named separately |
| Stale or ambiguous claim | Seven active pages listed in §3 | Corrected with dated wording, without deleting audit history |

The eight `docs/language/` pages were checked individually. `CORE_GRAMMAR.md`,
`TYPE_SYSTEM.md`, `CORE_LANGUAGE.md` and `ERROR_SYSTEM.md` carry
`Version: 0.1.0`, which is the specification revision from their originating
sessions, not a product-version claim; they were left unchanged.

## 3. Corrections and reasons

| File | Correction | Reason |
| --- | --- | --- |
| `docs/roadmap/IMPLEMENTATION_ROADMAP.md` | "Baseline" relabelled "Baseline at lock" and one sentence added that the current build is 1.0.5, unpublished | The locked Session 128 baseline is historically 1.0.4; the unqualified label read as the current baseline |
| `docs/language/WINDOWS_INSTALLATION.md` | Audit header now says "then-current 1.0.4 repository build"; added current-build 1.0.5 note; §7 manifest corrected from 1.0.4 to 1.0.5; added the Session 217 registry-install result | §7 falsely identified the current manifest; the install examples intentionally pin registry 1.0.4, which is still correct |
| `docs/language/DEBUGGING_AND_TESTING.md` | "Applies to" now names 1.0.5 repository build and 1.0.4 published package; repository version command and comparison comments updated; audit-time 1.0.2 facts retained and labelled | The page read as if the current repository build and installed package were 1.0.4 and 1.0.2; the recorded audit evidence remains 1.0.4/1.0.2 |
| `docs/language/STANDARD_LIBRARY_REFERENCE.md` | "Applies to" now names 1.0.5 current and 1.0.4 published; §1 table labelled "Version at audit (2026-11-05)"; added current-state note; command comments updated | Table values were correct for the audit but were presented as current; module contracts were not re-executed for 1.0.5 and are not claimed to have been |
| `docs/language/LANGUAGE_BASICS.md` | "Applies to" now names 1.0.5 current and 1.0.4 published, with the 1.0.4 audit baseline stated | Header falsely identified the current build; explicit 1.0.4 statements in the body remain version-specific |
| `docs/brand/BRAND_AND_WINDOWS_INTEGRATION.md` | "Version: 1.0.4 (unchanged)" clarified as "1.0.4 at implementation (unchanged by the 1.0.5 candidate)" | The feature landed in 1.0.4; the unqualified version read as the current product version |
| `docs/validation/M6_EXTERNAL/README.md` | Reading note now says the references were audited against 1.0.4, name 1.0.5 as current, and should be verified against 1.0.5 | The previous note referred to pages saying "Applies to 1.0.4", which is no longer true after the header corrections |
| `docs/validation/M6_EXTERNAL/EVALUATION_PROTOCOL.md` | Added the arithmetic behind the suggested effort: 18 time boxes sum to about 7.5 h, the required minimum set to about 3 h | The 4-6 h estimate and the stated time boxes were not self-explanatory; the protocol now states which subset fits the estimate |

No historical release statement was rewritten. `README.md` already stated the
registry serves 1.0.4, that 1.0.5 is unpublished, and that the current repository
build prints `mink 1.0.5`; it required no correction.

## 4. Verification performed

Commands were run from the repository root in Git Bash. Exit codes are the
commands' own.

| Check | Result |
| --- | --- |
| `grep '^version' Cargo.toml` | `1.0.5` |
| `grep '"version"' npm/mink/package.json` | `1.0.5` |
| `target/release/mink.exe --version` | `mink 1.0.5` (4,048,384 bytes; SHA-256 `8b3b2846...eccf4ee`) |
| `target/s213_clean/release/mink.exe --version` | `mink 1.0.5` (4,048,384 bytes; SHA-256 `87a1cae4...b6da23`, matching the package record) |
| `npm/mink/bin/mink.exe --version` | `mink 1.0.5` (4,028,416 bytes; SHA-256 `c288115d...479401`) |
| `mink --version` on `PATH` | `mink 1.0.2` (older unsupported install on this machine) |
| `npm view @p4inz-code/mink version` / `dist-tags.latest` | `1.0.4` / `1.0.4` |
| `npm install --prefix tmp/s217_prefix "@p4inz-code/mink@1.0.4"` | exit 0, `added 1 package`; 28 files, 26 `stdlib/` modules, no `windows/`; binary printed `mink 1.0.4` |
| Repository `stdlib` module count | 27 `.mink` files |
| `target/release/mink.exe help` | command list includes `build`, `run`, `check`, `test`, `repl` |
| `target/release/mink.exe build tmp/s217_probe/hello.mink` | exit 0; program printed `42`, exit 0 |
| `target/release/mink.exe build tmp/s217_probe/stdlib.mink` | exit 0; program printed `hi`, exit 0, no `E-R06` |

No Rust test suite was run: the change set is documentation only and the
compiler, runtime and stdlib were not modified. `git diff --check` returned
exit 0. The four pre-existing untracked user files remain untracked.

## 5. M6 package review

`docs/validation/M6_EXTERNAL/` still matches the repository:

- all six referenced language/reference pages exist;
- `benchmarks/m6/session176/01_hello.mink` through `10_imports.mink` exist
  (ten tutorial programs plus three larger programs);
- all 15 `examples/*/main.mink` programs exist, and all ten
  `validation/*/main.mink` programs exist with `python_ref.py` and `verify.py`;
- the package continues to state that 1.0.5 is unpublished and that npm
  `@p4inz-code/mink` serves 1.0.4, which the registry check confirms;
- the reading note now points the tester at 1.0.5 as the current build without
  pretending the underlying reference audit was re-run.

The package is not external validation. M6 remains OUTSTANDING.

## 6. Result

| Item | Status |
| --- | --- |
| Active docs identifying the current build as 1.0.4 | **none found after correction** |
| Historical 1.0.4 references | preserved and historically accurate |
| Registry / unpublished statements | explicit: npm 1.0.4, 1.0.5 unpublished |
| Installation and version commands | re-verified against the registry and repository build |
| M6 package instructions | coherent with the corrected docs and existing paths |
| Compiler / test source changed | none |
| M6 external validation | OUTSTANDING, unchanged |

Commit: `docs: reconcile current 1.0.5 claims with the published 1.0.4 state`.
