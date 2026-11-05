# M6 Session 183 - final acceptance and closure

**Date:** 2026-11-05
**Status:** **COMPLETE**
**Starting HEAD for the closure block:** `02925a9e0cd110227d2055e7df43f329b81cdb56`
**Branch:** `main`
**M6 decision:** **COMPLETE**, with explicit accepted limitations

This session reviewed the M6 evidence independently of the implementation work.
It did not assume that passing tests imply usability, and it did not treat
documentation volume as product reliability.

## 1. Acceptance criteria

| # | Criterion | Evidence | Verdict |
| ---: | --- | --- | --- |
| 1 | Install and identify the version | `npm view @p4inz-code/mink version` -> **1.0.4**; repository `mink --version` -> 1.0.4; 1.0.2 identified as stale and unsupported | **MET** |
| 2 | Write and compile a first program | README quick-start fences compile **5/5**; `examples/*/main.mink` previously built **15/15** | **MET** |
| 3 | Core constructs documented accurately | `LANGUAGE_BASICS.md`, `CORE_GRAMMAR.md`, corrected README `mod` wording and collection type list; complete reference fences **12/12** | **MET** |
| 4 | Module loading and collection types understandable | `mod` semantics documented and executed; `Vec`/`Map`/`Set` added to the README type list; environment module now loads | **MET** |
| 5 | Standard-library APIs discoverable | 27-module index, signatures, ownership notes and evidence markers in `STANDARD_LIBRARY_REFERENCE.md`; unexecuted modules named explicitly | **MET** |
| 6 | Ownership and cleanup clear enough to build | pass-back and collection rules documented; Session 180 app frees every owned value; two probe ownership errors explained | **MET** |
| 7 | Representative CLI application | `invsync` handles 12 cases with exact output, exit codes and artifacts | **MET** |
| 8 | Test and debug programs | `DEBUGGING_AND_TESTING.md`; `mink test`; 60 focused native tests pass | **MET** |
| 9 | Diagnostics and limitations honest | `E-P01`, `E-R04`, `E-R06`, `E-T01` reproduced; unsupported 1.0.2, collection UAF and unexecuted modules documented | **MET** |
| 10 | Prior defects resolved or accepted | environment fixed; ZIP and Vec reports withdrawn with evidence; version explained; collection UAF explicitly accepted as a known checker gap | **MET** |

## 2. Defect disposition review

| Blocker | Session 182 disposition | Acceptance judgement |
| --- | --- | --- |
| Environment module unloadable | Repaired with wrapper functions; 2/2 regression tests | Resolved |
| ZIP reader leak | Probe kept an owned pass-back; every reader call is clean when it is freed; 11/11 tests | Not a product defect |
| Nested `Vec<Str>` corruption | Probe read and freed a vector twice; minimal repro shows stale length and E-R04; 47/47 tests | Incorrect test assumption |
| 1.0.2 vs 1.0.4 | 1.0.2 predates the bundled `strings.mink` ownership fixes; 1.0.4 is the minimum verified compiler | Explained version limitation |

The ZIP and Vec conclusions are supported by per-call matrices and a minimal
reproduction, not by a plausible theory. No leak check was disabled and no
compiler behavior was changed to make a report disappear.

## 3. README correction and preservation

Two lines were corrected:

- standard-library files are loaded with `mod`, not imported with `use`;
- the type list now includes `Vec<T>`, `Map<K, V>` and `Set<T>`.

The README had 20 insertions and 12 deletions of pre-existing uncommitted work.
Committing the file normally would have included that work. The two corrected
lines were therefore applied to the `HEAD` blob, written as a new blob with
`git hash-object -w`, and staged with `git update-index --cacheinfo`. Commit
`02925a9` contains exactly **2 insertions and 2 deletions**; the working tree
still carries the original 20/12 diff. All pre-existing README edits were
preserved and verified after the commit.

README complete fences: **5/5 compile**. No previous content was lost.

A pre-existing paragraph in the README still says the registry serves 1.0.3
and that 1.0.4 is pending. `npm view` now reports 1.0.4. That paragraph is part
of the uncommitted pre-existing README work, so it was not rewritten; it is
recorded as a documentation limitation rather than silently merged.

## 4. Documentation consistency

Checked:

- `README.md`
- `docs/language/LANGUAGE_BASICS.md`
- `docs/language/CORE_GRAMMAR.md`
- `docs/language/CORE_LANGUAGE.md`
- `docs/language/STANDARD_LIBRARY_REFERENCE.md`
- `docs/language/DEBUGGING_AND_TESTING.md`
- `stdlib/environment.mink`, `stdlib/zip.mink`, `stdlib/strings.mink`
- the M6 application and verifier documentation

Results:

| Check | Result |
| --- | --- |
| Contradictory module-loading claims | corrected in README, reference and debugging guide; no remaining `use` claim |
| Incorrect ownership guarantees | none introduced; pass-back and double-free rules match execution |
| Stale Map workaround | no new workaround; the `Vec<Str>` report is withdrawn |
| Wrong standard-library signatures | environment wrappers added; executed signatures unchanged |
| Unsupported platform claims | still Windows x86_64 only; no Linux claim added |
| Compiler-version requirement | 1.0.4 stated as minimum; 1.0.2 marked unsupported |
| Examples that fail to compile | README 5/5, reference 12/12, no failures |
| Broken local links | **0** across `docs/language/` and `docs/benchmarks/M6/` |

The Sessions 178-181 consolidated record remains as historical evidence of the
PARTIAL decision at that time. It is superseded by this closure, not rewritten.

## 5. Accepted limitations

These do not invalidate M6 because their scope and impact are explicit:

1. **Collection use-after-free is not diagnosed.** `rt_map_free(m); rt_map_get(m, 1);`
   compiles and returns stale data. This is documented as an unsupported
   contract in `LANGUAGE_BASICS.md` section 5 and the standard-library
   reference. Correct programs are unaffected.
2. **1.0.2 is unsupported.** Its bundled `strings.mink` lacks the ownership
   releases added before 1.0.3, so heap arguments to helpers such as `str_trim`
   leak. The registry serves 1.0.4; the minimum verified compiler is 1.0.4.
3. **Some standard-library modules remain source-audited only** (`crypto`,
   `hashing`, `http`, `httpserver`, `network`, `random`, `tls`, `threads`,
   `tasks`). The reference marks them SOURCE and makes no execution claim.
4. **No external developer study.** The cold-start work is an internal
   workflow simulation; no human learning time is claimed.
5. **README publish-state paragraph is stale** (says registry 1.0.3). It is
   pre-existing uncommitted content and was deliberately not rewritten.

## 6. Decision

**M6 COMPLETE.**

The developer-usability deliverables exist and are executable: a verified
standard-library reference, a working debugging/testing guide, a representative
CLI application with an independent 12-case verifier, and regression coverage
for every resolved or withdrawn blocker. The four recorded blockers are either
fixed or shown not to be product defects, and the remaining limitations are
explicitly documented and bounded.

Closure is for the repository's M6 scope. It does not claim exhaustive
standard-library coverage, human usability validation or M7 readiness. It also
does not change the release version or publish anything.

## 7. Next action

**Session 184 - build, packaging and distribution usability.** Verify the
published 1.0.4 package from a clean install, the packaged `stdlib/` contents,
the Windows file-type registration and the CLI workflow. Do not begin M7 until
that verification and the M6 administrative close-out are complete.
