# MINK: package, build and dependency system

**Status:** Planning / Specification
**Version:** 0.1.0

## 1. Objective

MINK must provide a unified workflow for creating, building, testing, running, packaging, and distributing software.

It must support small applications, libraries, CLI tools, desktop applications, web backends, services, large production systems, and monorepos.

## 2. Core principles

The system must prioritize reproducibility, security, fast builds, deterministic dependency resolution, simple project setup, strong diagnostics, offline capability, cross-platform support, IDE integration, and AI tooling integration.

## 3. Unified tooling

The official toolchain should provide unified commands for common workflows.

Conceptually:

    mink new
    mink build
    mink run
    mink test
    mink check
    mink fmt
    mink lint
    mink add
    mink remove
    mink update
    mink publish
    mink package

Exact command names may evolve before CLI stabilization.

## 4. Project structure

A standard project should have predictable structure:

    project/
    ├── mink.toml
    ├── src/
    ├── tests/
    ├── examples/
    ├── assets/
    ├── docs/
    └── build/

Exact conventions will be finalized by the toolchain specification.

## 5. Project manifest

Every package or application should have a machine-readable manifest describing project identity, version, language version, dependencies, features, build configuration, targets, capabilities, license, and relevant metadata.

## 6. Dependency declaration

Dependencies must be declared explicitly.

Declarations should support package identity, version requirements, registry/source, optional features, platform conditions, development dependencies, and build-time dependencies.

## 7. Dependency resolution

Dependency resolution must be deterministic.

Given the same manifest, lock state, registry state, toolchain, and target configuration, the resolver should select the same dependency graph.

## 8. Lockfile

MINK application projects should use a lockfile for reproducible builds.

The lockfile should record exact versions, sources, content hashes, dependency relationships, and relevant build metadata.

## 9. Dependency integrity

Dependencies should be integrity-verified using cryptographic hashes and, where appropriate, signatures.

Unexpected content changes must be detected.

## 10. Dependency security

The package ecosystem should eventually provide vulnerability checks, advisory information, malicious-package warnings, license information, provenance, and transitive dependency analysis.

## 11. Registry

MINK should support an official public registry while allowing private registries, local sources, Git-based dependencies where appropriate, and offline caches.

## 12. Offline development

Projects with cached dependencies should remain buildable, testable, runnable, and checkable without network access.

## 13. Dependency cache

The package manager should maintain a secure local cache supporting integrity verification, versioned artifacts, multiple targets, offline builds, cleanup, and safe concurrent access.

## 14. Build profiles

Standard profiles should include Development, Debug, Release, Size Optimized, and Performance Optimized.

## 15. Build targets

Targets must identify operating system, CPU architecture, ABI, runtime configuration, and linker/toolchain requirements.

Cross-compilation should be supported without unnecessary source changes.

## 16. Reproducible builds

Builds should be reproducible wherever technically practical.

Build metadata should identify compiler version, language version, dependencies, target, profile, and relevant configuration.

## 17. Incremental builds

The build system must integrate with compiler incremental compilation.

Unchanged source, dependencies, generated artifacts, and build inputs should not be rebuilt unnecessarily.

## 18. Parallel builds

Independent build tasks should execute concurrently while avoiding unsafe resource contention.

## 19. Build scripts

Custom build logic may be supported through controlled build scripts.

Build scripts must have explicit security boundaries for filesystem, environment, native toolchain, and network access.

## 20. Native dependencies

The system must support native libraries, system libraries, C/C++ interoperability, platform-specific libraries, and native build tools.

## 21. Features

Packages should support optional features controlling optional dependencies, platform support, experimental capabilities, performance alternatives, and integration layers.

Feature resolution must remain deterministic.

## 22. Workspaces

MINK should support multi-package workspaces and monorepos with shared dependencies, build configuration, toolchain settings, testing, formatting, and linting.

Individual packages must remain independently understandable.

## 23. Libraries and applications

The package system must support reusable libraries and executable applications as distinct package roles.

Applications may depend on libraries without forcing application-specific entry points into libraries.

## 24. Build artifacts

Build outputs must remain separate from source.

Artifacts may include executables, libraries, debug symbols, generated documentation, generated bindings, package archives, and metadata.

## 25. Testing integration

The build system must integrate directly with unit tests, integration tests, documentation tests, benchmarks, and platform-specific tests.

## 26. Formatting and linting

Formatting and linting should be first-class toolchain capabilities.

The formatter must be deterministic and the linter should use structured compiler analysis where possible.

## 27. Publishing

Publishing tooling must validate manifests, package structure, versioning, licensing metadata, documentation, dependencies, buildability, and integrity metadata.

## 28. Package signing

The ecosystem should support package signing so consumers can verify authenticity and integrity where appropriate.

## 29. Dependency updates

The toolchain should safely check outdated dependencies, explain updates, detect compatibility changes, update lockfiles, run tests, and report affected packages.

Automated updates must not silently alter source behavior without validation.

## 30. AI development integration

The package and build system should expose structured project information to AI coding agents.

AI tooling should be able to discover project structure, dependencies, build commands, test commands, targets, compiler version, configuration, and diagnostics.

## 31. Dependency graph

The package manager should provide dependency graph inspection and explain why a dependency exists.

Conceptually:

    mink deps
    mink deps tree
    mink deps why <package>

## 32. Dependency policies

Projects should be able to define policies covering allowed registries, licenses, security requirements, source types, dependency pinning, offline-only mode, and native dependency restrictions.

## 33. Long-Term compatibility

The package ecosystem must minimize dependency fragmentation and make project upgrades straightforward.

## 34. Open architecture decisions

The following must be finalized before architecture freeze:

- Manifest format
- Lockfile format
- Registry protocol
- Package naming rules
- Versioning model
- Dependency resolution algorithm
- Cache format
- Package signing and trust model
- Build-script sandbox
- Workspace semantics
- Native dependency model
- Feature-resolution model
- Publishing protocol
- Dependency advisory integration
