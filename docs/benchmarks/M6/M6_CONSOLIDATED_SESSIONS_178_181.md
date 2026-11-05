# M6 Sessions 178-181 - consolidated developer-usability record

**Date:** 2026-11-05
**Status:** Sessions 178-181 **COMPLETE**; M6 **PARTIAL**
**Branch:** `main`
**Starting HEAD:** `14db65cac8ab76f010f3cf8006ad1ec36fb6fe52`
**Commit 1 (artifacts):** `3ec3b5f827a8b5632cb784aab9e0064f98624865`
**Commit 2 (this record):** see the repository log; this file is the second
commit of the pair.
**Evidence mode:** repository-internal audit and executable validation; no
external developer study.

## A. Session identities and status

| Session | Scope as executed | Status |
| --- | --- | --- |
| 178 | Standard-library discoverability and practical API audit | **COMPLETE** |
| 179 | Debugging, testing and developer workflow guide | **COMPLETE** |
| 180 | Cold-start application, verifier and learning evidence | **COMPLETE** |
| 181 | Cross-cutting usability audit, adversarial review and M6 decision | **COMPLETE** (decision: PARTIAL) |

The original programme table assigned 179 to diagnostics, 180 to testing and
181 to workload selection, with application work in 182-183 and closure in 185.
This assignment consolidated those scopes: 179 now covers diagnostics plus
testing, 180 absorbs the application implementation and audit, and 181 takes
the closure decision. The original rows remain in
[`M6_DEFERRED_SESSIONS.md`](M6_DEFERRED_SESSIONS.md) with the remap marked.
No historical record was rewritten.

## B. Baseline and preservation

| Item | Value |
| --- | --- |
| Starting HEAD | `14db65cac8ab76f010f3cf8006ad1ec36fb6fe52` |
| `origin/main` | `b860837a8a6039f304a503e3d85728a643ab470b` |
| Branch | `main` |
| Starting worktree | 63 entries: 37 modified, 26 untracked |
| Ending worktree after commit 1 | 63 entries, identical set |
| Pre-existing files reverted/staged | None |
| `README.md` | Untouched; pre-existing modifications preserved |

The expected HEAD and worktree count matched the handoff exactly. The
pre-existing entries are the M3/M4 work, including `README.md`, `LICENSE`,
backend/runtime sources, `stdlib/http.mink`, `stdlib/network.mink`, their
`npm/mink` copies, Sessions 128-149 records, `httpserver` work and the zip
artifact. None was resettled, cleaned, stashed or committed.

## C. Version and environment correction

The handoff expected an installed MINK 1.0.4 compiler. The installed npm
package on this machine reports **`mink 1.0.2`**; the repository
`target/release/mink.exe` reports **`mink 1.0.4`**. Earlier M6 records that
assume an installed 1.0.4 are corrected here. All recorded outputs in the new
references use the repository 1.0.4 binary unless the text says otherwise.
The installed binary was still used for the cross-version checks.

This is a superseded conclusion, not a defect in the previous sessions: the
Session 177 record described the then-available package as 1.0.4. The package
present now is 1.0.2.

## D. Deliverables

| Artifact | Path | Status |
| --- | --- | --- |
| Standard-library reference | `docs/language/STANDARD_LIBRARY_REFERENCE.md` | New, committed |
| Debugging/testing guide | `docs/language/DEBUGGING_AND_TESTING.md` | New, committed |
| Session 180 application | `benchmarks/m6/session180/invsync.mink` | New, committed |
| Session 180 verifier | `benchmarks/m6/session180/verify.py` | New, committed |
| Session 180 README | `benchmarks/m6/session180/README.md` | New, committed |
| Programme ledger update | `docs/benchmarks/M6/M6_DEFERRED_SESSIONS.md` | Modified, committed |
| This record | `docs/benchmarks/M6/M6_CONSOLIDATED_SESSIONS_178_181.md` | New, committed separately |

No compiler, runtime, standard-library, Linux-frozen, npm package or version
file was changed. No tag, release, push or version change was made.

## E. Standard-library inventory

All 27 `stdlib/*.mink` modules were opened and their top-level declarations
enumerated. Twenty-two modules expose callable functions; `option` and `result`
expose only generic enum declarations; `environment` exposes only `extern`
declarations and cannot be loaded.

| Module | Audited | Executed this session | Main evidence |
| --- | --- | --- | --- |
| `assert` | Yes | Yes | `mink test` pass and fail |
| `collections` | Yes | Yes | Vec create/push/get/set/sum/insert/pop/free |
| `crypto` | Yes | No | Source only |
| `csv` | Yes | Yes | Quoted-field parse, write, escape, element cleanup |
| `encoding` | Yes | Partial | Hex, Base64 encode/decode, URL encode; `Int` status flag discovered |
| `environment` | Yes | Yes (failure) | `mod environment;` parse failure; intrinsics without the module |
| `filesystem` | Yes | Yes | read/write/size/exists/remove/failure returns |
| `hashing` | Yes | No | Source only |
| `http` | Yes | No | Source only |
| `httpserver` | Yes | No | Source only |
| `json` | Yes | Yes | Parse, object/array access, serialize, arena free |
| `logging` | Yes | Yes | Levels, stderr format, suppression |
| `math` | Yes | Partial | pow, gcd, isqrt, popcount, sqrt, pi, floor, clamp |
| `network` | Yes | No | Source only |
| `option` | Yes | No | Declaration only |
| `process` | Yes | Yes | run/stdout/stderr/pid; invalid free reproduced |
| `random` | Yes | No | Source only |
| `re` | Yes | Yes | match/search/find_all/replace/split |
| `result` | Yes | No | Declaration only |
| `sqlite` | Yes | Yes | create/insert/select/rows/has_table/free |
| `strings` | Yes | Yes | search tuple, transform, split/join, parse |
| `tasks` | Yes | No | Source only |
| `threads` | Yes | No | Source only |
| `time` | Yes | Yes | epoch/calendar/format/strftime/strptime |
| `tls` | Yes | No | Source only |
| `zip` | Yes | Partial | Builder and reader; composite reader leak |
| `zlib` | Yes | Partial | compress/decompress, CRC-32 |

Omitted from execution: `crypto`, `hashing`, `http`, `httpserver`, `network`,
`random`, `tls`, `threads`, `tasks`, `option`, `result`. Their signatures are
documented as source evidence and marked accordingly. No claim of exhaustive
standard-library coverage is made.

## F. API contracts established by execution

Every claim below was compiled and run with `target/release/mink.exe` (1.0.4).

| Contract | Observed evidence |
| --- | --- |
| Print intrinsics append CRLF | `a=\r\n7\r\n`; no same-line print |
| Embedded `\n` adds an extra break | `rt_print_str("x\n")` writes `x\n\r\n` |
| Float format | `2.5`, `1`, `0.33333333333333331`, `0.30000000000000004`, `-0` |
| `rt_str_format` | Four `Str` args, up to three `{}`, returns owned string |
| `Map<K,V>` across functions | Parameter, return type, repeated passing all work |
| String map keys | Insert/lookup/replace work; keys may be heap strings |
| Map key order | `rt_map_keys` is hash order, **not** insertion order |
| Vec handle behavior | push/set/insert consume; get/len/pop borrow; free releases elements |
| `str_index_of` | `(Int, Str)`; frees the needle and returns the subject |
| `str_split` / `str_join` | Split consumes both and returns owned parts; join consumes the vector |
| `str_parse_int` | `(0, false)` on junk |
| Filesystem returns | write 5 / size 5 / read length 5 / missing read 0 / missing size -1 / remove 0 / write into missing dir -1 |
| JSON | Arena parse, object access, `json_to_str` round trip, `rt_free` |
| CSV | Quoted fields, `""` escape, write row; `rt_vec_free(row)` required |
| Base64 decode status | `(Str, Int)`, not `(Str, Bool)` |
| Time | epoch 1793881845 for 2026-11-05 12:30:45; format and strftime agree |
| Logging | Records on stderr, threshold via `MINK_LOG_LEVEL` |
| `process_stdout` | BSS-backed; freeing gives `E-R04` exit 104 |
| `sqlite` handles | Every tuple's second `Str` is the new database; discard + free old = `E-R04` |
| `zip_finish` | Frees the archive arena; a later `zip_free` is `E-R04` |
| Leak | `E-R06`, exit 106, earlier stdout kept |
| `rt_exit` with live allocations | Still exits 106 through the leak check |
| `mink run` | Propagates `main`'s integer return code (7 observed) |

## G. Exact commands and counts

```bash
git rev-parse HEAD                      # 14db65c...
git status --porcelain=v1 | wc -l       # 63 before and after commit 1
./target/release/mink.exe --version     # mink 1.0.4
mink --version                          # mink 1.0.2 (installed)
python benchmarks/m6/session180/verify.py
python benchmarks/m6/session180/verify.py --compiler <installed>/bin/mink.exe
```

| Check | Result |
| --- | --- |
| Session 180 verifier, repository 1.0.4 | **12/12 cases**, exit 0 |
| Session 180 verifier, installed 1.0.2 | build passes; **normal case exits 106**, verifier exit 1 |
| `examples/*/main.mink` compile sweep (temp copy) | **15/15**, exit 0 |
| README `mink` fences | **5/5** compile |
| Standard-library reference `mink` fences | **11/11** compile |
| Local links in `docs/language/` | **0 broken** |
| `git diff --check` (staged and worktree) | exit 0 |
| Pre-existing worktree entries after commit | **63**, unchanged |

The installed-compiler failure is a real version difference: 1.0.2 rejects a
map key reused after `has`/`get` and, once that is worked around, leaks one
allocation on the normal path. It is recorded as a limitation, not hidden. The
repository 1.0.4 behavior is the primary result.

## H. Session 180 application behavior

`invsync <input.csv>` validates a required `sku,category,qty,price` header,
four fields per record, integer quantity and price, non-negative values, and
non-empty identifiers. It aggregates quantity per category with
`Map<Str, Int>`, prints a deterministic summary, and writes `report.txt`.
Category order is first-seen input order, derived from the input because
`rt_map_keys` is hash-ordered.

| Case | Exit | Output/artifact |
| --- | --- | --- |
| Normal 4-row file | 0 | summary; report with `fasteners 42`, `hardware 80` |
| Header only | 0 | zero totals; two-line report |
| Empty file | 3 | stderr, no report |
| Wrong field count | 3 | stderr "invalid row 2", no report |
| Non-numeric quantity | 3 | stderr, no report |
| Negative quantity | 3 | stderr, no report |
| Bad header | 3 | stderr, no report |
| Missing file | 4 | stderr, no report |
| Zero/two arguments | 2 | usage on stderr |
| Blocked `report.txt` path | 5 | stderr, no success output |
| Aggregate/zero/CRLF boundary | 0 | exact aggregated report |
| Extra positional argument | 2 | usage on stderr |

The verifier builds in a temporary directory with cwd set to the repository
root, runs each case in its own directory, checks exact stdout, stderr, exit
code and artifact contents, and deletes only its own temporary tree.

## I. Cold-start learning evidence

This was an internal developer-workflow simulation performed by the agent, not
a study of an external developer. No human learning time is claimed.

What had to be discovered to build the application:

1. That modules load with `mod`, not `use`, and that the repository compiler
   needs cwd at the repository root. `LANGUAGE_BASICS.md` sections 4 and 7
   answered this; the failure mode was reproduced first.
2. That user functions consume `Str`, so parsing a CSV row requires a copy
   (`rt_str_concat(raw, "")`) before `csv_parse_row`. The reference and the
   `strings` module comments answered this.
3. That `csv_parse_row` returns `Vec<Str>` whose elements are released by
   `rt_vec_free`. The CSV source comment and the reference answered this.
4. That `rt_map_keys` is not insertion-ordered. Only execution revealed this;
   it is now in the reference.
5. That `rt_str_format` takes exactly four arguments (format plus three
   values). `LANGUAGE_BASICS.md` section 2 answered this.

APIs that were difficult to locate without the reference: the JSON arena entry
points, `csv_field_count`, the `sqlite` tuple convention, and the fact that
`environment.mink` is unusable. The API reference reduced source searching for
these modules, but it did not eliminate it for the modules that were not
executed.

Compiler diagnostics that were unclear: `E-T15` for a qualified module call;
`E-T37` noise when a tuple destructure targets an unknown function result; and
the truncated `assert_eq_int` failure text. Language concepts that required
workarounds: path-insensitive ownership (free only on one path), no
same-line print, no map iteration order, and the `Vec<Str>` corruption below.

## J. Adversarial findings and fixes

| # | Finding | Action |
| ---: | --- | --- |
| 1 | First report omitted a newline between the summary and the first category | Fixed; exact-output assertion now covers it |
| 2 | Category order used `rt_map_keys` (hash order) | Replaced with a first-seen pass over input rows |
| 3 | `Vec<Str>` accumulated inside nested `if`/`else` reported length 16 for one element and `E-R04` on free | Removed the vector; recorded as a suspected compiler/runtime defect |
| 4 | Installed 1.0.2 rejected a map key reused after `has`/`get` | Restructured to a lookup key plus a stored copy; 1.0.2 then built |
| 5 | Verifier used `Path.read_text(newline=...)`, unsupported here | Replaced with `open(..., newline="")` |
| 6 | Own CSV probe leaked before `rt_vec_free(row)` | Probe fixed; rule documented |
| 7 | Own sqlite probe discarded the returned db handle and freed the old one (`E-R04`) | Probe fixed; tuple convention documented |
| 8 | `zip_free` after `zip_finish` is an invalid free | Documented; no runtime change |
| 9 | `process_stdout()` must not be freed | Documented; reproduced |
| 10 | A probe named `strings.mink` shadowed the stdlib module | Documented as a module-resolution trap |
| 11 | `mod environment;` fails to parse | Documented as a defect; intrinsics work without it |

The `Vec<Str>` finding is the most serious new observation. The application's
reproducer was a `Vec<Str>` pushed only inside a deeply nested `else` branch;
`rt_vec_len` returned 16 and the free was invalid, while shallower probes with
the same operations exited 0. A minimal compiler reproduction was **not**
produced, so it is recorded as an unresolved suspected defect rather than a
confirmed one. The application no longer uses that pattern.

## K. Known limitations and deferred defects

1. Collection use-after-free (`Map`/`Vec`/`Set`) is still undiagnosed. Preserved
   known defect; no ownership-system rewrite attempted.
2. `stdlib/environment.mink` cannot be loaded; `rt_env_*` works only as
   predeclared intrinsics.
3. The `zip` reader leaks in a composite probe (exit 106); individual probes
   exit 0. Source not isolated.
4. `Vec<Str>` nested-push corruption (above) has no minimal reproduction.
5. Installed 1.0.2 does not run the Session 180 normal case cleanly (exit 106)
   while 1.0.4 does. Version parity unverified.
6. `src/backend/ir.rs` still says `PrintFloat` uses six fractional digits; the
   observed behavior uses up to 17 significant digits. Source comment left
   unchanged, runtime untouched.
7. Qualified module calls (`helper::double`) still produce the misleading
   `E-T15` diagnostic. Documented as unsupported; no diagnostic redesign.
8. `README.md` still says the standard library is "imported with `use`" and
   omits `Vec`/`Map`/`Set` from its type list. It had substantive uncommitted
   edits at baseline, so it was preserved untouched. This remains deferred.
9. `mink explain` does not document runtime `E-R04`/`E-R06`.
10. No exhaustive standard-library verification; the unexecuted modules are
    listed explicitly.
11. No external developer study or human timing.

## L. README and worktree decision

`README.md` was not modified. It contains pre-existing uncommitted changes, and
no narrowly scoped edit could be proven to preserve all of them while
correcting the `use` wording and the collection list. The contradictions are
recorded in the reference and here. All 63 pre-existing entries were preserved;
commit 1 staged only the six audit paths listed in section D, and the worktree
returned to 63 entries after the commit.

## M. M6 decision

**PARTIAL.** The developer-usability objectives are substantially served: an
audited API reference exists, a working diagnostics/testing guide exists, and a
representative application is independently verified on the repository
compiler. M6 cannot close on this evidence because the environment module is
unloadable, the zip reader leaks, the `Vec<Str>` corruption is unexplained, and
the installed compiler does not match the repository compiler. These are
concrete acceptance gaps, not polish.

The planned sessions were combined, but that is not a reason to mark M6
complete.

## N. Duration

- Earliest probe artifact: Unix `1793876630`.
- Last probe/verification artifact: Unix `1793877790`.
- Measured artifact window: **1159 seconds (about 19 minutes 19 seconds)**.
- Baseline reading, reasoning before the first probe, reference writing and the
  final commit were not separately instrumented.
- No idle time or artificial work was added.

## O. Work left uncommitted

None of this assignment's work is uncommitted. Commit 1 contains the reference,
guide, application, verifier, README and ledger update. Commit 2 contains this
record. The 63 pre-existing M3/M4 entries remain uncommitted by design, exactly
as found.

## P. Next recommended action

**Session 182 - defect closure**, before any M6 closure record or M7 work:

1. Fix or explicitly deprecate `stdlib/environment.mink` so `mod environment;`
   either loads or is documented as unsupported in the module index.
2. Isolate the `zip` reader leak with a per-call exit-code matrix and fix it.
3. Produce a minimal reproduction of the `Vec<Str>` nested-push corruption
   (length 16 / `E-R04`) or downgrade it to a documented workaround if it
   cannot be reproduced.
4. Reconcile installed 1.0.2 with repository 1.0.4 on the Session 180
   application, or record the version difference as a supported limitation.
5. Only then correct `README.md` once its pre-existing edits are resolved, and
   write the M6 closure decision.
