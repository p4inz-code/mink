# Milestone 2 — Windows practical-parity audit (Session 214)

Windows x86_64, 2026-11-08. Starting commit
`f85194750c694c0a77acabe1945c5f9d11434391` (Sessions 211–212). This session
audits whether M2 is still outstanding before doing any of its work, as
required.

## 1. Entry check: is M2 outstanding?

**No.** M2 was closed at Session 138
([`SESSION_138_M2_P9_FINAL_WINDOWS_GAP_AUDIT.md`](../implementation/SESSION_138_M2_P9_FINAL_WINDOWS_GAP_AUDIT.md)),
which revalidated the complete evidence set from Sessions 125–137, closed four
correctness defects (if-expression-in-operand compiler panic, `else if`
expression lowering, JSON serializer fixed base under an enlarged arena, JSON
parse write-pointer corruption), and declared M2 CLOSED after a release build
and representative release workloads passed.

Session 214 therefore did not rebuild the applications from scratch or invent
new ones. It audited the existing evidence, checked the current tree against the
M2 workload classes, and closed the one genuine gap the audit found: the M2
evidence was produced by the **debug** compiler, while all subsequent work
(including the release verification in Session 213) was validated on the release
binary.

## 2. M2 evidence audit

| Workload class (M2 stopping criteria) | Evidence | Status |
| --- | --- | --- |
| CLI utility end to end | `a1_fileinspect`, `a3_textgrep`, `h_manifest`; Sessions 129–138 | COMPLETE |
| Filesystem automation | `a2_treeops`; Session 129 (P1) | COMPLETE |
| Text and regex processing | `a3_textgrep` (`re` library); Session 129 | COMPLETE |
| JSON parsing and serialization | `c_json`; Session 138 JSON arena fix | COMPLETE |
| CSV parsing and aggregation | `b_csvdata` | COMPLETE |
| SQLite persistence | `d_sqlite`; Session 136 | COMPLETE |
| Large-file processing | Session 136 (P8); `tests/large_file.rs` (4 GiB sparse) | COMPLETE |
| Subprocess control and exit codes | `f_process`; Sessions 130–131 (P2–P3) | COMPLETE |
| HTTP/HTTPS client | `e_network`; Sessions 132–133 (P4–P5) | COMPLETE |
| TCP sockets | `e_network`, `network_lib`; Session 133 | COMPLETE |
| UDP sockets | `e_network`; Session 133 | COMPLETE |
| HTTP server and concurrency | Session 134 (P6); `httpserver_lib`; M5 `hookbox` | COMPLETE |
| Threads and shared state | `g_threads`; `threads_lib` | COMPLETE |
| Resource scaling and limits | Session 132 (P4); `capacity`, `resource_lifecycle` | COMPLETE |
| Build artefact and packaging | `h_manifest`, `zip_lib`, `release` suite | COMPLETE |
| Documented failure behaviour | all 228 cases include error-path comparisons | COMPLETE |
| Transparent limitations | README and audit limitation sections | COMPLETE |

No workload class is missing evidence. No new application is warranted by the
plan's own stopping criterion.

## 3. Gap closed: release-compiler replay

The ten validation applications were rebuilt with the Session 213 release
compiler (`target/s213_clean/release/mink.exe`, 4,048,384 bytes) rather than the
debug binary, then compared against their unchanged Python references.

| Application | Cases | Result |
| --- | ---: | --- |
| `a1_fileinspect` | 20 | 0/20 differ |
| `a2_treeops` | 18 | 18/18 match |
| `a3_textgrep` | 35 | 35/35 match |
| `b_csvdata` | 34 | 34/34 match |
| `c_json` | 43 | 43/43 match |
| `d_sqlite` | 10 | 10/10 match |
| `e_network` | 18 | 18/18 match |
| `f_process` | 19 | 19/19 match |
| `g_threads` | 18 | 18/18 match |
| `h_manifest` | 13 | 13/13 match |
| **Total** | **228** | **228/228 match, 0 differences** |

All ten builds exited 0 with no crash markers. Byte-level stdout/stderr
comparison and exit-code equality are enforced by each `verify.py`; no tolerance
was added and no case was removed. This confirms that the M2 practical-parity
evidence holds for the optimized compiler, not only for debug.

## 4. Defects found

None in MINK source. The only fix in this session's scope was the test-harness
path defect already corrected in Session 212 (`encoding_lib`, `math_lib`), which
had the same practical consequence for release-mode validation: those suites
would have exercised the debug compiler. Verified 75/75 and 106/106 in both
profiles.

## 5. Limitations (unchanged, documented)

- 1 GiB arena and `MAX_LIVE_ALLOCS` ceiling; exhaustion is a documented failure, not silent success.
- ANSI Windows paths and `MAX_PATH`-length constraints; Unicode path handling is not claimed.
- No console control-handler API; no thread pool, condition variables or dynamic thread prioritisation.
- SQLite performance is not benchmarked against native SQLite; only correctness and practical workloads are claimed.
- Soak coverage is bounded (60 s runs and the 600 s opt-in soak), not a long-duration guarantee.
- No claim of superiority over other languages on any M2 workload.

## 6. Verdict

**WINDOWS PRACTICAL PARITY (M2): PASS.** The milestone was already complete at
Session 138; this audit revalidated the evidence, confirmed every workload class
is covered, and extended the practical workload matrix to the release compiler
with 228/228 matching cases.

This is not external developer validation (M6 remains outstanding), and it does
not authorize publication: 1.0.5 remains unpublished with no tag or release.
