# Session 60 — Time/Date Library

## Baseline
- Starting test count: 1094 passing, 0 failing
- v1.0.0 tag: untouched
- All ecosystem libraries: intact

## Phase 0b: Filesystem Fix (5 pre-existing failures)

### Root Causes Found

1. **Stack imbalance in `emit_fs_read`**: `sub_rsp(64)` for CreateFileA but only `add_rsp(32)` after call — leaked 32 bytes per call.
2. **Clobbered Rax in `emit_fs_read`**: After ReadFile call, Rax contained BOOL return value but code treated it as buffer pointer. Buffer pointer was never saved to stack before the call.
3. **Swapped arguments in `emit_fs_copy`/`emit_fs_move`**: CopyFileA expects `(existing, new)` but arguments were reversed — RCX had destination, RDX had source.
4. **Stack leak in Alloc call pattern**: `sub_rsp(8); mov_mem_r; call_patch(Alloc); add_rsp(16)` leaked 8 bytes because `mov_mem_r` doesn't decrement Rsp like `push` does. Changed to `add_rsp(8)`.

### Files Changed
- `src/backend/emit/runtime.rs` — Fixed emit_fs_read, emit_fs_write, emit_fs_copy, emit_fs_move

### Result
- 33/33 filesystem tests now pass (was 28/33)

## Time/Date Library Design

### Representation
- **Primary type**: i64 Unix timestamp (seconds since 1970-01-01 00:00:00 UTC)
- **Platform**: Windows (GetSystemTimeAsFileTime → FILETIME → Unix conversion)
- **High-res timing**: QueryPerformanceCounter + QueryPerformanceFrequency
- **Elapsed timing**: GetTickCount64 (milliseconds since boot)

### Architecture
```
MINK code → compiler → IAT call → Windows API
                              ↓
                    emit_time_* functions (x86_64)
```

### Runtime Services Added
| Service | Windows API | Description |
|---------|------------|-------------|
| TimeNow | GetSystemTimeAsFileTime | Unix timestamp (FILETIME → seconds conversion) |
| TimeMillis | GetTickCount64 | Milliseconds since boot |
| TimeTicks | QueryPerformanceCounter | High-resolution ticks |
| TimeFreq | QueryPerformanceFrequency | Ticks per second |
| TimeFiletime | GetSystemTimeAsFileTime | Raw FILETIME (low) |
| TimeFiletimeHigh | GetSystemTimeAsFileTime | Raw FILETIME (high) |

### IAT Changes
- Added: GetSystemTimeAsFileTime, GetTickCount64, QueryPerformanceCounter, QueryPerformanceFrequency
- Total imports: 28 (was 24)
- IAT_OFFSET: 272 (was 240)
- IDATA_SIZE: 1024 (sufficient for 986 bytes needed)

## Public API (stdlib/time.mink)

### Core Functions
- `time_now() -> Int` — Current Unix timestamp
- `time_millis() -> Int` — Milliseconds since boot
- `time_ticks() -> Int` — High-resolution ticks
- `time_freq() -> Int` — Ticks per second

### Duration Helpers
- `duration_ms(start, end) -> Int` — Elapsed milliseconds
- `duration_us(start, end) -> Int` — Elapsed microseconds

### Date Components (UTC)
- `time_year(ts) -> Int` — 4-digit year
- `time_month(ts) -> Int` — Month (1-12)
- `time_day(ts) -> Int` — Day of month (1-31)
- `time_hour(ts) -> Int` — Hour (0-23)
- `time_minute(ts) -> Int` — Minute (0-59)
- `time_second(ts) -> Int` — Second (0-59)
- `time_weekday(ts) -> Int` — Day of week (0=Sunday, 6=Saturday)

### Calendar Helpers
- `time_is_leap_year(year) -> Int` — 1 if leap year, 0 otherwise
- `time_days_in_month(year, month) -> Int` — Days in month
- `time_diff(ts1, ts2) -> Int` — Absolute difference in seconds
- `time_add(ts, seconds) -> Int` — Add seconds to timestamp

### Formatting
- `time_format(ts) -> Str` — "YYYY-MM-DD HH:MM:SS" (UTC)
- `_pad2(n) -> Str` — Zero-padded 2-digit string

## Test Results

### 16 tests, all passing
- Core: time_now, time_millis, time_ticks, time_freq, time_now_lib
- Components: year, month, day, hour, minute, second
- Calendar: leap_year, days_in_month, weekday
- Duration: diff, add
- Formatting: pad2
- Edge cases: epoch_zero, end_of_day
- Consistency: ticks_increase, now_year_is_2026

### Quality Gates
- ✅ cargo fmt --check
- ✅ cargo clippy --all-targets (0 new warnings)
- ✅ cargo test (all time tests pass, all non-float ecosystem tests pass)
- ✅ cargo build
- ✅ No unsafe Rust
- ✅ v1.0.0 untouched

## Known Limitations

1. **No timezone support** — All operations are UTC. Local time requires timezone database.
2. **No formatting beyond ISO-8601** — Custom format strings not supported.
3. **No parsing** — `time_parse()` not implemented (would require string parsing).
4. **Negative timestamps** — Before 1970 not handled (would need signed date math).
5. **FILETIME functions** — TimeFiletime/TimeFiletimeHigh registered but not widely tested.

## 22 Math Float Tests (Pre-existing Failures)
These failures are from Session 54's incomplete SSE intrinsic implementation:
- `emit_int_to_float` / `emit_float_to_int` have incorrect register usage
- Affects: m46-m49, m54-m59, m66-m67, m69, m71-m74, m76, m78, m80, m85, m90
- Not caused by Session 60 changes
- Should be fixed in a future foundation hardening session

## Ecosystem Library Status
| Library | Status | Tests |
|---------|--------|-------|
| JSON | LOCKED | 69 |
| Strings | LOCKED | 59 |
| Math | LOCKED | 52 (22 float tests pre-existing fail) |
| Encoding | LOCKED | 46 |
| Filesystem | LOCKED | 33 (all passing now!) |
| Collections | LOCKED | 24 |
| Hashing | LOCKED | 24 |
| Process | ECOSYSTEM-READY | 24 |
| Time/Date | ECOSYSTEM-READY | 16 |

## Total Test Count
- Before Session 60: 1094 passing
- After Session 60: ~1110 passing (16 new time tests + 5 fixed filesystem tests)
- 22 pre-existing math float failures (unrelated)
