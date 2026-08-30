# SESSION 58 — HASHING LIBRARY + FOUNDATION HARDENING

## Objective
Build the MINK Hashing library as a strong foundation for authentication, HTTP, package management, caching, serialization, and future cryptography.

## Status: ECOSYSTEM-READY

## API Implemented

### Non-Cryptographic Hashes
- `hash_fnv1a(s: Str) -> Int` — FNV-1a hash, 64-bit wrapping arithmetic
- `hash_djb2(s: Str) -> Int` — djb2 hash by Dan Bernstein
- `hash_combine(a: Int, b: Int) -> Int` — boost::hash_combine style

### Cryptographic Hash
- `hash_sha256(s: Str) -> Str` — SHA-256 digest as 64-char lowercase hex string

### Hex Encoding
- `hex_encode_byte(b: Int) -> Str` — 2-char hex of a byte

### Comparison
- `hash_digest_equal(a: Str, b: Str) -> Bool` — constant-time comparison

## Architecture Decision

### SHA-256: Pure MINK vs Runtime Intrinsic

**Approach A (considered):** Runtime intrinsic with x86_64 machine code emit
- Pros: Fast, compact
- Cons: ~2000 lines of emit code, register pressure, not portable, not auditable in MINK source

**Approach B (chosen):** Pure MINK implementation using bitwise ops
- Pros: Portable, auditable in MINK source, no runtime changes, cross-platform by design, proves language capability
- Cons: Slower (~3x), more verbose

**Decision:** Pure MINK. SHA-256 in emit code was attempted and abandoned due to extreme register pressure (only 12 available registers for 8 working vars + temps + loop counter + workspace ptr). The pure MINK approach is the correct architectural choice because:
1. Zero runtime changes — no risk of destabilizing existing libraries
2. Fully auditable — every operation visible in MINK source
3. Portable — works on any future MINK target
4. AI-friendly — agents can read and verify the implementation
5. Demonstrates MINK's capability for complex algorithms

## Key Findings

### 64-bit vs 32-bit Arithmetic
MINK Int is 64-bit signed. SHA-256 requires 32-bit modular arithmetic. The `~` (bitwise NOT) operator produces 64-bit results, creating garbage in upper 32 bits. Left shifts on 64-bit produce different results than 32-bit.

**Solution:** Every intermediate value must be masked with `& 0xFFFFFFFF` after each shift, rotation, and boolean operation. A `_rot32()` helper function was created to encapsulate 32-bit rotation with proper masking.

### Short-Circuit Evaluation Impact (Session 57)
The `&&` and `||` operators were desugared into control flow in Session 57. This fixed a pre-existing test (`comparisons_and_logical_lower` in `tests/backend.rs`) that expected `And`/`Or` binary ops in the backend IR.

## Files Changed

| File | Change |
|------|--------|
| stdlib/hashing.mink | New: 350 lines, 6 functions |
| tests/hashing_lib.rs | New: 300 lines, 24 tests |
| tests/backend.rs | Fix: update `comparisons_and_logical_lower` for short-circuit desugaring |

## Test Results

| Suite | Tests | Status |
|-------|-------|--------|
| hashing_lib | 24 | ALL PASS |
| json | 37 | ALL PASS |
| strings_lib | 73 | ALL PASS |
| math_lib | 106 | ALL PASS |
| encoding_lib | 57 | ALL PASS |
| filesystem_lib | 33 | ALL PASS |
| collections_lib | 24 | ALL PASS |
| All other suites | 802 | ALL PASS |
| **Total** | **1156** | **ALL PASS** |

## SHA-256 Test Vectors Verified
- SHA-256("") = e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855 ✓
- SHA-256("a") = ca978112ca1bbdcafac231b39a23dc4da786eff8147c4e72b9807785afee48bb ✓
- SHA-256("abc") = ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad ✓
- SHA-256("hello world") = b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9 ✓
- SHA-256("1234567890") = c775e7b757ede630cd0aa1113bd102661ab38829ca52a6422ab782862f268646 ✓
- SHA-256(63×'a') = 7d3e74a05d7db15bce4ad9ec0658ea98e3f06eeecf16b4c6fff2da457ddc2f34 ✓
- SHA-256("abcdbcde...nopq") [multi-block] = 248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1 ✓

## Security Analysis

### Bounds Checking
- All string operations use `rt_str_byte` with bounds checking
- All memory operations use `rt_mem_store`/`rt_mem_load` with validation
- No raw pointer arithmetic outside workspace

### Memory Safety
- Workspace allocated with `rt_alloc`, freed with `rt_free`
- No use-after-free: workspace is freed after hex conversion
- No double-free: single `rt_free(ws)` at end
- No leaks: all allocations paired with frees

### Deterministic Behavior
- SHA-256 output is fully deterministic for same input
- No random or platform-dependent behavior
- No global state

## Limitations

1. **Performance:** Pure MINK SHA-256 is ~3x slower than native C. Acceptable for V1; can be optimized to runtime intrinsic later.
2. **64-bit wrapping:** FNV-1a and djb2 use 64-bit wrapping, not traditional 32-bit. This is a deliberate MINK choice.
3. **No streaming:** SHA-256 requires the entire input in memory. Streaming API deferred to future sessions.
4. **No password hashing:** SHA-256 is NOT suitable for password storage. bcrypt/argon2 deferred to cryptography library.

## Ecosystem Library Status

| Library | Status |
|---------|--------|
| JSON | LOCKED |
| Strings | LOCKED |
| Math | LOCKED |
| Encoding | LOCKED |
| Filesystem | LOCKED |
| Collections | LOCKED |
| **Hashing** | **ECOSYSTEM-READY** |

## Recommendations for Session 59

### Next Library
Per dependency graph: **Process** (requires Filesystem + Hashing for checksums). Alternatively: **Random** (small, high-value, enables testing frameworks).

### Foundation Priority
1. Dynamic allocation (1 MiB fixed heap is increasingly limiting)
2. User function wrapper stack corruption fix
3. Module system preparation
