# Session 164 — M5 Application 1: `repoaudit`

**Date:** 2026-11-02
**Status:** **COMPLETE — implementation and Session 164 gate green.**
**Contract:** [`M5_APPLICATION_SELECTION.md`](M5_APPLICATION_SELECTION.md),
section 5 (`repoaudit`), frozen in Session 163.
**Boundary:** application engineering only. No performance measurement, no
cross-language port, and no MINK compiler/runtime/stdlib change.

## 1. Contract reference

The frozen contract requires an offline ASCII project inventory and policy
audit with:

- `repoaudit <config.json>` or `--help`;
- exactly seven JSON config keys and full schema/type/range validation;
- a recursive case-insensitive include-extension walk with case-insensitive
  directory exclusions at any depth;
- deterministic byte-order inventory paths using `/`;
- binary, physical-line, marker, size and marker-limit analysis;
- `inventory.csv`, `violations.csv`, compact `summary.json`, and one stdout
  CRLF summary line;
- exit codes 0–7 with diagnostics on stderr only;
- no glob/gitignore, hashing, network, process, watch, auto-fix, HTML/Markdown
  report, configurable regex or parallel scanning.

## 2. Implementation summary

Single executable source:
`benchmarks/m5/repoaudit/app/main.mink` (1,006 lines). It uses only the
existing `filesystem`, `json` and `strings` stdlib modules.

Design points:

- manual argv scan with one positional config path and `--help`;
- arena-backed JSON parse followed by exact-key, type, range and token
  validation;
- iterative directory walk (no recursive Vec duplication) with normalized
  relative paths and explicit `/` joining;
- insertion sort in ascending byte order before any report is built;
- whole-file reads only at or below `max_file_bytes`; NUL probe limited to the
  first 4,096 bytes; LF-based physical line count; non-overlapping marker scan;
- RFC-4180 CSV quoting for fields containing comma, quote, CR or LF;
- reports are built fully in memory before writing, so a scan failure leaves no
  partial reports;
- explicit single-exit cleanup; the runtime's leak check is clean on success,
  violation and tested failure paths.

### Files added

```text
benchmarks/m5/repoaudit/app/main.mink
benchmarks/m5/repoaudit/tests/run_tests.py
benchmarks/m5/repoaudit/README.md
benchmarks/m5/repoaudit/.gitignore
benchmarks/m5/repoaudit/.gitattributes
benchmarks/m5/repoaudit/fixtures/basic/...
benchmarks/m5/repoaudit/fixtures/clean/...
benchmarks/m5/repoaudit/fixtures/empty/...
benchmarks/m5/repoaudit/expected/basic/...
benchmarks/m5/repoaudit/expected/clean/...
benchmarks/m5/repoaudit/expected/empty/...
```

No MINK source, M4 result, or pre-existing M3 file was changed.

## 3. Test matrix — Session 164 gate

The Python driver is an independent oracle only; it is not a shipped port. It
recomputes every report from the frozen contract and compares complete output
trees byte-for-byte.

| Category | Checks | Result |
| --- | ---: | --- |
| Basic contract + committed goldens | 3 | GREEN |
| No-match / empty inventory | 1 | GREEN |
| Line, binary, marker, exclusion semantics | 2 | GREEN |
| Violation ordering and details | included above | GREEN |
| Three-run byte determinism | 1 | GREEN |
| Help and four usage-error forms | 5 | GREEN |
| Config missing / not a file | 2 | GREEN |
| Invalid JSON / non-object / duplicate key | 3 | GREEN |
| Schema, type, range and token validation | 24 | GREEN |
| Root missing / not a directory | 1 | GREEN |
| Out missing / not a directory / report blocked | 1 | GREEN |
| Config and out paths containing spaces | 2 | GREEN |
| Stdout exactly one CRLF line | 1 | GREEN |
| Windows sharing-lock scan failure (`7`) | 1 | GREEN |
| Isolated executable outside the source tree | 1 | GREEN |
| **Total** | **48** | **48 passed, 0 failed, 0 skipped** |

Session 165 later added two closure checks (case/boundary coverage and CSV
quoting), bringing the closure suite to 50.

## 4. Standalone artifact

```text
build:  target/release/mink.exe build benchmarks/m5/repoaudit/app/main.mink
artifact: benchmarks/m5/repoaudit/build/repoaudit.exe
size:     216,576 bytes
sha256:   3d1b15cf91f9afa8466a811c82355564660c716e753187c0bc4763e2a361b46a
```

The suite copies the executable to a temporary directory outside the source
tree, runs it against a copied fixture, and confirms the violation exit code
and reports remain correct. No MINK source tree or sidecar file is required at
run time.

## 5. Deterministic-output evidence

The basic violating fixture was run three additional times after the oracle
comparison. Every run produced:

- exit code `6`;
- identical `inventory.csv`, `violations.csv`, `summary.json`;
- identical stdout bytes.

Basic report totals:
`{"files":16,"bytes":12549,"lines":26,"binary":1,"todo":10,"fixme":2,"violations":2}`.
Clean fixture: 3 files, 65 bytes, 4 lines, 0 violations. Empty fixture: all
zero with header-only CSV reports.

Fixture bytes are preserved by
`benchmarks/m5/repoaudit/.gitattributes` (`* -text`). `git ls-files --eol`
confirms CRLF fixtures stay CRLF in the index and LF fixtures stay LF.

## 6. Defects found during implementation

All were application-code defects; none required a MINK change.

1. **Zero-capacity vector** — `rt_vec_new(0)` raised `E-R08` for empty arrays.
   The application now uses a minimum capacity of one and frees every vector.
2. **Summary leak** — `summary` was scoped to the success branch and not
   released, producing `E-R06`. The branch now frees the body or its
   post-write placeholder.
3. **Double line ending** — `rt_print_str` appends the line ending, so the audit
   line must not include its own CRLF. Help output was corrected the same way.
4. **Unconsumed report paths on write failure** — failure branches now release
   the paths whose writes were skipped.

No compiler/runtime defect was found. No MINK source change was made.

## 7. Known limitations and contract interpretations

- The unscored ANSI probe (`café/notes.txt`) exits 0 but reports the path as
  `caf�`. The frozen no-Unicode-path guarantee is respected; this is recorded,
  not counted as a scored check.
- `markers[0]` feeds `todo` and `markers[1]` feeds `fixme`. The frozen report
  schema has no column for additional configured markers, so entries 3–4 are
  schema-valid but do not appear in reports or affect `max_markers`. This is a
  Session 163 contract ambiguity, not hidden behavior.
- CSV line endings are LF, stdout is CRLF, and `summary.json` has no trailing
  newline. These are deterministic and oracle-checked.
- Paths remain ASCII-only; there is no glob, gitignore, symlink or hidden-file
  policy, and no parallel scan.

## 8. Git

Implementation commit: `2fe1d21` —
`benchmarks: implement and verify M5 repoaudit`.

Only `benchmarks/m5/repoaudit/**` files were staged. `git diff --cached --check`
passed with CR-at-EOL allowed for intentional fixtures. Pre-existing modified
and untracked M3 work was untouched. No reset, clean, stash, rebase, amend,
force, push, tag or publish was used.

## 9. Next action

**Session 165 — `repoaudit` deep audit / closure.** Audit every frozen
requirement, adversarial inputs, byte-exact artifacts, error paths and the
standalone executable. Do not start `hookbox` until closure is green.
