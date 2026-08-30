# MINK C ABI Specification

**Version:** 0.1.0 (Design Draft)
**Date:** August 25, 2026
**Status:** DESIGN ONLY — not implemented
**Scope:** Stable C ABI as the universal FFI boundary for MINK

---

## 1. Goals

1. **Universal interop.** Every major language (Python, C++, C#, Rust, Go, Java) has C FFI support. A stable C ABI is the universal adapter that lets MINK work with all of them without N language-specific bridges.

2. **Zero assumptions.** The C ABI must work regardless of MINK's internal calling convention, heap model, or ownership semantics. C sees only flat, blittable types and function pointers.

3. **V1.x stability.** Once shipped, the C ABI must not break. New types and calling conventions can be added, but existing signatures must remain valid across compiler versions.

4. **Cross-platform.** The ABI maps to the platform's C calling convention (System V AMD64 on Linux/macOS, Microsoft x64 on Windows).

5. **Safety at the boundary.** MINK's ownership model cannot cross the C ABI. The ABI spec explicitly defines ownership transfer rules and safety contracts.

---

## 2. Primitive Type Mapping

| MINK Type | C Type | Size | Alignment | Notes |
|-----------|--------|------|-----------|-------|
| `Int` | `int64_t` | 8 bytes | 8 | Signed 64-bit integer |
| `Bool` | `uint8_t` | 1 byte | 1 | 0 = false, 1 = true |
| `Float` | `double` | 8 bytes | 8 | IEEE 754 double-precision |
| `Char` | `uint8_t` | 1 byte | 1 | Single UTF-8 byte (Unicode scalar ≤ 0x7F in V1) |
| `()` | `void` | 0 | N/A | No return value |

### V1 Limitations
- `Char` is byte-sized in V1. Unicode support (multi-byte codepoints) is deferred.
- `Int` is always 64-bit signed. No unsigned integer type in V1.
- `Float` is always 64-bit double-precision. No 32-bit float in V1.

### V2+ Extensions (not yet designed)
- `u8`, `u16`, `u32`, `u64`, `i8`, `i16`, `i32` — sized integers
- `f32` — single-precision float
- `char` — 4-byte Unicode codepoint

---

## 3. Pointer Types

| MINK Type | C Type | Size | Notes |
|-----------|--------|------|-------|
| `Ptr<T>` | `T*` | 8 bytes (x86_64) | Raw pointer, no ownership semantics |
| `&T` | `T*` | 8 bytes | Borrowed pointer (read-only contract) |
| `&mut T` | `T*` | 8 bytes | Exclusive pointer (read-write contract) |

**Critical rule:** C does not distinguish between `Ptr<T>`, `&T`, and `&mut T`. All three are `T*` in C. The distinction is a compile-time contract in MINK, not an ABI-level distinction.

---

## 4. String Representation

MINK strings cross the C ABI as a **fat pointer struct**:

```c
typedef struct {
    uint64_t len;       // byte length
    const uint8_t *ptr; // pointer to UTF-8 bytes
} MinkStr;
```

### Layout guarantees
- `len` is at offset 0, `ptr` is at offset 8
- Total size: 16 bytes on x86_64
- Alignment: 8 bytes
- `ptr` points to valid UTF-8 memory for `len` bytes

### Ownership rules
- **MINK → C:** The C caller receives a read-only view. The MINK string remains valid until the MINK side releases it. C must NOT free the pointer.
- **C → MINK:** The C caller must provide a valid `MinkStr`. MINK takes ownership and will free the memory using its allocator.
- **Shared lifetime:** Both sides agree on a lifetime contract. The C caller must not access the string after the agreed-upon lifetime ends.

### V1 string constraints
- Strings are immutable after creation in V1 (no `rt_str_set_byte` across ABI)
- Heap strings are length-prefixed internally; the `MinkStr` struct is the ABI representation
- Image literals (read-only, in `.rdata`) are valid as long as the MINK image is loaded

---

## 5. Struct Representation

MINK structs cross the C ABI as **C-compatible structs** with the same layout:

```mink
struct Point {
    x: Int,
    y: Int,
}
```

Becomes in C:

```c
typedef struct {
    int64_t x;
    int64_t y;
} MinkPoint;
```

### Layout rules
- Fields are laid out in declaration order
- Each field is aligned to its natural alignment
- Structs are padded to the alignment of the largest field
- **Blittable only:** Structs containing `Str`, `Vec<T>`, or other heap types cannot cross the ABI directly (they require wrapper functions)

### Alignment table

| C Type | Alignment |
|--------|-----------|
| `uint8_t`, `int8_t` | 1 |
| `uint16_t`, `int16_t` | 2 |
| `uint32_t`, `int32_t` | 4 |
| `uint64_t`, `int64_t`, `double`, pointers | 8 |

### V1 restriction
- Only structs with `Int`, `Bool`, `Float`, `Char`, and pointer fields are blittable
- Nested structs are supported (recursive layout)
- Enum fields are represented as `int64_t` (the discriminant)

---

## 6. Array Representation

MINK arrays cross the C ABI as a **fat pointer struct**:

```c
typedef struct {
    uint64_t len;       // element count
    const void *ptr;    // pointer to elements
} MinkSlice;
```

### For typed arrays

```c
typedef struct {
    uint64_t len;
    const int64_t *ptr;
} MinkSliceInt;
```

### Ownership rules
- **MINK → C:** Read-only view. C must not modify or free the data.
- **C → MINK:** C provides a valid pointer and length. MINK copies the data into its own allocator.
- **Zero-copy (read-only):** Both sides agree on a lifetime contract. C must not access after the contract ends.

---

## 7. Vec<T> Representation

MINK `Vec<T>` crosses the C ABI as:

```c
typedef struct {
    uint64_t capacity;
    uint64_t length;
    const void *elements;  // pointer to elements
} MinkVec;
```

### Ownership rules
- **MINK → C:** Read-only view. C must not modify or free.
- **C → MINK:** MINK takes ownership and copies data into its own allocator.
- **No zero-copy for Vec across ABI** (V1): MINK's Vec uses its own allocator; C cannot free MINK memory.

---

## 8. Enum Representation

MINK enums cross the C ABI as `int64_t` discriminants:

```c
// MINK: enum Color { Red, Green, Blue }
typedef int64_t MinkColor;
// MinkColor_Red = 0, MinkColor_Green = 1, MinkColor_Blue = 2
```

### Payloaded enums (V2+)

When enums carry data, the ABI representation will be:

```c
typedef struct {
    int64_t tag;
    union {
        int64_t int_val;
        MinkStr str_val;
        // ... other payloads
    } payload;
} MinkOptionInt;
```

**V1 limitation:** Only unit enums (no payload) cross the ABI. `Option<T>` and `Result<T,E>` are compiler-provided types that do not cross the ABI in V1.

---

## 9. Result/Error Representation

### V1 approach
- MINK functions that can fail return `Result<T, E>` internally
- At the C ABI boundary, errors are represented as:
  - Return code (`int64_t`): 0 = success, non-zero = error code
  - Out-parameter for the result value

```c
// MINK: fn divide(a: Int, b: Int) -> Result<Int, Int>
int64_t mink_divide(int64_t a, int64_t b, int64_t *result_out) {
    // returns 0 on success, error code on failure
    // writes result to *result_out on success
}
```

### V2+ approach
- Direct `Result<T, E>` return with tagged union representation
- Panic-to-error translation at the boundary

---

## 10. Ownership Across the ABI Boundary

### Fundamental rule
**MINK ownership does not cross the C ABI.** C has no concept of ownership. The ABI defines explicit transfer protocols.

### Caller/callee ownership model

| Scenario | Who allocates? | Who frees? | Lifetime |
|----------|---------------|------------|----------|
| MINK returns `Int` | N/A (value type) | N/A | N/A |
| MINK returns `MinkStr` | MINK | MINK (caller must not free) | Until MINK releases |
| C passes `MinkStr` to MINK | C | MINK (takes ownership) | Until MINK releases |
| MINK passes `Ptr<T>` to C | MINK | MINK | Until MINK releases |
| C passes `Ptr<T>` to MINK | C | C | Until C releases |

### Rules
1. **Value types** (`Int`, `Bool`, `Float`, `Char`) are always copied. No ownership concerns.
2. **Pointer types** are borrowed across the ABI. The pointer is valid only for the agreed lifetime.
3. **String types** follow explicit ownership transfer: the allocator side owns and frees.
4. **C must never call `rt_free`** on MINK-allocated memory. MINK must never free C-allocated memory.
5. **Shared allocators** are possible in V2+ but not in V1.

---

## 11. Borrowing Across the ABI Boundary

### V1 contract
- MINK `&T` → C `const T*`: C receives a read-only pointer. Valid for the agreed lifetime.
- MINK `&mut T` → C `T*`: C receives a read-write pointer. MINK guarantees exclusive access for the agreed lifetime.
- C → MINK borrowed data: MINK treats it as `Ptr<T>` (untyped borrow). No compile-time lifetime checking.

### Lifetime documentation
Since C has no lifetime annotations, the ABI spec must document the expected lifetime for each function:

```c
/// Computes the sum of elements in `data`.
/// `data.ptr` must remain valid for the duration of this call.
/// `data` is read-only; this function does not modify it.
int64_t mink_sum(MinkSliceInt data);
```

---

## 12. Allocation/Free Rules

### V1 rules
1. **MINK allocates with its own allocator** (`rt_alloc`/`rt_str_alloc`).
2. **C allocates with its own allocator** (`malloc`/`free`).
3. **No cross-allocator freeing.** MINK cannot `free()` C memory. C cannot `rt_free()` MINK memory.
4. **String transfer:** When C passes a `MinkStr` to MINK, MINK copies the bytes into its own allocator and the C caller retains ownership of the original.
5. **Return values:** MINK functions that return `MinkStr` return a MINK-allocated string. The C caller must call `mink_str_free()` (a C-exported function) to release it.

### Exported deallocation functions

```c
/// Free a MINK-allocated string.
void mink_str_free(MinkStr s);

/// Free a MINK-allocated Vec.
void mink_vec_free(MinkVec v);
```

These are the ONLY C-callable deallocation functions. C code must use them instead of `free()`.

---

## 13. Calling Convention

### Platform-specific conventions

| Platform | Convention | Argument passing | Return value | Callee-saved |
|----------|------------|-----------------|--------------|--------------|
| Windows x86_64 | Microsoft x64 | RCX, RDX, R8, R9 (then stack) | RAX | RBX, RBP, RDI, RSI, RSP, R12-R15 |
| Linux x86_64 | System V AMD64 | RDI, RSI, RDX, RCX, R8, R9 (then stack) | RAX | RBX, RBP, R12-R15 |
| macOS ARM64 | AAPCS64 | X0-X7 | X0 | X19-X28 |
| Linux ARM64 | AAPCS64 | X0-X7 | X0 | X19-X28 |

### V1 calling convention
MINK currently uses a custom stack-based convention:
- Arguments pushed rightmost-first, one 64-bit word each
- Callee reads at `[rbp + 16 + 8 * i]`
- Result in `rax`
- Stack 16-byte aligned at every `call`

### C ABI calling convention
Functions exported with `#[export]` must use the **platform's C calling convention**, not MINK's internal convention. This requires the backend to emit a wrapper that:
1. Receives arguments in C calling convention registers
2. Translates to MINK's internal convention (or directly uses C convention for exported functions)
3. Returns the result in the C convention

### V1 implementation note
For V1, exported functions will use the C calling convention directly (no wrapper needed if the backend emits C-convention code for exported functions). The MINK internal convention remains for MINK-to-MINK calls.

---

## 14. Symbol Naming

### Exported functions
- MINK function `add` → C symbol `mink_add`
- MINK function `my_module::compute` → C symbol `mink_my_module_compute`
- All exported symbols are prefixed with `mink_`
- No name mangling (plain C linkage)

### Naming rules
- Lowercase with underscores
- No special characters
- Package name is part of the symbol prefix for disambiguation
- Version suffix for ABI versioning: `mink_add@1` (V1 ABI)

### Symbol format
```
mink_<module_path>_<function_name>@<abi_version>
```

Examples:
- `mink_main_add@1`
- `mink_math_sqrt@1`
- `mink_net_connect@1`

---

## 15. Exported Functions

### MINK source syntax

```mink
#[export]
fn add(a: Int, b: Int) -> Int {
    return a + b;
}
```

### Generated C header

```c
#ifndef MINK_MAIN_H
#define MINK_MAIN_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/// Adds two integers.
int64_t mink_main_add(int64_t a, int64_t b);

#ifdef __cplusplus
}
#endif

#endif /* MINK_MAIN_H */
```

### Export rules
1. Only functions with `#[export]` attribute are visible to C
2. Exported functions must have C-compatible parameter types (primitives, pointers, blittable structs)
3. Exported functions must not use MINK-specific types (`Option<T>`, `Result<T,E>`, closures) in their signatures
4. Exported functions must not panic (panic across ABI is undefined behavior)

---

## 16. Imported Functions

### MINK source syntax

```mink
extern "C" fn printf(format: Ptr<Char>, ...) -> Int;
extern "C" fn malloc(size: Int) -> Ptr<Int>;
extern "C" fn free(ptr: Ptr<Int>);
```

### Import rules
1. `extern "C"` declares a function defined in external C code
2. The function must be linked at build time (static or dynamic)
3. Parameters and return types must be C-compatible
4. Variadic functions use `...` (V1: limited to the last parameter)

---

## 17. ABI Stability and Versioning

### Version scheme
- ABI version is independent of compiler version
- Format: `MAJOR.MINOR`
- `MAJOR` bump = breaking ABI change (all consumers must recompile)
- `MINOR` bump = additive change (existing consumers continue working)

### V1 ABI version: `1.0`
- The initial stable ABI
- All V1.x compiler releases will produce V1-compatible binaries
- Breaking changes (if any) require ABI version `2.0`

### ABI version in executable header
- The MINK executable header includes the ABI version
- Runtime checks ABI version at startup
- Version mismatch → structured error and exit

### Stability guarantees
1. Once a function signature is exported, it must remain valid across V1.x releases
2. New types and functions can be added in minor versions
3. The `MinkStr` struct layout is frozen in V1.0
4. The calling convention is frozen per platform
5. Symbol naming convention is frozen

---

## 18. Layout Guarantees

### Struct layout
- Packed structs: NOT supported (alignment must match C rules)
- `#[repr(C)]` equivalent: all MINK structs exported to C use C-compatible layout
- Field order = declaration order
- Padding inserted for alignment

### Platform-specific sizes

| Type | x86_64 | ARM64 |
|------|--------|-------|
| `Int` (int64_t) | 8 bytes | 8 bytes |
| `Bool` (uint8_t) | 1 byte | 1 byte |
| `Float` (double) | 8 bytes | 8 bytes |
| `Char` (uint8_t) | 1 byte | 1 byte |
| Pointer | 8 bytes | 8 bytes |
| `MinkStr` | 16 bytes | 16 bytes |

### Endianness
- x86_64: little-endian
- ARM64: little-endian
- V1: all targets are little-endian
- V2+: big-endian support if needed

---

## 19. Platform Differences

| Aspect | Windows x86_64 | Linux x86_64 | macOS ARM64 |
|--------|----------------|--------------|-------------|
| Calling convention | Microsoft x64 | System V AMD64 | AAPCS64 |
| Name decoration | `name` (no mangling) | `name` (no mangling) | `name` (no mangling) |
| Dynamic library | `.dll` | `.so` | `.dylib` |
| Symbol visibility | All exported by default | Hidden by default, `__attribute__((visibility("default")))` | Hidden by default, `__attribute__((visibility("default")))` |
| Exception handling | Structured Exception Handling | setjmp/longjmp | setjmp/longjmp |
| Thread-local storage | `__declspec(thread)` | `__thread` | `_Thread_local` |

### V1 considerations
- Only Windows x86_64 is implemented
- Linux and macOS will follow the same ABI rules with platform-specific calling conventions
- Symbol visibility must be explicitly managed on ELF/Mach-O targets

---

## 20. Safety Boundaries

### What the ABI guarantees
1. Type-safe data transfer for primitive types
2. Struct layout compatibility (blittable types)
3. Ownership transfer protocols
4. Lifetime documentation for borrowed data

### What the ABI does NOT guarantee
1. **Lifetime safety across the boundary.** C can hold a pointer past its valid lifetime.
2. **Thread safety.** C and MINK can race on shared data. Synchronization is the caller's responsibility.
3. **Panic safety.** MINK panics across the ABI boundary are undefined behavior.
4. **Exception safety.** C exceptions must not cross into MINK. MINK errors must not cross into C as exceptions.

### Safety rules for consumers
1. C code must not dereference a `MinkStr.ptr` after calling `mink_str_free()`
2. C code must not call `mink_str_free()` twice
3. C code must not pass a null `MinkStr.ptr` to MINK functions
4. C code must respect the lifetime documentation for each function

---

## 21. Panic/Error Behavior

### MINK panic across ABI
- **V1:** Panic across ABI boundary is **undefined behavior** (fatal error, process abort)
- **V2+:** Panic handler that translates to C error code + error message

### MINK error across ABI
- Functions returning `Result<T, E>` use the out-parameter pattern:
  ```c
  int64_t mink_parse(const char *input, MinkStr *result_out);
  // Returns 0 on success, error code on failure
  ```

### C error into MINK
- C functions that can fail return `int64_t` (0 = success, non-zero = error code)
- MINK checks the return code and propagates as `Result<T, Int>`

---

## 22. Thread Assumptions

### V1
- MINK is single-threaded. C code can call MINK functions from a single thread only.
- C code must not call MINK functions from multiple threads simultaneously.

### V2+
- MINK will add thread support
- Thread safety guarantees for C ABI functions will be documented per-function
- `#[thread_safe]` attribute for functions that are safe to call from multiple threads

---

## 23. C Header Generation

### Tool: `mink bind`

```bash
mink bind --target c src/lib.mink > lib.h
mink bind --target c++ src/lib.mink > lib.hpp
mink bind --target rust src/lib.mink > mink_bindings.rs
mink bind --target python src/lib.mink > mink_module.py
mink bind --target csharp src/lib.mink > MinkBindings.cs
```

### Header generation rules
1. Only `#[export]` functions are included
2. Types are mapped to C equivalents
3. Structs generate `typedef struct { ... } MinkTypeName;`
4. Enums generate `typedef int64_t MinkEnumName;` with `#define` constants
5. Doc comments become C comments
6. `#include` guards are generated

### Generated header example

```c
/* Generated by mink bind --target c */
/* Do not edit manually. */
#ifndef MINK_LIB_H
#define MINK_LIB_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct {
    int64_t x;
    int64_t y;
} MinkPoint;

/// Adds two integers.
int64_t mink_math_add(int64_t a, int64_t b);

/// Computes distance between two points.
double mink_math_distance(MinkPoint a, MinkPoint b);

/// Parses a string into an integer.
/// Returns 0 on success, 1 on parse error.
int64_t mink_math_parse_int(MinkStr input, int64_t *result_out);

/// Frees a MINK-allocated string.
void mink_str_free(MinkStr s);

#ifdef __cplusplus
}
#endif

#endif /* MINK_LIB_H */
```

---

## 24. MINK-to-C (Exporting)

### Flow
1. MINK developer marks functions with `#[export]`
2. `mink bind --target c` generates a C header
3. MINK compiler emits C-convention code for exported functions
4. C developer includes the header and links against the MINK library
5. C developer calls MINK functions through the generated interface

### Build artifacts
- Static library: `libmink_package.a` (Linux), `mink_package.lib` (Windows)
- Shared library: `libmink_package.so` (Linux), `mink_package.dll` (Windows), `libmink_package.dylib` (macOS)

---

## 25. C-to-MINK (Importing)

### Flow
1. MINK developer declares external functions with `extern "C"`
2. MINK developer provides the C library (`.a` or `.so`/`.dll`)
3. MINK compiler links against the C library
4. MINK code calls C functions through the declared interface

### Linking
- Static linking: `-L path -l library_name`
- Dynamic linking: `-L path -l library_name` (runtime dependency)

---

## 26. Future C++ Compatibility

### V1
- C ABI works with C++ through `extern "C"` linkage
- No C++-specific features

### V2+ (not yet designed)
- C++ wrapper header generation (`mink bind --target c++`)
- RAII wrappers for MINK resources
- Exception boundary (C++ exceptions → MINK errors)
- Namespace support in generated headers
- `std::string_view` ↔ `MinkStr` conversion helpers
- `std::span` ↔ `MinkSlice` conversion helpers

---

## 27. V1.x vs V2+ Explicit Boundary

### V1.x guarantees (shipped)
- Primitive type mapping (`Int`, `Bool`, `Float`, `Char`, `()`)
- Pointer types (`Ptr<T>`)
- `MinkStr` fat pointer (16 bytes)
- Blittable struct layout
- `#[export]` and `extern "C"` syntax
- C header generation
- Exported deallocation functions (`mink_str_free`, `mink_vec_free`)
- Platform-specific calling conventions

### V2+ (not yet designed)
- Sized integer types (`u8`, `u16`, `u32`, `u64`, `i8`, `i16`, `i32`)
- `f32` type
- Unicode `char` type
- `Option<T>` and `Result<T,E>` ABI representation
- Payloaded enum ABI representation
- C++ wrapper generation
- Python binding generation
- C# P/Invoke generation
- Rust binding generation
- Dynamic library export/import
- Thread-safe ABI functions
- Panic-to-error translation
- Shared allocator support

---

## 28. Two-Approach Analysis

### Approach A: C ABI as primary FFI boundary (CHOSEN)

| Criterion | Assessment |
|-----------|------------|
| **Pros** | Universal support, simple implementation, proven at scale, every language has C FFI |
| **Cons** | Some overhead for complex types, no type-safe ownership crossing |
| **Complexity** | Low-Medium |
| **Performance** | Good for most use cases, slight overhead for complex marshaling |
| **Security** | Clear boundary, explicit ownership rules |
| **Compatibility** | Universal — works with all target languages |
| **Maintainability** | One ABI to maintain, not N |
| **Ecosystem impact** | Maximum reach — every language can interoperate |

### Approach B: Native ABI with language-specific bridges

| Criterion | Assessment |
|-----------|------------|
| **Pros** | Zero-overhead for specific languages, richer type information |
| **Cons** | N implementations for N languages, massive maintenance burden, versioning complexity |
| **Complexity** | High |
| **Performance** | Optimal for specific pairs, worse overall |
| **Security** | More attack surface per bridge |
| **Compatibility** | Good for target languages, poor for others |
| **Maintainability** | High burden — each bridge needs its own testing and versioning |
| **Ecosystem impact** | Fragmented — only well-supported languages benefit |

### Decision: **Approach A**

**Reasoning:** MINK's strategic goal is interoperability with all major languages. C ABI covers 95% of use cases. The 5% overhead for complex types is acceptable. The maintenance burden of N bridges would be catastrophic for a small team. C ABI is the universal adapter.

---

*This specification is part of the MINK Ecosystem Architecture Design Pack (Session 50).*
*Do NOT implement until the design is frozen and reviewed.*
