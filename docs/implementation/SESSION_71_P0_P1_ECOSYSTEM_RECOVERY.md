# SESSION 71 — P0/P1 CRASH AND CORRECTNESS RECOVERY

**Date:** 2026-08-30
**Branch:** `main`
**HEAD:** `528a964`

---

## 1. Starting Baseline

| Metric | Value |
|--------|-------|
| HEAD | `ac49c01` |
| Tests | 2394 passed, 37 failed, 1 ignored |
| Failing suites | collections_lib (2), crypto_lib (8), filesystem_lib (10), process_lib (16), random_lib (1) |

## 2. Session 71 Results

### VecRemove (P0) — FIXED

**Root cause:** `emit_vec_remove` returned the removed element value instead of the data pointer. The test pattern `v = rt_vec_remove(v, i)` assigns the result back to v, then calls `rt_vec_len(v)` treating the element value as a pointer → ACCESS_VIOLATION.

**Fix:** Changed return value from `mov_r_mem(Rax, Rbp, -16)` (removed element) to `mov_r_mem(Rax, Rbp, -8)` (data pointer).

**Commit:** `00a6a09`

**Result:** collections_lib 24/24 (was 22/24)

### Crypto Random (P0) — FIXED

**Root cause:** `emit_crypto_init` loaded `bcryptprimitives.dll` but `BCryptGenRandom` is exported from `bcrypt.dll`. `GetProcAddress("BCryptGenRandom")` returned NULL → crypto_init returned -1 → all random operations crashed when calling through NULL function pointer.

**Verification:** `dumpbin /exports bcryptprimitives.dll` shows 11 exports, none named BCryptGenRandom. `dumpbin /exports bcrypt.dll` shows BCryptGenRandom at ordinal 32.

**Fix:** Changed DLL name from "bcryptprimitives.dll" to "bcrypt.dll" in emit_crypto_init.

**Commit:** `528a964`

**Result:** crypto_lib random tests now pass (r01-r05 all pass).

### HMAC (P1) — FIXED

**Root cause:** Test h03 called `rt_str_free(key_bytes)` after `_raw_to_hex(key_bytes)` had already consumed (moved) key_bytes, violating MINK ownership rules → E-S10 compiler error → BUILD FAILED.

**Fix:** Removed the double-free `rt_str_free(key_bytes)` from the test code.

**Commit:** `528a964`

**Result:** h03 HMAC test now passes.

## 3. Test Matrix

| Suite | Before | After | Change |
|-------|--------|-------|--------|
| collections_lib | 22/24 | 24/24 | +2 |
| crypto_lib | 10/18 | 16/18 | +6 |
| **Total** | **2394 pass, 37 fail** | **2402 pass, 29 fail** | **+8 pass, -8 fail** |

## 4. Remaining Failures (29)

| Suite | Failures | Root Cause | Priority |
|-------|----------|------------|----------|
| crypto_lib | 2 | HKDF pointer-operation crashes in MINK code | P1 |
| filesystem_lib | 10 | Stub implementations return dummy values | P2 |
| process_lib | 16 | Stub implementations return dummy values | P2 |
| random_lib | 1 | seed=0 edge case | P3 |
