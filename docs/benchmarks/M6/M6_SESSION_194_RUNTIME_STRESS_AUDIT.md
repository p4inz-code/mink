# Session 194 — Roadmap Reconciliation and Windows Runtime Stress Audit

Windows x86_64, 2026-11-06. Starting HEAD
`38ed92da7d5acc8ea14548fdb43c2acce150dcb8`.

## 1. Baseline

| Item | Observed |
| --- | --- |
| Branch | `main` |
| HEAD | `38ed92d` (matches the expected starting commit) |
| `origin/main` | `38ed92d` after fetch |
| `git ls-remote` | `38ed92d` — local, tracking and remote agree |
| Ahead/behind | `0 0` |
| Untracked user files | all four present and untouched (`MINK UI SETS OFFICIAL.zip`, chat export, `nl1.mink`, `nl1.exe`) |
| npm (scoped package) | `@p4inz-code/mink` — `1.0.1`–`1.0.4`, `latest: 1.0.4`; 1.0.5 unpublished |

Registry note: `npm view mink` refers to an unrelated `f1lt3r/mink` package
(`0.0.1`, `1.0.0-alpha.0`). The repository package is scoped
(`"name": "@p4inz-code/mink"`), and the scoped query confirms the historical
1.0.4 claim. No publication claim is changed by this session.

## 2. Roadmap reconciliation

| Milestone | Recorded status | Checked against implementation | Result |
| --- | --- | --- | --- |
| M0–M1 | COMPLETE | records present, M1 evidence preserved | consistent |
| M2 | COMPLETE (Session 138) | stress records present | consistent |
| M3 | COMPLETE (Sessions 140–149) | defect list closed in records | consistent |
| M4 | COMPLETE | M4 benchmark records exist | consistent; roadmap §8's "next action: M4 Session 151" is historical and already carries a dated Session 193 status note |
| M5 | COMPLETE | final report and readiness gate present | consistent |
| M6 | repository acceptance COMPLETE; external validation not performed | no outside developer exists in the record; Session 174 was an internal cold-start simulation and is labelled as such | **not externally validated; cannot be closed internally** |
| M7 | NOT STARTED, demand-gated | no demand evidence; roadmap forbids starting | unchanged |
| M8 | NOT STARTED, gated on M6/M7 evidence | not reachable | unchanged |
| Linux | frozen | untouched by 190–194 | unchanged |

No acceptance criterion was wrongly marked satisfied by Sessions 189–193. The
ownership closure is library-level reliability work and does not substitute for
M6's external-developer criterion.

## 3. Areas inspected

- `src/runtime/allocator.rs` — liveness table, leak scan, free-list semantics.
- `src/backend/emit/runtime.rs` — `Free`, `MemLoad`, `MemStore`, `Vec*`,
  `Map*`, `Set*` services.
- `docs/implementation/RUNTIME_IMPLEMENTATION.md` — documented `E-R04/05/06/07/10`
  semantics and the liveness-table contract.
- `docs/language/STANDARD_LIBRARY_REFERENCE.md` §5.2 — the recorded collection
  use-after-free limitation.
- `docs/benchmarks/M6/M6_SESSION_177/182/183` — history of the nested-collection
  and use-after-free findings.
- `docs/audits/SYSTEMS_READINESS_AUDIT.md`, `docs/roadmap/IMPLEMENTATION_ROADMAP.md`,
  `M6_DEFERRED_SESSIONS.md`.

Key runtime fact: the liveness table is authoritative for `rt_free`,
`rt_mem_load` and `rt_mem_store`, but the collection services read the header
(`capacity`, `length`, `descriptor`) directly from the handle with no table
consultation. That is the structural cause of the confirmed defect below.

## 4. Historical collection concern — reproduced

The concern was recorded as "collection use-after-free is not diagnosed" and
preserved as an unsupported contract. It **still exists** and is deterministic,
not timing-dependent:

| Probe | Program after `free` | Observed | Exit |
| --- | --- | --- | --- |
| `uaf_vec_len` | `rt_vec_len(v)` | prints `48` (freed block's free-list link read as length) | 0 |
| `uaf_vec_get` | `rt_vec_get(v, 0)` | prints stale `77` | 0 |
| `uaf_map_get` | `rt_map_get(m, 1)` | prints stale `42` | 0 |
| `uaf_map_len` | `rt_map_len(m)` | prints `144` | 0 |
| `uaf_set_len` | `rt_set_len(s)` | prints `96` | 0 |
| `uaf_set_has` | `rt_set_has(s, 9)` | prints `0` | 0 |
| `uaf_vec_push` | `rt_vec_push(v, 2)` then `len` | prints `33`, exit 0, deterministic 3/3 | 0 |
| `uaf_after_reuse` | free `v`, allocate `w`, then read `v` | reads `w`'s header (`0`, `0`) | 0 |

Repeat runs of `uaf_vec_len` returned `48` five times out of five; repeat runs of
`uaf_vec_push` returned `33` three times out of three. The values are deterministic
because the LIFO free list writes a fixed link pattern into the freed block.

Controls that already work:

| Probe | Observed | Exit |
| --- | --- | --- |
| double `rt_vec_free` | `E-R04: invalid free` | 104 |
| `rt_vec_get(v, 5)` on a one-element vec | `E-R10: array index out of range` | 110 |
| leaked handle at exit | `E-R06` | 106 |

**Root cause:** `emit_vec_len`, `emit_vec_get`, `emit_vec_push`, and the map/set
equivalents read `[handle+0/8/16]` directly. The liveness table is never
consulted, so a dead block is indistinguishable from a live one. This is shared
across `Vec`, `Map` and `Set` — not specific to one collection type — and it is
documented as a limitation rather than silently tolerated by tests.

**Severity:** silent stale reads and writes after free in a language whose
headline guarantee is explicit ownership. It is worse than the leak class fixed
in Sessions 190–193 because it produces wrong data rather than a detectable
exit code.

**Not fixed yet; no fixing commit exists.** The limitation is recorded in
`STANDARD_LIBRARY_REFERENCE.md` §5.2 and the M6 records, but no code change has
ever addressed it.

## 5. Stress baselines (bounded, Windows x86_64, release compiler)

| Workload | Count | Result | Wall time |
| --- | --- | --- | --- |
| `rt_alloc(32)` + `rt_free` cycle | 20,000 | exit 0 | 28 ms |
| `rt_vec_push` growth then free | 50,000 pushes | exit 0, length 50,000 | 29 ms |
| create + free independent vectors | 5,000 | exit 0 | 29 ms |
| write/read/remove file cycle (7 bytes) | 200 | exit 0 | 73 ms |
| 256 KiB string build + write + read + free | 262,144 bytes | exit 0, length matches | 30 ms |
| `rt_process_run("cmd /c exit 7")` | 30 | exit 0, every child code 7 | 644 ms |
| compiler builds of `invsync.mink` | 5 | 5/5 success | 156 ms |

Probe outcomes that were **probe errors, not defects**:

- A filesystem probe freeing inside an `if` and again after it failed `E-S10`
  (use of moved value). This is the documented no-path-sensitivity rule; the
  corrected single-release probe passes.
- `Vec<Vec<Int>>` fails to build with `E-B07: collection element type not
  supported: type 'unresolved' has no collection descriptor`. Nested collections
  as element types are not a supported construction; this is a documented
  limitation, not a regression.
- A live-allocation-limit probe hit a type error (`Ptr<Int>` assigned to `Int`)
  in the probe itself and was not pursued; the limit is separately documented as
  `E-R03`.

No leak, crash, stale handle or resource exhaustion appeared in the stress
probes above. Process and filesystem lifecycles are clean under these bounds.

## 6. Selected Session 195 issue

**Collection handle use-after-free is not diagnosed (`Vec`, `Map`, `Set`).**

Selection rationale:

- Confirmed defect with a deterministic reproduction and a structural root
  cause, shared by all three collection types.
- Material impact: silent wrong data after free, unlike every other runtime
  misuse, which fails with a structured `E-R0N` code.
- Fixable within the existing runtime design: the liveness table already exists
  and is consulted by `Free`/`MemLoad`/`MemStore`; the collection services must
  consult it before using the header.
- Independently verifiable with the probes above plus the existing suites.
- Fits the platform commitment (Windows x86_64; Linux stays frozen).

### Acceptance criteria for Session 195

1. A read/query/mutation on a freed `Vec`, `Map` or `Set` handle exits `105`
   (`E-R05`) with the structured runtime error, not stale data.
2. Double-free still exits `104` (`E-R04`); out-of-bounds still exits `110`
   (`E-R10`); leaks still exit `106` (`E-R06`).
3. Valid collection programs are unchanged: `collections_lib` (49),
   `filesystem_lib` (47), `strings_lib` (88) and the full suite stay green.
4. Permanent regression tests cover `Vec`, `Map` and `Set` use-after-free,
   including use-after-free on mutation.
5. Release build and `invsync` verifier still pass.

## 7. Deferred / not in scope

- M6 external developer validation — requires a real outside developer; not
  fabricated.
- M7 ecosystem work — demand-gated.
- `Vec<Vec<T>>` element support — separate capability decision.
- Generation counters to detect handle reuse after a block is reallocated to a
  new object — beyond the liveness-table design; a freed-then-reused handle can
  still alias. This limitation will be stated if it survives the fix.
- npm publication — unauthorized and unauthenticated; 1.0.5 stays unpublished.

## 8. Next action

Session 195: implement liveness validation for collection handles at the entry
of the affected `Vec`/`Map`/`Set` services, with regression tests and
documentation, without changing free/double-free/out-of-bounds semantics.
