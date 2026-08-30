# MINK AI Developer Architecture

**Version:** 0.1.0 (Design Draft)
**Date:** August 25, 2026
**Status:** DESIGN ONLY — not implemented
**Scope:** AI-first tooling infrastructure for the MINK ecosystem

---

## 1. Goals

1. **Agent-friendly by design.** MINK's tooling is designed so that AI coding agents can use it effectively, not as an afterthought.

2. **Machine-readable everything.** Diagnostics, project metadata, dependency graphs, and API documentation are all machine-readable.

3. **Deterministic behavior.** Same input → same output. AI agents can verify their changes produce consistent results.

4. **Fast feedback loops.** Compilation, testing, and linting are fast enough for interactive agent use.

5. **Not a gimmick.** The AI tooling is real infrastructure that makes MINK objectively easier for agents to generate and maintain.

---

## 2. Machine-Readable Compiler Diagnostics

### Current state (V1)
- Every error has a stable code (`E-L01`, `E-T05`, `E-S10`, etc.)
- Every error has source spans and related locations
- Diagnostics are printed to stderr in human-readable format

### V1.x improvement: JSON diagnostic output

```bash
mink check main.mink --json
```

```json
{
  "success": false,
  "errors": [
    {
      "code": "E-T05",
      "severity": "error",
      "message": "type mismatch: expected Int, found Bool",
      "span": {
        "file": "main.mink",
        "start_line": 10,
        "start_column": 5,
        "end_line": 10,
        "end_column": 8
      },
      "related": [
        {
          "message": "expected here",
          "span": {
            "file": "main.mink",
            "start_line": 8,
            "start_column": 1,
            "end_line": 8,
            "end_column": 20
          }
        }
      ],
      "suggested_fix": "cast the value to Int",
      "documentation_url": "https://mink.dev/docs/errors/E-T05"
    }
  ],
  "warnings": [],
  "token_count": 42,
  "files_checked": 1
}
```

### Diagnostic JSON schema

```json
{
  "code": "string (stable error code)",
  "severity": "error | warning | note",
  "message": "string (human-readable message)",
  "span": {
    "file": "string (file path)",
    "start_line": "integer",
    "start_column": "integer",
    "end_line": "integer",
    "end_column": "integer"
  },
  "related": [
    {
      "message": "string",
      "span": { "... same as above ..." }
    }
  ],
  "suggested_fix": "string (optional fix suggestion)",
  "documentation_url": "string (link to error documentation)"
}
```

### Diagnostic codes

| Prefix | Domain | Examples |
|--------|--------|---------|
| `E-L` | Lexical | `E-L01` (unterminated string), `E-L02` (invalid character) |
| `E-P` | Parser | `E-P01` (expected token), `E-P02` (unexpected token) |
| `E-S` | Semantic | `E-S01` (undefined symbol), `E-S02` (duplicate definition) |
| `E-T` | Type | `E-T01` (type mismatch), `E-T02` (missing type) |
| `E-O` | Ownership | `E-S10` (use after move), `E-S12` (borrow conflict) |
| `E-R` | Runtime | `E-R02` (out of memory), `E-R06` (leak detected) |
| `E-B` | Backend | `E-B01` (unsupported target), `E-B07` (emission failed) |

---

## 3. Stable Error Codes

### Error code documentation

```bash
mink explain E-T05
```

```
Error E-T05: Type Mismatch

The compiler expected a value of one type but found a different type.

Example:
  fn add(a: Int, b: Int) -> Int {
      return a + b;
  }
  
  fn main() {
      let x: Int = add(1, true);  // E-T05: expected Int, found Bool
  }

Common causes:
  - Passing the wrong type to a function
  - Assigning a value to a variable of the wrong type
  - Returning the wrong type from a function

Suggested fix:
  Cast the value to the expected type, or change the function signature.

Documentation: https://mink.dev/docs/errors/E-T05
```

### Error code as machine-readable resource

```json
{
  "code": "E-T05",
  "title": "Type Mismatch",
  "description": "The compiler expected a value of one type but found a different type.",
  "severity": "error",
  "category": "type",
  "examples": [
    {
      "code": "fn add(a: Int, b: Int) -> Int { return a + b; }\nfn main() { let x: Int = add(1, true); }",
      "error_span": { "line": 3, "column": 25, "length": 4 },
      "message": "expected Int, found Bool"
    }
  ],
  "common_causes": [
    "Passing the wrong type to a function",
    "Assigning a value to a variable of the wrong type",
    "Returning the wrong type from a function"
  ],
  "suggested_fixes": [
    "Cast the value to the expected type",
    "Change the function signature to accept the actual type"
  ],
  "documentation_url": "https://mink.dev/docs/errors/E-T05"
}
```

---

## 4. Structured JSON Diagnostics

### All compiler commands support `--json`

```bash
mink check main.mink --json
mink build main.mink --json
mink test --json
mink deps --json
```

### JSON output format

```json
{
  "command": "check",
  "success": true,
  "duration_ms": 42,
  "files_checked": 3,
  "token_count": 156,
  "errors": [],
  "warnings": [],
  "notes": []
}
```

### JSON output for build

```json
{
  "command": "build",
  "success": true,
  "output": "main.exe",
  "target": "x86_64-pc-windows-msvc",
  "functions": 12,
  "bindings": 3,
  "duration_ms": 234,
  "errors": [],
  "warnings": []
}
```

### JSON output for test

```json
{
  "command": "test",
  "success": true,
  "total": 42,
  "passed": 40,
  "failed": 1,
  "ignored": 1,
  "duration_ms": 1234,
  "tests": [
    {
      "name": "test_addition",
      "status": "passed",
      "duration_ms": 12
    },
    {
      "name": "test_division_by_zero",
      "status": "failed",
      "duration_ms": 8,
      "error": {
        "message": "assertion failed: 1/0 == error",
        "span": { "file": "tests/test_math.mink", "line": 15, "column": 5 }
      }
    }
  ]
}
```

---

## 5. Compiler Introspection

### AST/HIR/MIR inspection interfaces

```bash
mink ast main.mink --json     # Dump AST as JSON
mink hir main.mink --json     # Dump HIR as JSON
mink mir main.mink --json     # Dump MIR as JSON
mink backend main.mink --json # Dump backend IR as JSON
```

### Use cases for AI agents
1. **Code understanding.** Parse the AST to understand program structure.
2. **Code generation.** Generate MINK code by constructing AST nodes.
3. **Refactoring.** Transform AST/HIR/MIR to refactor code.
4. **Debugging.** Inspect intermediate representations to understand compilation.
5. **Testing.** Verify that generated code compiles correctly.

### AST JSON format

```json
{
  "kind": "Program",
  "items": [
    {
      "kind": "Function",
      "name": "main",
      "params": [],
      "return_type": null,
      "body": {
        "kind": "Block",
        "stmts": [
          {
            "kind": "Let",
            "name": "x",
            "type": "Int",
            "value": {
              "kind": "IntLiteral",
              "value": 42
            }
          }
        ]
      }
    }
  ]
}
```

---

## 6. Package Metadata

### Machine-readable package metadata

```bash
mink metadata --json
```

```json
{
  "name": "my-project",
  "version": "0.1.0",
  "edition": "2026",
  "license": "Apache-2.0",
  "description": "A MINK project",
  "dependencies": [
    {
      "name": "mink-std",
      "version": "1.0.0",
      "source": "registry",
      "features": ["collections", "strings"]
    }
  ],
  "features": ["std", "async"],
  "targets": ["x86_64-pc-windows-msvc"],
  "modules": [
    {
      "name": "main",
      "path": "src/main.mink",
      "public": true,
      "exports": ["main"]
    },
    {
      "name": "lib",
      "path": "src/lib.mink",
      "public": true,
      "exports": ["add", "subtract", "Point"]
    }
  ]
}
```

---

## 7. API Documentation Format

### Machine-readable API documentation

```bash
mink doc --json
```

```json
{
  "package": "mink-std",
  "version": "1.0.0",
  "modules": [
    {
      "name": "collections",
      "description": "Collection types for MINK",
      "items": [
        {
          "kind": "struct",
          "name": "Vec",
          "description": "A dynamic array of elements",
          "type_params": ["T"],
          "methods": [
            {
              "name": "push",
              "description": "Append an element to the end of the vector",
              "params": [
                { "name": "self", "type": "&mut Vec<T>", "description": "The vector" },
                { "name": "value", "type": "T", "description": "The element to append" }
              ],
              "returns": null,
              "examples": [
                {
                  "code": "let mut v = Vec::new();\nv.push(42);",
                  "description": "Push an integer to a vector"
                }
              ]
            }
          ]
        }
      ]
    }
  ]
}
```

### Documentation generation

```bash
mink doc --format html    # Generate HTML documentation
mink doc --format json    # Generate JSON metadata
mink doc --format markdown # Generate Markdown documentation
```

---

## 8. Examples Discoverable by Agents

### Example index

```bash
mink examples --json
```

```json
{
  "examples": [
    {
      "name": "hello_world",
      "path": "examples/hello_world.mink",
      "description": "A simple hello world program",
      "tags": ["basic", "intro"]
    },
    {
      "name": "fibonacci",
      "path": "examples/fibonacci.mink",
      "description": "Compute Fibonacci numbers recursively",
      "tags": ["recursion", "math"]
    }
  ]
}
```

### Package examples

```bash
mink doc --package mink-json --examples --json
```

```json
{
  "package": "mink-json",
  "examples": [
    {
      "name": "parse_json",
      "path": "examples/parse.mink",
      "description": "Parse a JSON string into a value",
      "code": "use mink_json::{parse, Value};\n\nfn main() {\n    let json = parse(\"{\\\"name\\\": \\\"MINK\\\"}\");\n    match json {\n        Result::Ok(value) => rt_print_str(\"Parsed!\"),\n        Result::Err(e) => rt_print_str(\"Parse error\"),\n    }\n}"
    }
  ]
}
```

---

## 9. Deterministic Builds

### Guarantees
1. Same source + same compiler version → same output
2. No non-deterministic behavior (HashMap ordering, file timestamps, etc.)
3. Build output is identical across machines

### Build verification

```bash
mink build main.mink --verify-deterministic
```

### Deterministic test results
- Tests are ordered deterministically
- Random number generators use fixed seeds (unless explicitly seeded)
- Time-dependent tests use mock time

---

## 10. Deterministic Tests

### Test ordering
- Tests run in a deterministic order (alphabetical by default)
- Test execution is reproducible across runs
- Test output is identical across machines

### Test isolation
- Each test runs in isolation (no shared state)
- Tests cannot affect each other's results
- Test failures are reported with exact reproduction steps

---

## 11. Fast Compile/Test Loops

### Compilation speed targets
- Single file: < 100ms
- Small project (10 files): < 1s
- Medium project (100 files): < 10s
- Large project (1000 files): < 60s

### Test speed targets
- Single test: < 10ms
- Small test suite (100 tests): < 1s
- Medium test suite (1000 tests): < 10s

### Incremental compilation (V2+)
- Only recompile changed files
- Cache intermediate results
- Parallel compilation of independent modules

---

## 12. Dependency Discovery

### Dependency graph as JSON

```bash
mink deps --json
```

```json
{
  "root": "my-project 0.1.0",
  "dependencies": [
    {
      "name": "mink-std",
      "version": "1.0.0",
      "source": "registry",
      "dependencies": []
    },
    {
      "name": "mink-json",
      "version": "0.3.0",
      "source": "registry",
      "dependencies": [
        { "name": "mink-std", "version": "1.0.0" }
      ]
    }
  ]
}
```

### Dependency impact analysis

```bash
mink deps --impact mink-std --json
```

```json
{
  "target": "mink-std 1.0.0",
  "dependents": [
    "my-project 0.1.0",
    "mink-json 0.3.0"
  ],
  "total_dependents": 2
}
```

---

## 13. Automated API Documentation

### Documentation generation from source

```bash
mink doc --format html --output docs/
mink doc --format json --output docs/api.json
mink doc --format markdown --output docs/
```

### Documentation metadata

```json
{
  "package": "my-project",
  "version": "0.1.0",
  "modules": [
    {
      "name": "math",
      "description": "Mathematical operations",
      "public_items": 12,
      "documented_items": 12,
      "coverage": 1.0
    }
  ],
  "total_items": 42,
  "documented_items": 40,
  "documentation_coverage": 0.952
}
```

---

## 14. Generated Bindings

### Binding generation for all target languages

```bash
mink bind --target c src/lib.mink > bindings/lib.h
mink bind --target c++ src/lib.mink > bindings/lib.hpp
mink bind --target python src/lib.mink > bindings/mink_module.py
mink bind --target csharp src/lib.mink > bindings/MinkBindings.cs
mink bind --target rust src/lib.mink > bindings/mink_bindings.rs
mink bind --target go src/lib.mink > bindings/mink_bindings.go
mink bind --target java src/lib.mink > bindings/MinkNative.java
```

### Binding metadata

```json
{
  "source": "my-project",
  "target": "python",
  "output": "bindings/mink_module.py",
  "exported_functions": 12,
  "exported_types": 5,
  "generated_at": "2026-08-25T12:00:00Z"
}
```

---

## 15. Code-Generation Support

### MINK source generation

```bash
mink generate --template struct --name Point --fields "x:Int,y:Int" > point.mink
mink generate --template impl --type Point > point_impl.mink
```

### AI agent code generation workflow
1. Agent reads project metadata (`mink metadata --json`)
2. Agent reads API documentation (`mink doc --json`)
3. Agent generates MINK code
4. Agent validates with `mink check --json`
5. Agent runs tests with `mink test --json`
6. Agent iterates until all checks pass

---

## 16. Safe Compiler Invocation

### Compiler as a library

```rust
// Rust API for the MINK compiler
pub fn check(source: &str) -> DiagnosticReport;
pub fn build(source: &str, target: Target) -> BuildResult;
pub fn ast(source: &str) -> AstNode;
pub fn hir(source: &str) -> HirProgram;
pub fn mir(source: &str) -> MirProgram;
```

### Compiler process isolation (V2+)
- The compiler runs as a separate process
- Communication via JSON-RPC or stdio
- Timeout and resource limits
- Crash recovery

### Compiler sandboxing
- Read-only access to source files
- Write access to output directory only
- No network access
- No environment variable access (except PATH)

---

## 17. Agent-Friendly CLI

### CLI design principles
1. **Deterministic output.** Same input → same output.
2. **Machine-readable.** `--json` flag on all commands.
3. **Fast.** Sub-second response for most commands.
4. **Informative.** Error messages include context and suggestions.
5. **Non-interactive.** No prompts or interactive input required.

### CLI commands for agents

| Command | Purpose | JSON output |
|---------|---------|-------------|
| `mink check --json` | Validate source | Yes |
| `mink build --json` | Compile source | Yes |
| `mink test --json` | Run tests | Yes |
| `mink deps --json` | Dependency graph | Yes |
| `mink metadata --json` | Project metadata | Yes |
| `mink doc --json` | API documentation | Yes |
| `mink ast --json` | AST inspection | Yes |
| `mink hir --json` | HIR inspection | Yes |
| `mink mir --json` | MIR inspection | Yes |
| `mink explain <code>` | Error explanation | Yes |

---

## 18. Machine-Readable Project Metadata

### Project introspection

```bash
mink info --json
```

```json
{
  "compiler_version": "1.0.0",
  "language_version": "2026",
  "target": "x86_64-pc-windows-msvc",
  "package": {
    "name": "my-project",
    "version": "0.1.0"
  },
  "dependencies": {
    "count": 3,
    "packages": ["mink-std", "mink-json", "my-utils"]
  },
  "modules": {
    "count": 5,
    "files": ["src/main.mink", "src/lib.mink", "src/utils.mink"]
  },
  "tests": {
    "count": 42,
    "passed": 40,
    "failed": 1,
    "ignored": 1
  }
}
```

---

## 19. Two-Approach Analysis

### Approach A: Real infrastructure for AI agents (CHOSEN)

| Criterion | Assessment |
|-----------|------------|
| **Pros** | Objectively useful, durable, benefits all developers, not just AI |
| **Cons** | More upfront design, requires implementation effort |
| **Complexity** | Medium |
| **Performance** | Good (JSON serialization is fast) |
| **Security** | Good (read-only inspection, no code execution) |
| **Compatibility** | Excellent — standard tools work with JSON |
| **Maintainability** | Good — JSON schemas are stable |
| **Ecosystem impact** | Maximum — makes MINK the most AI-friendly language |

### Approach B: AI gimmick (e.g., natural language interface)

| Criterion | Assessment |
|-----------|------------|
| **Pros** | Easy to demo, impressive initially |
| **Cons** | Fragile, unreliable, not useful for real work, becomes technical debt |
| **Complexity** | High (LLM integration, prompt engineering) |
| **Performance** | Poor (network calls, latency) |
| **Security** | Poor (prompt injection, data leakage) |
| **Compatibility** | Poor — requires specific LLM versions |
| **Maintainability** | Poor — breaks with LLM updates |
| **Ecosystem impact** | Negative — embarrasses the project when it fails |

### Decision: **Approach A**

**Reasoning:** AI agents need reliable, fast, deterministic tooling. Natural language interfaces are fragile and unreliable. The right approach is to make MINK's existing tooling machine-readable: JSON output, stable error codes, structured metadata. This benefits ALL developers (human and AI) and is durable over time.

---

*This specification is part of the MINK Ecosystem Architecture Design Pack (Session 50).*
*Do NOT implement until the design is frozen and reviewed.*
