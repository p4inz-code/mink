# Session 175 — hookbox contract closure and M5 re-verification

**Date:** 2026-11-04
**Status:** **COMPLETE** (M5 closure session; M6 Session 175 installation audit
is NOT STARTED and deferred)
**Starting HEAD:** `aeceee1` (JSON arena capacity fix)
**Scope:** verify that the Session 174 fix closes the frozen hookbox contract,
and re-run the M5 evidence set.

## 1. Contract clause that was open

Selection section 6: `POST /hooks` body must be a JSON object of at most 8,192
bytes and must be persisted as an event.

Before Session 174, valid bodies of 4,097-8,192 bytes returned `400`. After the
fix, the application passes its real arena capacity to `json_parse_in` and
accepts the full frozen range. Bodies above 8,192 bytes still return `413`.

## 2. Boundary verification

`python benchmarks/m5/hookbox/tests/driver.py --no-build`:

| Case | Result | Evidence |
| --- | --- | --- |
| 4,096-byte object | 201 | exact `{"seq":1}` and exact event file |
| 4,097-byte object | 201 | exact event file (oracle-compared) |
| 8,192-byte object | 201 | exact event file (oracle-compared) |
| 8,193-byte body | 413 | no event file |
| declared length over 8,192 | 413 | no event file |

All other boundary cases (limit 1/20, sequence recovery, 25 events) still pass.

## 3. Full re-verification

| Suite | Result | Runs | Exit |
| --- | --- | ---: | ---: |
| `hookbox` | 60 passed, 0 failed, 0 skipped | 3 consecutive | 0 |
| `repoaudit` | 50 passed, 0 failed, 0 skipped | 1 | 0 |
| `releasepack` | 53 passed, 0 failed, 0 skipped | 1 | 0 |
| `tests/json.rs` (stdlib) | 68 passed, 0 failed | 1 | 0 |

Total M5 checks per full run: **163** (50 + 60 + 53), all passing. The 68 JSON
tests are standard-library regressions and are not added to the application
check count.

The hookbox suite's own determinism case still produces byte-identical stdout
(port normalized) and event files across three independent runs, and its
standalone case still executes the copied executable from outside the
repository.

## 4. Artifact

| Item | Value |
| --- | --- |
| Size | 231,424 bytes |
| SHA-256 | `a7c4be44a67f7f0dbb54372e4386fd6b862a842f3593d83e0a00add078a48e25` |
| Build | `target/release/mink.exe build benchmarks/m5/hookbox/app/main.mink` |
| Outside-tree execution | PASS (suite case) |

The binary grew by 512 bytes relative to the pre-fix artifact
(230,912 bytes), consistent with the added capacity-aware entry point.

## 5. Contract status

| Frozen area | Status |
| --- | --- |
| CLI, help, exit codes 0-4 | GREEN |
| `POST /hooks` for all valid objects up to 8,192 bytes | GREEN |
| `GET /health` | GREEN |
| `GET /events` | GREEN |
| 404 / 405 / 413 / 500 behavior | GREEN |
| Event naming, bytes, recovery, determinism | GREEN |
| Inbox creation/writability/cleanup | GREEN |
| Standalone Windows executable | GREEN |

**hookbox: GREEN.** The earlier APPLICATION GREEN / CONTRACT PARTIAL label is
superseded by this session and by the revised readiness gate.

## 6. Remaining limitations (unchanged, non-contractual)

Sequential single-connection HTTP; Content-Length only; no keep-alive, chunked
requests, TLS or Ctrl+C handler; ASCII path scope; Windows case-insensitive
event names. None is required by the frozen contract.

## 7. Reproducible commands

```bash
target/release/mink.exe build benchmarks/m5/hookbox/app/main.mink
mkdir -p benchmarks/m5/hookbox/build
cp benchmarks/m5/hookbox/app/main.exe benchmarks/m5/hookbox/build/hookbox.exe
python benchmarks/m5/hookbox/tests/driver.py --no-build
python benchmarks/m5/repoaudit/tests/run_tests.py --no-build
python benchmarks/m5/releasepack/tests/driver.py --no-build
cargo test --test json
```
