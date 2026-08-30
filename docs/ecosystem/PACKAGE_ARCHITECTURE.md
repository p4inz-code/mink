# MINK Package Architecture

**Version:** 0.1.0 (Design Draft)
**Date:** August 25, 2026
**Status:** DESIGN ONLY — not implemented
**Scope:** Complete package system design for the MINK ecosystem

---

## 1. Goals

1. **Dependency management.** Developers declare dependencies in a manifest file. The package manager resolves versions, downloads packages, and manages the dependency graph.

2. **Reproducible builds.** The lockfile ensures identical builds across machines and CI environments.

3. **Security.** Package integrity is verified by content hash. Build scripts are sandboxed.

4. **Scalability.** The system handles small projects (single file) and large workspaces (hundreds of packages).

5. **AI-friendliness.** Machine-readable manifest, structured dependency graph, deterministic resolution.

---

## 2. Package Identity

### Package name
- Format: `lowercase-with-hyphens` (e.g., `mink-json`, `mink-net`)
- Optional namespace: `@org/package-name` (e.g., `@acme/mink-web`)
- No uppercase, no underscores (hyphens only for readability)
- Maximum 64 characters
- Must be unique in the registry

### Package version
- Semantic versioning: `MAJOR.MINOR.PATCH`
- Pre-release: `1.0.0-alpha.1`, `1.0.0-beta.2`, `1.0.0-rc.1`
- Build metadata: `1.0.0+build.123` (ignored for version comparison)

### Version semantics
- `MAJOR` = breaking changes (incompatible API)
- `MINOR` = new features (backward compatible)
- `PATCH` = bug fixes (backward compatible)

---

## 3. mink.toml Manifest

```toml
[package]
name = "my-project"
version = "0.1.0"
edition = "2026"
license = "Apache-2.0"
description = "A MINK project"
authors = ["Author Name <email@example.com>"]
repository = "https://github.com/user/my-project"
keywords = ["example", "demo"]
categories = ["example"]

[dependencies]
mink-std = "1.0.0"
mink-json = { version = "0.3.0", features = ["serde"] }
mink-net = { version = ">=1.0, <2.0" }
my-utils = { path = "../my-utils" }

[dev-dependencies]
mink-test = "1.0.0"

[build-dependencies]
mink-codegen = "0.2.0"

[target.'cfg(target_os = "windows")'.dependencies]
mink-win32 = "1.0.0"

[target.'cfg(target_os = "linux")'.dependencies]
mink-linux = "1.0.0"

[features]
default = ["std"]
std = ["mink-std"]
async = ["mink-net/async"]
```

### Manifest fields

| Field | Required | Description |
|-------|----------|-------------|
| `name` | Yes | Package name |
| `version` | Yes | Package version (semver) |
| `edition` | Yes | Language edition (`2026`) |
| `license` | No | SPDX license identifier |
| `description` | No | Short description |
| `authors` | No | List of authors |
| `repository` | No | Source repository URL |
| `keywords` | No | Search keywords |
| `categories` | No | Registry categories |
| `dependencies` | No | Runtime dependencies |
| `dev-dependencies` | No | Test-only dependencies |
| `build-dependencies` | No | Build-script dependencies |
| `target.*.dependencies` | No | Platform-specific dependencies |
| `features` | No | Feature flags |

---

## 4. Dependency Declaration

### Version requirements

| Syntax | Meaning | Example |
|--------|---------|---------|
| `"1.0.0"` | Exact version | Only `1.0.0` |
| `"^1.0.0"` | Compatible (default) | `>=1.0.0, <2.0.0` |
| `"~1.2.3"` | Patch-level | `>=1.2.3, <1.3.0` |
| `">=1.0, <2.0"` | Range | Any version in range |
| `"*"` | Any version | **Not recommended** |

### Default behavior
- `version = "1.0.0"` is equivalent to `version = "^1.0.0"`
- The default is compatible (semver-compatible) versioning

### Dependencies with features

```toml
[dependencies]
mink-json = { version = "0.3.0", features = ["serde", "pretty"] }
```

### Path dependencies (local)

```toml
[dependencies]
my-utils = { path = "../my-utils" }
```

### Git dependencies (V2+)

```toml
[dependencies]
my-lib = { git = "https://github.com/user/my-lib", branch = "main" }
```

---

## 5. Lockfile (mink.lock)

### Purpose
- Records exact versions of all resolved dependencies
- Ensures reproducible builds
- Records content hashes for integrity verification

### Format

```toml
# This file is auto-generated. Do not edit manually.
# Run `mink update` to regenerate.

[[package]]
name = "mink-std"
version = "1.0.0"
source = "registry+https://mink.pkg.dev"
checksum = "sha256:abc123..."
dependencies = []

[[package]]
name = "mink-json"
version = "0.3.0"
source = "registry+https://mink.pkg.dev"
checksum = "sha256:def456..."
dependencies = ["mink-std"]

[[package]]
name = "my-project"
version = "0.1.0"
source = "local"
dependencies = ["mink-std", "mink-json"]
```

### Lockfile rules
1. The lockfile is the authoritative source for dependency versions
2. `mink build` uses the lockfile if it exists
3. `mink update` regenerates the lockfile
4. The lockfile is committed to version control
5. Content hashes verify package integrity

---

## 6. Source vs Binary Packages

### Source packages
- Contain `.mink` source files
- Compiled by the consumer's compiler
- Default package type
- Require compatible compiler version

### Binary packages (V2+)
- Pre-compiled native libraries
- Platform-specific
- Used for performance-critical or proprietary packages
- Require matching target triple

### Mixed packages
- Source package with optional pre-compiled binaries
- Falls back to source compilation if binary is unavailable

---

## 7. Build Dependencies

### Purpose
- Code generation tools
- Build scripts that run during compilation
- Platform detection scripts

### Sandboxing
- Build scripts run with restricted permissions
- No network access by default (opt-in)
- Filesystem access limited to the package directory
- Environment variables are sanitized

### Build script contract

```mink
// build.mink (V2+, not V1)
fn build() {
    // Code generation, platform detection, etc.
}
```

---

## 8. Target-Specific Dependencies

```toml
[target.'cfg(target_os = "windows")'.dependencies]
mink-win32 = "1.0.0"

[target.'cfg(target_os = "linux")'.dependencies]
mink-linux = "1.0.0"

[target.'cfg(target_arch = "aarch64")'.dependencies]
mink-arm = "1.0.0"
```

### Platform detection
- `target_os`: `windows`, `linux`, `macos`
- `target_arch`: `x86_64`, `aarch64`
- `target_env`: `msvc`, `gnu`

---

## 9. Optional Dependencies and Features

### Optional dependencies

```toml
[dependencies]
mink-json = { version = "0.3.0", optional = true }

[features]
json = ["mink-json"]
full = ["json", "async"]
```

### Feature rules
1. Features are additive (never subtractive)
2. Features can activate optional dependencies
3. Features can enable conditional compilation (`#[cfg(feature = "json")]`)
4. The `default` feature is activated unless explicitly disabled

```toml
[dependencies]
my-lib = { version = "1.0.0", default-features = false, features = ["basic"] }
```

---

## 10. Workspace Support

### Workspace manifest

```toml
# mink.toml (workspace root)
[workspace]
members = ["core", "utils", "web", "cli"]

[workspace.dependencies]
mink-std = "1.0.0"
mink-test = "1.0.0"
```

### Member packages

```toml
# core/mink.toml
[package]
name = "my-core"
version = "0.1.0"

[dependencies]
mink-std.workspace = true
```

### Workspace rules
1. All members share a single `mink.lock`
2. Dependencies can be inherited from the workspace
3. Features are unified across the workspace
4. Build commands operate on the workspace by default

---

## 11. Module Layout

### Standard layout

```
my-package/
├── mink.toml
├── mink.lock
├── src/
│   ├── main.mink      # Binary entry point (optional)
│   ├── lib.mink        # Library root (optional)
│   ├── module1.mink    # Module file
│   └── module2/
│       ├── mod.mink    # Module root
│       ├── sub1.mink   # Submodule
│       └── sub2.mink   # Submodule
├── tests/
│   ├── test_module1.mink
│   └── integration/
│       └── test_api.mink
├── examples/
│   └── demo.mink
├── benches/
│   └── perf.mink
└── docs/
```

### Module declarations

```mink
// src/lib.mink
mod module1;           // loads src/module1.mink
mod module2;           // loads src/module2/mod.mink

pub use module1::Item; // re-export
```

### Module visibility
- `pub` — visible everywhere
- `pub(crate)` — visible within the package
- `pub(super)` — visible in the parent module
- Private by default

---

## 12. Imports and Exports

### Import syntax

```mink
use mink_std::collections::Vec;
use mink_json::{parse, stringify};
use my_utils::{helper1, helper2};
```

### Import rules
1. `use` imports items by path
2. `use mod_name;` imports a module (V1 syntax, retained)
3. `use package::module::Item;` imports a specific item
4. `use package::module::{Item1, Item2};` imports multiple items
5. `use package::module::*;` imports all public items (wildcard)

### Export rules
- `pub` makes an item visible to importers
- `pub(crate)` restricts visibility to the current package
- The package root (`lib.mink` or `main.mink`) determines the public API

---

## 13. Package Boundaries

### Each package is a compilation unit
- Packages are compiled independently
- Only public items are visible across package boundaries
- Private items are not accessible from other packages

### Cross-package type compatibility
- Types are identified by their fully-qualified path (`package::module::Type`)
- Two types with the same name from different packages are distinct
- Type compatibility requires the same origin package

---

## 14. Reproducible Builds

### Guarantees
1. Same `mink.lock` → same dependency versions
2. Same compiler version → same compilation behavior
3. Deterministic compilation (no HashMap ordering leakage)

### Build verification
```bash
mink verify  # Check that the lockfile matches the manifest
mink audit   # Check for known vulnerabilities
```

### Content hashing
- Every package version has a SHA-256 content hash
- The lockfile records hashes for all dependencies
- Downloads are verified against the hash before extraction

---

## 15. Package Caching

### Cache location
- `~/.mink/cache/` (user cache)
- `~/.mink/registry/` (registry index)

### Cache structure

```
~/.mink/cache/
├── registry/
│   └── mink.dev/
│       ├── mink-std/
│       │   └── 1.0.0/
│       │       ├── mink-std-1.0.0.tar.gz
│       │       └── mink-std-1.0.0.tar.gz.sha256
│       └── mink-json/
│           └── 0.3.0/
│               ├── mink-json-0.3.0.tar.gz
│               └── mink-json-0.3.0.tar.gz.sha256
└── builds/
    └── <hash>/
        └── ...  # cached build artifacts
```

### Cache rules
1. Packages are cached after first download
2. Cache is verified against content hash on use
3. `mink clean` clears the build cache
4. `mink clean --all` clears the entire cache
5. Offline mode uses only cached packages

---

## 16. Package Verification

### Verification steps
1. Download package archive
2. Verify SHA-256 content hash against lockfile
3. Extract to cache directory
4. Verify extracted files match expected structure
5. Compile and link

### Integrity failure
- Hash mismatch → error, package not used
- Corrupted archive → error, re-download attempted
- Missing expected files → error, package rejected

---

## 17. Package Signing (V2+)

### Signing model
- Packages can be signed by the publisher
- Signature is included in the package archive
- Registry verifies signatures for trusted publishers
- Consumers can require signatures for all dependencies

### Signature format
- Ed25519 signatures (fast, small)
- Public key published with the publisher's account
- Signature covers the package archive hash

---

## 18. Registry Interaction

### Registry URL
- Default: `https://mink.pkg.dev`
- Configurable per-package in `mink.toml`

### Registry API

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/v1/packages/{name}` | GET | Package metadata |
| `/api/v1/packages/{name}/{version}` | GET | Version metadata |
| `/api/v1/packages/{name}/{version}/download` | GET | Download package |
| `/api/v1/packages/{name}/{version}/verify` | GET | Verify integrity |
| `/api/v1/search?q={query}` | GET | Search packages |
| `/api/v1/packages` | POST | Publish package |

### Registry interaction rules
1. All requests use HTTPS
2. Retries with exponential backoff
3. Cached responses respect `Cache-Control` headers
4. Rate limiting: 100 requests/minute per IP
5. Authentication: API tokens (V2+)

---

## 19. Publishing

### Publish command

```bash
mink publish --token <api-token>
```

### Publish process
1. Verify package name is available (or user owns it)
2. Build the package
3. Run tests
4. Create package archive
5. Sign the archive (if signing is enabled)
6. Upload to registry
7. Registry verifies integrity and signature
8. Package becomes available

### Publish restrictions
- Cannot publish a version that already exists
- Cannot publish without a valid `mink.toml`
- Cannot publish with failing tests
- Cannot publish with uncommitted changes (V2+)

---

## 20. Downloading

### Download process
1. Check cache for exact version
2. If not cached, download from registry
3. Verify content hash against lockfile
4. Extract to cache directory
5. Make available for compilation

### Offline mode
- `mink build --offline` uses only cached packages
- Fails if any dependency is not cached
- Useful for CI/CD and air-gapped environments

---

## 21. Dependency Resolution

### Resolution algorithm
1. Read `mink.toml` for direct dependencies
2. For each dependency, read its `mink.toml` for transitive dependencies
3. Resolve versions using semver compatibility
4. Detect conflicts (diamond problem)
5. Detect cycles
6. Generate lockfile

### Conflict resolution
- If two packages require incompatible versions → error
- If two packages require compatible versions → use the highest compatible version
- The resolver is deterministic given the same inputs

### Cycle detection
- If A depends on B and B depends on A → error
- Cycles are always errors (no circular dependencies)

---

## 22. Dependency Conflicts

### Diamond problem
```
A → B 1.0.0
A → C 1.0.0
B → D ^1.0.0
C → D ^1.1.0
```

**Resolution:** Use D 1.1.0 (highest compatible version).

### Incompatible versions
```
A → B ^1.0.0
C → B ^2.0.0
```

**Resolution:** Error — cannot satisfy both constraints.

### Conflict reporting
```
error: version conflict for package `b`
  required by: a 1.0.0 (wants ^1.0.0)
  required by: c 1.0.0 (wants ^2.0.0)
  no version satisfies both constraints
```

---

## 23. Transitive Dependencies

### Rules
1. All transitive dependencies are resolved and locked
2. Transitive dependencies are not re-exported (no implicit re-exports)
3. Version conflicts in transitive dependencies are errors
4. The lockfile records the complete dependency graph

### Dependency tree display

```bash
mink deps
```

```
my-project 0.1.0
├── mink-std 1.0.0
├── mink-json 0.3.0
│   └── mink-std 1.0.0 (compatible, unified)
└── my-utils 0.2.0
    └── mink-std 1.0.0 (compatible, unified)
```

---

## 24. Malicious Package Protection

### Measures
1. **Content hashing.** Every package is verified by SHA-256 hash.
2. **Lockfile integrity.** The lockfile records hashes; tampering is detected.
3. **Build script sandboxing.** Build scripts run with restricted permissions.
4. **No automatic network access.** Build scripts must opt-in to network.
5. **Dependency auditing.** `mink audit` checks for known vulnerabilities.
6. **Trusted publishers.** Package signing identifies the publisher.
7. **Typosquatting detection.** Registry warns on suspiciously similar package names.
8. **Dependency confusion protection.** Namespace isolation prevents shadowing.

### Audit command

```bash
mink audit
```

```
warning: advisory for mink-crypto 0.1.0
  advisory: RUSTSEC-2026-0001
  title: Timing attack in HMAC verification
  severity: medium
  recommendation: upgrade to mink-crypto 0.2.0
```

---

## 25. Two-Approach Analysis

### Approach A: Cargo-inspired package model (CHOSEN)

| Criterion | Assessment |
|-----------|------------|
| **Pros** | Mature, handles features/platforms/workspaces, proven at scale, familiar to Rust developers |
| **Cons** | Complex feature resolution, more configuration |
| **Complexity** | High (but well-understood) |
| **Performance** | Good with caching |
| **Security** | Strong (content hashing, lockfile integrity, build sandboxing) |
| **Compatibility** | Excellent for a systems language |
| **Maintainability** | Well-documented model, large reference implementation |
| **Ecosystem impact** | Maximum — Cargo is the gold standard for systems-language package managers |

### Approach B: Go-module-inspired package model

| Criterion | Assessment |
|-----------|------------|
| **Pros** | Simple, fast builds, minimal configuration |
| **Cons** | Less flexible, no features, no workspaces, limited ecosystem tooling |
| **Complexity** | Low-Medium |
| **Performance** | Excellent (simpler resolution) |
| **Security** | Good (module proxy, checksums) |
| **Compatibility** | Limited for a systems language |
| **Maintainability** | Simple to maintain |
| **Ecosystem impact** | Moderate — good for simple projects, insufficient for complex ecosystems |

### Decision: **Approach A**

**Reasoning:** MINK is a systems language that needs features, platform-specific dependencies, workspaces, and a rich ecosystem. Cargo's model handles all of this. The complexity is justified by the ecosystem's needs. Go's module model is excellent for Go but insufficient for MINK's requirements.

---

*This specification is part of the MINK Ecosystem Architecture Design Pack (Session 50).*
*Do NOT implement until the design is frozen and reviewed.*
