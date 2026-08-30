# Session 61 — Random Library + Math Regression Fix

## Executive Summary

**Math regression: FIXED.** 22 float test failures caused by swapped `movq` SSE instructions in `emit_int_to_float` and `emit_float_to_int`. Single root cause — both functions had the xmm↔rax direction reversed.

**Random library: IMPLEMENTED.** xorshift64* PRNG with seed, next, bounded int, bool, byte, choice. 15 tests, all passing.

**BSS alignment bug: FIXED.** Arena must be at 16-byte aligned offset. Adding the PRNG state slot required shifting arena from 1472→1488 (skipping 1480 which is not 16-byte aligned).

## Math Regression Root Cause

### Bug: Swapped movq directions in float conversion

**emit_int_to_float:**
```
cvtsi2sd xmm0, rax    // xmm0 = (double)rax  ✓
movq xmm0, rax         // xmm0 = rax (OVERWRITES xmm0!) ✗
```
Should be: `movq rax, xmm0` (move float bits from xmm0 to rax for return).

**emit_float_to_int:**
```
movq rax, xmm0         // rax = xmm0 (reads FROM xmm0 which is empty!) ✗
cvttsd2si rax, xmm0    // converts empty xmm0
```
Should be: `movq xmm0, rax` (load float bits from rax into xmm0 for conversion).

### Why it wasn't caught earlier
- The `movq_xmm0_rax` method emits `0F 6E` (movq xmm0, rax = GPR→XMM)
- The `movq_rax_xmm0` method emits `0F 7E` (movq rax, xmm0 = XMM→GPR)
- Names are correct but usage was reversed in both functions
- Only affected IntToFloat/FloatToInt runtime services; float binary ops (div, mul, etc.) use `load_float` which is correct

### Tests fixed: 22
All float-related math tests now pass: m46-m49, m54-m59, m66-m67, m69, m71-m74, m76, m78, m80, m85, m90

## Random Library

### Algorithm: xorshift64*
- Period: 2^64 - 1
- State: 64-bit nonzero integer
- Steps: state ^= state >> 12; state ^= state << 25; state ^= state >> 27; return state * 0x2545F4914F6CDD1D
- NOT cryptographically secure (documented)

### BSS Layout Change
- Added 8-byte PRNG state at offset 1472
- Arena shifted from 1472 → 1488 (16-byte aligned)
- Table and size updated accordingly

### API
| Function | Description |
|----------|-------------|
| `rt_random_seed(seed)` | Set PRNG state (0→1) |
| `rt_random_next() -> Int` | Next 64-bit random value |
| `random_seed(seed)` | Wrapper ensuring nonzero |
| `random_int(min, max) -> Int` | Bounded random integer |
| `random_bool() -> Int` | 0 or 1 |
| `random_byte() -> Int` | 0-255 |
| `random_choice(count) -> Int` | Random index [0, count) |

### Tests: 15 passing
- Seed reproducibility
- Different seeds → different sequences
- Seed 0 treated as 1
- Full-range nonzero values
- Bounded integers in range
- Single-element ranges
- Bool values (0/1 only)
- Byte range (0-255)
- Choice range
- Uniform distribution (statistical)
- Bit distribution (statistical)
- Deterministic sequence
- Large/negative seeds
- 10000 generations without crash

## Quality Gates
- ✅ cargo fmt --check
- ✅ cargo clippy --all-targets (0 new warnings)
- ✅ cargo test — 1491 passing, 21 failing (all pre-existing process_lib)
- ✅ No unsafe Rust
- ✅ v1.0.0 untouched

## Ecosystem Library Status
| Library | Status | Tests |
|---------|--------|-------|
| JSON | LOCKED | 37 |
| Strings | LOCKED | 63 |
| Math | LOCKED | 106 (22 float tests FIXED) |
| Encoding | LOCKED | 57 |
| Filesystem | LOCKED | 33 |
| Collections | LOCKED | 24 |
| Hashing | LOCKED | 24 |
| Process | ECOSYSTEM-READY | 4/25 (21 pre-existing failures) |
| Time/Date | LOCKED | 16 |
| Random | ECOSYSTEM-READY | 15 |

## Remaining Items
- 21 process_lib failures (Session 59 emit_process_run was never completed)
- No dynamic allocation (arena sufficient for current libraries)
- Process output capture stubbed
- No timezone support in Time/Date
