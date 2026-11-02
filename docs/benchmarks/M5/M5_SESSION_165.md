# Session 165 — `repoaudit` deep audit and closure

**Date:** 2026-11-02
**Status:** **REPOAUDIT COMPLETE — 50/50 checks green.**
**Contract:** [`M5_APPLICATION_SELECTION.md`](M5_APPLICATION_SELECTION.md),
section 5 (`repoaudit`), frozen in Session 163.
**Implementation commit:** `2fe1d21`.
**Scope:** closure audit only. No `hookbox` implementation began in this
session, and no performance measurement was performed.

## 1. Contract-by-contract audit

Every frozen requirement is mapped to implementation evidence, a test, and an
observed result. There are no BLOCKED required items.

| # | Frozen requirement | Implementation location | Test evidence | Result |
| ---: | --- | --- | --- | --- |
| 1 | `repoaudit <config.json>` or `--help`; other forms usage error | `main` argv scan; `print_help` | help; missing/extra/unknown; help with extra | GREEN |
| 2 | Exactly the seven required config keys | `load_config`; `json_obj_len == 7`; `find_key` | missing key, extra key, duplicate key | GREEN |
| 3 | `root`, `out` non-empty strings | `get_string`; `schema_fail` | root empty, out empty, root wrong type | GREEN |
| 4 | `include_extensions` 1–8 entries, `^\.[A-Za-z0-9]{1,8}$`, case-insensitive matching | `get_string_array`, `valid_extension`, `ext_selected`, `list_has_ci` | empty, nine entries, no dot, too long, non-alnum; `.MD` and `.MiNk` fixture rows | GREEN |
| 5 | `exclude_dirs` 0–8 non-empty base names, no slash, case-insensitive at any depth | `valid_exclude_name`; iterative `walk`; `name_excluded` | slash, empty, nine entries; `TARGET` excludes both `target` directories | GREEN |
| 6 | `markers` 1–4 entries, `^[A-Za-z0-9_]{1,16}$`, case-sensitive matching | `get_string_array`, `valid_marker`, `count_marker` | empty, five entries, hyphen, 17 chars; lowercase `todo fixme` counts zero | GREEN |
| 7 | `max_file_bytes` integer 1–1,048,576 | `get_uint`; canonical decimal check; range checks | zero, 1,048,577, float, string, negative | GREEN |
| 8 | `max_markers` integer 0–1,000 | `get_uint`; range check | negative, 1,001, string | GREEN |
| 9 | `out` must already exist as a directory | `main`; `fs_exists`/`fs_is_dir` | missing out; out is a file; blocked report path | GREEN |
| 10 | Inventory header, path/byte sort, `/` separators, relative paths | `walk`; `sort_paths`; `inventory_row`; `inventory.csv` golden | basic/clean/empty goldens; byte-order rows | GREEN |
| 11 | `lines` contract: 0 empty; LF+1 when no final LF; `-1` binary/size-skipped | scan line-count block | empty, CRLF, LF-final, no-final-LF, binary, oversized | GREEN |
| 12 | `binary=1` when NUL occurs in first 4,096 bytes; no marker counts | NUL probe before marker scan | `binary.txt`; `nul_at_4096.txt` (NUL at offset 4,096 stays text) | GREEN |
| 13 | Marker counts non-overlapping, case-sensitive | `count_marker` advances by marker length | `TODOTODO` counts 2; lowercase counts 0 | GREEN |
| 14 | File larger than `max_file_bytes`: not read, lines `-1`, binary 0, markers 0, `file_too_large`, `limit=<n>` | scan size branch; `violation_row` | oversized row; exact `limit=8192` detail; equal-size boundary reads normally | GREEN |
| 15 | Text `todo+fixme > max_markers`: `too_many_markers`, `count=<n>` | scan policy branch | `src/app.mink` 4 > 3; equal 4 with limit 4 is clean | GREEN |
| 16 | `summary.json` exact compact shape and key order | `append_num` construction | all three goldens; oracle byte comparison | GREEN |
| 17 | stdout exactly one CRLF line with six totals | scan stdout block; runtime `rt_print_str` termination | stdout byte test; oracle byte comparison | GREEN |
| 18 | stderr diagnostics prefixed `repoaudit: `; no stdout before fatal | `fail_msg`, `fail_path`, `usage`; write-after-scan ordering | every error test asserts empty stdout and prefix | GREEN |
| 19 | Exit 0 clean | `scan` returns 0 when no violations | clean fixture; equal boundaries | GREEN |
| 20 | Exit 1 usage/unknown/extra | argv scan | four usage forms | GREEN |
| 21 | Exit 2 config missing/not regular | `load_config` checks | missing config; config is a directory | GREEN |
| 22 | Exit 3 invalid JSON / non-object / schema | `load_config`, `schema_fail` | invalid JSON, array, duplicate, 24 schema cases | GREEN |
| 23 | Exit 4 root missing/not directory | `main` root checks | both forms | GREEN |
| 24 | Exit 5 out missing/not directory/report write failure | `main` out checks; `write_report` | missing, file, blocked `inventory.csv` directory | GREEN |
| 25 | Exit 6 violations after reports written | scan returns 6 after writes/stdout | basic fixture; reports exist and match goldens | GREEN |
| 26 | Exit 7 scan I/O failure | `walk`; file size/read checks | exclusive Windows sharing lock on one source file; no reports left | GREEN |
| 27 | No stack traces; explicit error handling | single-exit main/scan; diagnostics | all error tests | GREEN |
| 28 | Repeated-run determinism and no leaked strings | sorted walk; single-exit cleanup; summary-branch release | three repeated runs byte-identical; runtime leak check clean | GREEN |
| 29 | Required features: manual argv, schema, recursive scanning, analysis, reports | `main.mink` | 50-check suite | GREEN |
| 30 | Explicit non-features remain absent | source has no network/process/hash/archive/watch/HTML/Markdown/regex machinery | source inspection; no imports beyond filesystem/json/strings | GREEN |
| 31 | ASCII fixtures; documented no-Unicode-path guarantee | `README.md`; unscored ANSI probe | probe recorded below; not scored | GREEN (scoped) |
| 32 | Fixture budget ≤40 files, depth ≤5, arena-sized configs | 25 files in the basic tree; depth 3; small config | fixture inventory | GREEN |
| 33 | One standalone `repoaudit.exe`, no sidecar/runtime dependency | build procedure; isolated executable test | 216,576-byte PE; run outside source tree | GREEN |
| 34 | Definition of DONE | all items above | 50 passed, 0 failed, 0 skipped | GREEN |

### Frozen-contract ambiguity: markers 3–4

The schema accepts 1–4 marker tokens, but the frozen report columns are only
`todo` and `fixme`, and the policy is defined as `todo+fixme > max_markers`.
The implementation maps `markers[0] → todo` and `markers[1] → fixme`; entries
3–4 are accepted but cannot appear in the frozen reports and do not affect the
frozen policy. This is recorded as a contract limitation from Session 163, not
silently invented behavior. It does not block the required two-marker workflow.

## 2. Adversarial input audit

| Input | Observed result |
| --- | --- |
| Empty project | header-only CSVs, zero summary, exit 0 |
| No matching extension | header-only CSVs, zero summary, exit 0 |
| Nested directories | relative `/` paths; `nested/deeper/ok.txt` inventoried |
| Multiple matching files | 16 rows in basic fixture, byte-sorted |
| Zero-line file | `src/empty.txt` → lines 0 |
| File without final newline | `src/no_final.txt` → 2 lines, marker at final line counted |
| CRLF file | `src/crlf.txt` → 2 lines |
| NUL in first 4,096 bytes | binary row, lines `-1`, markers 0 |
| NUL exactly at offset 4,096 | treated as text; marker counted |
| Oversized file | `lines=-1`, `binary=0`, no markers, one `file_too_large` |
| Marker at first line | `docs/notes.txt` counted |
| Marker at final line | `src/no_final.txt` counted |
| Multiple markers | TODO/FIXME totals separate and combined policy checked |
| Multiple violations | oversized + marker-heavy rows in path order |
| Clean project | exit 0, no violations |
| Violating project | exit 6, reports present |
| Malformed JSON | exit 3, stdout empty |
| Missing config | exit 2 |
| Missing root | exit 4 |
| Invalid config values | exit 3 across 24 schema cases |
| Repeated identical execution | three byte-identical report trees |

## 3. Byte-exact artifact audit

The Python oracle independently recomputes inventory, violations, summary and
stdout from the frozen contract. The MINK executable matches it on every
fixture and on generated case/boundary/quoted-path variants.

Committed goldens:

- `expected/basic/inventory.csv`
  SHA-256 `9b741575798c793935daf2c70e0a338a2dcc1831318cc02ed9c4d78d9f3a3950`
- `expected/basic/violations.csv`
  SHA-256 `76d6cba51b88ccbb6d4aa0cccc471ec6f50469842e8eef2a64e9e203abb21fc9`
- `expected/basic/summary.json`
  SHA-256 `5045ffd155c4dc006a0023b833314a8e100377916c6c0cc9ed58b0e341ee3cb0`
- `expected/basic/stdout.txt` ends in exactly one CRLF.

Basic totals: files 16, bytes 12,549, lines 26, binary 1, TODO 10, FIXME 2,
violations 2. `src/app.mink` is the marker violation (count 4); the 8,193-byte
asset is the size violation (`limit=8192`).

Line-ending preservation is checked in Git:

- CRLF fixtures remain `i/crlf` with `w/crlf`;
- LF fixtures and goldens remain `i/lf` with `w/lf`;
- `benchmarks/m5/repoaudit/.gitattributes` sets `* -text` so checkout does not
  rewrite the compared bytes.

Three repeated basic runs produced identical stdout and identical hashes for
all three reports.

## 4. Error-path audit

- All seven exit classes are exercised: `0, 1, 2, 3, 4, 5, 6, 7` (0 and 6 are
  success outcomes; 1–5 and 7 are failures).
- Every failure test asserts empty stdout and a `repoaudit: ` stderr prefix.
- A scan failure leaves no `inventory.csv`, `violations.csv` or `summary.json`.
- A report-write failure returns 5 and does not print a success summary.
- No error path produced a stack trace, crash or runtime leak.
- No MINK compiler/runtime defect was found. The defects found during
  implementation were application bugs (zero-capacity vector, un-freed summary,
  duplicate line ending, un-freed report paths); all were fixed in the
  application, not in MINK.

## 5. Standalone release-like check

The freshly built executable was copied to a temporary binary directory
outside the repository source tree and run from a copied fixture directory.

Observed:

- launches without sidecars;
- clean project exits 0;
- violating project exits 6;
- reports match the oracle and committed goldens;
- no source-tree-relative resource is needed;
- no unexpected runtime dependency appeared.

Artifact: 216,576 bytes, SHA-256
`3d1b15cf91f9afa8466a811c82355564660c716e753187c0bc4763e2a361b46a`.

This is an engineering check, not a product release claim.

## 6. Evidence quality against M4

`repoaudit` is not a disguised JSON utility or a file-hashing benchmark. It
adds a complete policy workflow: seven-key schema validation, recursive
inventory, physical-line and marker analysis, size/marker policy, three
deterministic reports, and a non-zero policy exit. M4 did not exercise this
combination.

The M4 conclusion remains scoped: MINK can make a defined class of small
single-binary Windows CLI tools simpler to build and ship. `repoaudit` supports
that for this application: one build command, three existing stdlib modules,
one 216,576-byte artifact, and no package manager. It does not establish a
universal language ranking or a developer-time advantage; no controlled
developer-time measurement exists.

## 7. Limitations carried forward

- No Unicode path guarantee. The unscored probe reported `caf�` for `café`
  while exiting 0. Scored fixtures are ASCII.
- `markers[3..4]` are schema-valid but have no frozen report column and no
  policy effect; documented above.
- `exclude_dirs` rejects backslash and control characters in addition to `/`;
  this matches Windows base-name constraints but is stricter than the literal
  “no slash” sentence.
- CSV quoting is implemented for comma/quote/CR/LF paths; the frozen examples
  do not include a comma, but the added closure check covers it.
- Reports are overwritten in place, not by atomic rename. The contract allows
  “atomically enough for this scope”; a crash between writes can leave a
  partial set.
- The application is a single 1,006-line source file. No local module-splitting
  convention was available without inventing one; this is implementation
  friction, not a correctness limitation.

## 8. Test totals

| Suite | Checks | Passed | Failed | Skipped |
| --- | ---: | ---: | ---: | ---: |
| Session 164 gate | 48 | 48 | 0 | 0 |
| Session 165 closure additions | 2 | 2 | 0 | 0 |
| **Final `repoaudit` suite** | **50** | **50** | **0** | **0** |

`mink check` passes parsing, semantic analysis, type checking, HIR/MIR lowering
and MIR optimization. The unscored ANSI probe is reported separately and is not
part of the 50.

## 9. Commits and files

- `2fe1d21` — implementation, fixtures, goldens, oracle suite.
- Session 165 closure commit — two added checks and these Session 164/165
  records. It is the repository HEAD immediately after this audit; the log is
  authoritative for its hash.

Only M5-owned files were staged. Pre-existing modified/untracked M3 work was
untouched. No reset, clean, stash, rebase, amend, force, push, tag or publish
was used.

## 10. Final status

**REPOAUDIT COMPLETE.**

There is no unresolved `repoaudit` correctness defect, no blocked frozen
requirement, and no MINK source modification. Session 166 may begin `hookbox`.
