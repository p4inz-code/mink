# MINK Cross-Platform Architecture

**Version:** 0.1.0 (Design Draft)
**Date:** August 25, 2026
**Status:** DESIGN ONLY — not implemented
**Scope:** Cross-platform target model for the MINK ecosystem

---

## 1. Goals

1. **Universal deployment.** MINK must compile and run on Windows, Linux, and macOS (x86_64 and ARM64).

2. **Zero runtime overhead.** Platform selection happens at compile time, not runtime. No vtable dispatch, no runtime detection.

3. **Consistent behavior.** The language semantics are identical across platforms. Platform differences are isolated to the runtime and backend.

4. **Cross-compilation.** The compiler can target any supported platform from any host platform.

5. **Future extensibility.** New platforms (WASM, embedded, RISC-V) can be added without changing the core architecture.

---

## 2. Target Triple Format

### Standard format
```
{arch}-{vendor}-{os}-{abi}
```

### Supported targets

| Target Triple | Architecture | OS | ABI | Status |
|---------------|-------------|-----|-----|--------|
| `x86_64-pc-windows-msvc` | x86_64 | Windows | MSVC | **Implemented** (as `x86_64-windows-pe`) |
| `x86_64-unknown-linux-gnu` | x86_64 | Linux | GNU | Recognized, not implemented |
| `aarch64-unknown-linux-gnu` | ARM64 | Linux | GNU | Recognized, not implemented |
| `aarch64-apple-darwin` | ARM64 | macOS | Darwin | Recognized, not implemented |
| `x86_64-apple-darwin` | x86_64 | macOS | Darwin | Future |
| `wasm32-unknown-wasi` | WASM32 | WASI | WASI | Future |

### Target naming convention
- V1 uses simplified names: `x86_64-windows-pe`
- V2 uses standard GNU triple format: `x86_64-pc-windows-msvc`
- Both formats are accepted; the simplified form maps to the standard form

### Target parsing

```rust
// Current V1 (simplified)
Target::parse("x86_64-windows-pe")

// V2 (standard triples)
Target::parse("x86_64-pc-windows-msvc")
Target::parse("x86_64-unknown-linux-gnu")
Target::parse("aarch64-unknown-linux-gnu")
Target::parse("aarch64-apple-darwin")
```

---

## 3. Target Selection

### Default target
- The host's native target (detected at compiler startup)
- On Windows x86_64: `x86_64-pc-windows-msvc`
- On Linux x86_64: `x86_64-unknown-linux-gnu`
- On macOS ARM64: `aarch64-apple-darwin`

### Explicit target selection

```bash
mink build main.mink --target x86_64-unknown-linux-gnu
```

### Target detection

```mink
// Conditional compilation (V2+)
#[cfg(target_os = "windows")]
fn platform_impl() { ... }

#[cfg(target_os = "linux")]
fn platform_impl() { ... }

#[cfg(target_arch = "aarch64")]
fn arm64_specific() { ... }
```

---

## 4. Target Abstraction

### Current architecture (V1)

The backend is target-independent up to emission:

```
MIR → Backend IR (BProgram) → Target-specific emitter → Machine code
```

The `Target` enum selects the emitter:

```rust
pub enum Target {
    X86_64WindowsPe,    // Implemented
    X86_64LinuxElf,     // Recognized, not implemented
    AArch64LinuxElf,    // Recognized, not implemented
}
```

### V2 architecture

The target abstraction expands to include platform-specific runtime services:

```
Compiler (target-independent)
    ↓
Backend IR (BProgram) → Target emitter → Machine code + platform runtime
    ↓
Platform runtime (target-specific)
    - Syscall wrappers
    - Memory allocation
    - Thread management
    - I/O operations
```

### Target trait (V2)

```rust
trait TargetBackend {
    fn emit(&self, program: &BProgram) -> Result<Image, BackendError>;
    fn calling_convention(&self) -> CallingConvention;
    fn pointer_size(&self) -> u32;
    fn alignment(&self, ty: &Type) -> u32;
    fn executable_format(&self) -> ExecutableFormat;
}
```

---

## 5. Platform APIs

### Platform abstraction layers

| Layer | Windows | Linux | macOS |
|-------|---------|-------|-------|
| **Syscalls** | Win32 API | POSIX syscalls | POSIX syscalls |
| **File I/O** | CreateFile/ReadFile/WriteFile | open/read/write | open/read/write |
| **Memory** | VirtualAlloc/VirtualFree | mmap/munmap | mmap/munmap |
| **Threads** | CreateThread | pthread_create | pthread_create |
| **Networking** | Winsock2 | BSD sockets | BSD sockets |
| **Dynamic loading** | LoadLibrary/GetProcAddress | dlopen/dlsym | dlopen/dlsym |
| **Environment** | GetEnvironmentVariable | getenv | getenv |
| **Processes** | CreateProcess | fork/exec | fork/exec |
| **Signals** | SEH/structured exceptions | signals | signals |
| **Time** | QueryPerformanceCounter | clock_gettime | clock_gettime |

### Platform module structure (V2)

```
src/platform/
├── mod.rs              # Platform trait
├── windows/
│   ├── mod.rs          # Windows platform implementation
│   ├── syscalls.rs     # Win32 API wrappers
│   ├── file.rs         # File I/O
│   ├── memory.rs       # Memory allocation
│   └── threads.rs      # Thread management
├── linux/
│   ├── mod.rs          # Linux platform implementation
│   ├── syscalls.rs     # POSIX syscall wrappers
│   ├── file.rs         # File I/O
│   ├── memory.rs       # Memory allocation
│   └── threads.rs      # Thread management
└── macos/
    ├── mod.rs          # macOS platform implementation
    ├── syscalls.rs     # POSIX syscall wrappers
    ├── file.rs         # File I/O
    ├── memory.rs       # Memory allocation
    └── threads.rs      # Thread management
```

---

## 6. Conditional Compilation

### Syntax (V2+)

```mink
#[cfg(target_os = "windows")]
fn read_input() -> Str {
    // Windows-specific implementation
}

#[cfg(target_os = "linux")]
fn read_input() -> Str {
    // Linux-specific implementation
}

#[cfg(target_os = "macos")]
fn read_input() -> Str {
    // macOS-specific implementation
}
```

### cfg predicates

| Predicate | Values | Example |
|-----------|--------|---------|
| `target_os` | `windows`, `linux`, `macos` | `#[cfg(target_os = "windows")]` |
| `target_arch` | `x86_64`, `aarch64` | `#[cfg(target_arch = "aarch64")]` |
| `target_env` | `msvc`, `gnu` | `#[cfg(target_env = "gnu")]` |
| `target_family` | `windows`, `unix` | `#[cfg(target_family = "unix")]` |
| `feature` | any feature name | `#[cfg(feature = "async")]` |
| `test` | `true` in test mode | `#[cfg(test)]` |

### Conditional compilation rules
1. `#[cfg(...)]` on functions, modules, structs, enums, and constants
2. `cfg!()` macro for inline conditionals (V2+)
3. `cfg_attr` for conditional attributes (V2+)

---

## 7. OS Abstraction

### File abstraction

```mink
// V2+: platform-agnostic file operations
struct File {
    handle: Ptr<Void>,
}

impl File {
    fn open(path: Str, mode: FileMode) -> Result<File, IoError>;
    fn read(self: &mut File, buf: Ptr<UInt8>, len: Int) -> Result<Int, IoError>;
    fn write(self: &mut File, buf: Ptr<UInt8>, len: Int) -> Result<Int, IoError>;
    fn close(self: File);
}
```

### Process abstraction

```mink
// V2+: platform-agnostic process operations
struct Process {
    pid: Int,
}

impl Process {
    fn spawn(command: Str, args: Vec<Str>) -> Result<Process, ProcessError>;
    fn wait(self: &Process) -> Result<Int, ProcessError>;
    fn kill(self: &Process) -> Result<(), ProcessError>;
}
```

### Environment abstraction

```mink
// V2+: platform-agnostic environment access
fn env_get(key: Str) -> Option<Str>;
fn env_set(key: Str, value: Str) -> Result<(), EnvError>;
fn env_remove(key: Str) -> Result<(), EnvError>;
fn env_vars() -> Vec<(Str, Str)>;
```

---

## 8. Path Handling

### Path representation
- Paths are strings (UTF-8 on all platforms)
- Separator: `\` on Windows, `/` on Linux/macOS
- The `Path` type normalizes separators internally

### Path operations

```mink
struct Path {
    raw: Str,
}

impl Path {
    fn new(raw: Str) -> Path;
    fn join(self: &Path, other: &Path) -> Path;
    fn parent(self: &Path) -> Option<Path>;
    fn filename(self: &Path) -> Option<Str>;
    fn extension(self: &Path) -> Option<Str>;
    fn is_absolute(self: &Path) -> Bool;
    fn normalize(self: &Path) -> Path;
}
```

### Platform-specific path rules
- Windows: drive letters (`C:\`), UNC paths (`\\server\share`)
- Linux/macOS: root (`/`), home (`~`)
- Maximum path length: 260 (Windows legacy), unlimited (Linux/macOS)

---

## 9. Endianness

### V1
- All targets are little-endian (x86_64, ARM64)
- No big-endian support

### V2+
- Endianness detection via `target_endian`
- Big-endian support for embedded targets
- Byte order conversions for network protocols

---

## 10. Pointer Width

### Platform pointer sizes

| Platform | Pointer Size | Maximum Addressable |
|----------|-------------|-------------------|
| x86_64 | 8 bytes | 2^64 bytes |
| ARM64 | 8 bytes | 2^64 bytes |
| x86 (future) | 4 bytes | 2^32 bytes |
| WASM32 | 4 bytes | 2^32 bytes |

### V1
- All targets use 64-bit pointers
- `Ptr<T>` is always 8 bytes

---

## 11. ABI Differences

### Calling conventions per platform

| Platform | Convention | First 4 args | Return | Callee-saved |
|----------|------------|-------------|--------|--------------|
| Windows x86_64 | Microsoft x64 | RCX, RDX, R8, R9 | RAX | RBX, RBP, RDI, RSI, R12-R15 |
| Linux x86_64 | System V AMD64 | RDI, RSI, RDX, RCX | RAX | RBX, RBP, R12-R15 |
| macOS ARM64 | AAPCS64 | X0-X3 | X0 | X19-X28 |
| Linux ARM64 | AAPCS64 | X0-X3 | X0 | X19-X28 |

### Stack alignment
- x86_64: 16-byte alignment before `call`
- ARM64: 16-byte alignment

### Register usage
- Windows: volatile (RAX, RCX, RDX, R8-R11) and non-volatile (RBX, RBP, RDI, RSI, R12-R15)
- Linux/macOS: caller-saved (RAX, RCX, RDX, RSI, RDI, R8-R11) and callee-saved (RBX, RBP, R12-R15)

---

## 12. Runtime Portability

### Runtime services per platform

| Service | Windows | Linux | macOS |
|---------|---------|-------|-------|
| **Heap allocation** | VirtualAlloc | mmap | mmap |
| **Stdout** | WriteFile(GetStdHandle) | write(1) | write(1) |
| **Stderr** | WriteFile(GetStdHandle) | write(2) | write(2) |
| **Exit** | ExitProcess | _exit | _exit |
| **Thread local** | TlsAlloc | __thread | _Thread_local |

### Runtime embedding
- The runtime is embedded in the executable (no shared library dependency)
- Platform-specific runtime code is selected at compile time
- The runtime binary size is small (~4KB of machine code)

---

## 13. Backend Portability

### Backend architecture

```
MIR → Backend IR (BProgram) → Target emitter → Image
```

The Backend IR is target-independent. Each target provides an emitter:

| Target | Emitter | Output |
|--------|---------|--------|
| x86_64-windows-pe | `x86_64.rs` + `pe.rs` | `.exe` |
| x86_64-linux-elf | `x86_64.rs` + `elf.rs` | ELF binary |
| aarch64-linux-elf | `aarch64.rs` + `elf.rs` | ELF binary |
| aarch64-apple-darwin | `aarch64.rs` + `macho.rs` | Mach-O binary |

### Emitter components
1. **Instruction selection.** Backend IR → platform instructions
2. **Register allocation.** Map virtual registers to physical registers
3. **Frame layout.** Stack frame sizing and alignment
4. **Relocation.** Fix up addresses for position-dependent code
5. **Container.** Assemble into executable format (PE, ELF, Mach-O)

---

## 14. Executable Formats

### PE (Windows)

```
DOS Header
PE Signature
COFF Header
Optional Header
Section Headers
.text (code)
.data (initialized data)
.bss (uninitialized data)
.rdata (read-only data)
```

### ELF (Linux)

```
ELF Header
Program Headers
.text (code)
.data (initialized data)
.bss (uninitialized data)
.rodata (read-only data)
Section Headers
```

### Mach-O (macOS)

```
Mach-O Header
Load Commands
__TEXT segment (code)
__DATA segment (data)
__LINKEDIT segment (symbols)
```

---

## 15. Linker Strategy

### V1: No external linker
- The backend emits complete executable images directly
- No dependency on `ld`, `link.exe`, or any external toolchain
- Self-contained code generation

### V2+: Optional external linker
- For shared libraries (`.so`, `.dll`, `.dylib`)
- For dynamic linking scenarios
- The compiler can emit object files (`.o`, `.obj`) for external linking

### Linking model

| Scenario | V1 | V2+ |
|----------|-----|-----|
| Static executable | Backend emits directly | Backend emits directly |
| Shared library | Not supported | External linker required |
| Dynamic linking | Not supported | External linker required |
| Cross-compilation | Backend emits directly | Backend emits directly |

---

## 16. Cross-Compilation Strategy

### V1
- Cross-compilation is NOT supported (only host target)
- The backend emits for the host platform only

### V2+
- Cross-compilation is supported via `--target`
- The compiler emits for the specified target, regardless of the host
- No cross-compilation toolchain is needed (the backend is self-contained)

### Cross-compilation flow

```
Host: Linux x86_64
Target: Windows x86_64

mink build main.mink --target x86_64-pc-windows-msvc

→ Compiler runs on Linux
→ Emits Windows PE executable
→ Output: main.exe (runs on Windows)
```

### Requirements for cross-compilation
1. The compiler must be able to emit the target's executable format
2. The runtime must be available for the target platform
3. No external toolchain is needed (self-contained emission)

---

## 17. CI Matrix

### Recommended CI matrix

| Host OS | Target | Status |
|---------|--------|--------|
| Windows x86_64 | x86_64-pc-windows-msvc | **Priority 1** (current) |
| Ubuntu x86_64 | x86_64-unknown-linux-gnu | **Priority 2** |
| macOS ARM64 | aarch64-apple-darwin | **Priority 3** |
| Ubuntu ARM64 | aarch64-unknown-linux-gnu | **Priority 4** |
| Ubuntu x86_64 | x86_64-pc-windows-msvc | Cross-compilation test |

### CI workflow

```yaml
# .github/workflows/ci.yml (V2+)
matrix:
  include:
    - host: windows-latest
      target: x86_64-pc-windows-msvc
    - host: ubuntu-latest
      target: x86_64-unknown-linux-gnu
    - host: macos-latest
      target: aarch64-apple-darwin

steps:
  - name: Build compiler
    run: cargo build --release
  - name: Run compiler tests
    run: cargo test
  - name: Build MINK tests
    run: ./target/release/mink build tests/main.mink --target ${{ matrix.target }}
  - name: Run MINK tests
    run: ./target/release/mink test
```

---

## 18. Two-Approach Analysis

### Approach A: Runtime-linked platform abstraction (CHOSEN)

| Criterion | Assessment |
|-----------|------------|
| **Pros** | Zero runtime overhead, matches current model, clean separation, compile-time selection |
| **Cons** | Different binaries per platform, no runtime detection |
| **Complexity** | Medium |
| **Performance** | Optimal (no indirection) |
| **Security** | Clear — no runtime dispatch to exploit |
| **Compatibility** | Excellent — matches how C/C++ work |
| **Maintainability** | Each platform is an isolated module |
| **Ecosystem impact** | Maximum — standard for systems languages |

### Approach B: Runtime-vtable platform abstraction

| Criterion | Assessment |
|-----------|------------|
| **Pros** | Single binary, runtime detection, dynamic dispatch |
| **Cons** | Indirection overhead, complex init, harder to optimize |
| **Complexity** | Medium-High |
| **Performance** | Good but not optimal (vtable indirection) |
| **Security** | More attack surface (runtime dispatch) |
| **Compatibility** | Good but unusual for systems languages |
| **Maintainability** | Single binary to maintain, but complex initialization |
| **Ecosystem impact** | Moderate — unusual for systems languages |

### Decision: **Approach A**

**Reasoning:** MINK compiles to native code with no external toolchain. Platform selection should happen at compile time, matching the current model. The runtime is embedded in the binary, so platform-specific runtime code is selected during compilation, not at runtime. This is the standard approach for systems languages (C, C++, Rust all use compile-time platform selection).

---

*This specification is part of the MINK Ecosystem Architecture Design Pack (Session 50).*
*Do NOT implement until the design is frozen and reviewed.*
