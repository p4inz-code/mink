# MINK Library Priority Framework

**Version:** 0.1.0 (Design Draft)
**Date:** August 25, 2026
**Status:** DESIGN ONLY — not implemented
**Scope:** Scoring system and ranked list of candidate ecosystem libraries

---

## 1. Goals

1. **One at a time.** We build libraries ONE AT A TIME. Each library is completed, tested, documented, and audited before the next begins.

2. **Data-driven selection.** Library selection is based on a scoring system, not gut feeling.

3. **Maximum impact.** The first library must be the one that provides the most value to the most users.

4. **Foundation first.** Libraries that enable other libraries are prioritized.

5. **Interoperability value.** Libraries that work well with Python, C++, C#, and AI agents are prioritized.

---

## 2. Scoring System

### Scoring criteria

| Criterion | Weight | Description |
|-----------|--------|-------------|
| **Developer demand** | 20% | How many developers need this library? |
| **Python integration value** | 10% | How useful is this for Python interop? |
| **C/C++ integration value** | 10% | How useful is this for C/C++ interop? |
| **C# integration value** | 5% | How useful is this for C# interop? |
| **AI-agent usefulness** | 10% | How useful is this for AI coding agents? |
| **Performance advantage** | 10% | Does MINK offer a performance advantage here? |
| **Safety advantage** | 10% | Does MINK offer a safety advantage here? |
| **Ecosystem leverage** | 15% | Does this enable other libraries? |
| **Cross-platform importance** | 5% | Is this critical for cross-platform support? |
| **Implementation feasibility** | 5% | How feasible is this to implement? |

### Scoring scale

| Score | Meaning |
|-------|---------|
| 10 | Critical — must have |
| 8 | High value — very important |
| 6 | Medium value — useful |
| 4 | Low value — nice to have |
| 2 | Minimal value — rarely needed |
| 0 | Not applicable |

### Weighted score calculation

```
Score = Σ (criterion_score × criterion_weight)
```

Maximum possible score: 10.0

---

## 3. Candidate Domains

### Candidate list

| # | Domain | Description | Status |
|---|--------|-------------|--------|
| 1 | **Core primitives** | Option/Result methods, Vec methods, string formatting | V1 partial |
| 2 | **Filesystem** | File I/O, directory operations, path manipulation | Not started |
| 3 | **JSON** | JSON parsing and serialization | Not started |
| 4 | **Math** | Basic math operations (abs, min, max, pow, sqrt) | Not started |
| 5 | **Time** | Duration, Instant, SystemTime | Not started |
| 6 | **Process** | Process spawn, environment variables, CLI args | Not started |
| 7 | **Testing** | Test framework, assertions, test runner | Not started |
| 8 | **CLI** | Argument parsing, help generation | Not started |
| 9 | **Strings** | String manipulation (split, trim, contains) | Partial (runtime) |
| 10 | **Collections** | HashMap, HashSet, BTreeMap | Not started |
| 11 | **Networking** | TCP/UDP sockets | Not started |
| 12 | **Encoding** | Base64, hex, TOML, CSV | Not started |
| 13 | **Crypto** | Hash functions (SHA-256) | Not started |
| 14 | **Concurrency** | Threads, mutexes, channels | Not started |
| 15 | **Logging** | Structured logging | Not started |
| 16 | **Regex** | Regular expressions | Not started |
| 17 | **Compression** | Gzip, zlib | Not started |
| 18 | **Serialization** | Generic serialization framework | Not started |

---

## 4. Scoring Each Candidate

### 1. Core primitives (Option/Result methods, Vec methods, string formatting)

| Criterion | Score | Reasoning |
|-----------|-------|-----------|
| Developer demand | 10 | Every developer needs these |
| Python integration | 6 | Basic interop value |
| C/C++ integration | 6 | Basic interop value |
| C# integration | 4 | Basic interop value |
| AI-agent usefulness | 8 | Agents need to use these methods |
| Performance advantage | 6 | MINK's ownership model is safe |
| Safety advantage | 8 | Ownership prevents leaks |
| Ecosystem leverage | 10 | Every other library depends on these |
| Cross-platform importance | 8 | Required on all platforms |
| Implementation feasibility | 10 | Already partially implemented |
| **Weighted score** | **8.3** | |

### 2. Filesystem

| Criterion | Score | Reasoning |
|-----------|-------|-----------|
| Developer demand | 9 | Almost every program needs file I/O |
| Python integration | 8 | Python devs expect filesystem access |
| C/C++ integration | 8 | C/C++ devs expect filesystem access |
| C# integration | 6 | .NET devs expect filesystem access |
| AI-agent usefulness | 8 | Agents need to read/write files |
| Performance advantage | 6 | MINK's safety model prevents file handle leaks |
| Safety advantage | 8 | RAII-style file handles |
| Ecosystem leverage | 8 | Many libraries depend on filesystem |
| Cross-platform importance | 10 | Critical for cross-platform |
| Implementation feasibility | 6 | Requires platform abstraction |
| **Weighted score** | **8.0** | |

### 3. JSON

| Criterion | Score | Reasoning |
|-----------|-------|-----------|
| Developer demand | 9 | JSON is ubiquitous |
| Python integration | 10 | Python devs use JSON constantly |
| C/C++ integration | 8 | C/C++ devs use JSON frequently |
| C# integration | 8 | .NET devs use JSON frequently |
| AI-agent usefulness | 10 | Agents need to parse/generate JSON |
| Performance advantage | 8 | MINK can be faster than Python for JSON |
| Safety advantage | 8 | Ownership prevents buffer overflows |
| Ecosystem leverage | 8 | Many libraries depend on JSON |
| Cross-platform importance | 8 | Required on all platforms |
| Implementation feasibility | 6 | JSON parsing is well-understood |
| **Weighted score** | **8.5** | |

### 4. Math

| Criterion | Score | Reasoning |
|-----------|-------|-----------|
| Developer demand | 7 | Many programs need basic math |
| Python integration | 6 | Python has math built-in |
| C/C++ integration | 6 | C has math built-in |
| C# integration | 4 | .NET has math built-in |
| AI-agent usefulness | 6 | Agents need basic math |
| Performance advantage | 6 | MINK can be faster for numeric code |
| Safety advantage | 6 | Overflow checking |
| Ecosystem leverage | 6 | Some libraries depend on math |
| Cross-platform importance | 6 | Required on all platforms |
| Implementation feasibility | 10 | Very simple to implement |
| **Weighted score** | **6.6** | |

### 5. Time

| Criterion | Score | Reasoning |
|-----------|-------|-----------|
| Developer demand | 7 | Many programs need time operations |
| Python integration | 6 | Python has time built-in |
| C/C++ integration | 6 | C has time built-in |
| C# integration | 4 | .NET has time built-in |
| AI-agent usefulness | 4 | Agents rarely need time operations |
| Performance advantage | 4 | Time operations are platform-specific |
| Safety advantage | 4 | Time operations are safe by default |
| Ecosystem leverage | 6 | Some libraries depend on time |
| Cross-platform importance | 8 | Critical for cross-platform |
| Implementation feasibility | 6 | Requires platform abstraction |
| **Weighted score** | **5.8** | |

### 6. Process

| Criterion | Score | Reasoning |
|-----------|-------|-----------|
| Developer demand | 7 | Many programs need process management |
| Python integration | 6 | Python has subprocess |
| C/C++ integration | 6 | C has system/exec |
| C# integration | 4 | .NET has Process class |
| AI-agent usefulness | 6 | Agents need to run commands |
| Performance advantage | 4 | Process management is platform-specific |
| Safety advantage | 6 | Ownership prevents process handle leaks |
| Ecosystem leverage | 6 | Some libraries depend on process |
| Cross-platform importance | 8 | Critical for cross-platform |
| Implementation feasibility | 6 | Requires platform abstraction |
| **Weighted score** | **6.0** | |

### 7. Testing

| Criterion | Score | Reasoning |
|-----------|-------|-----------|
| Developer demand | 9 | Every developer needs testing |
| Python integration | 6 | Python has pytest |
| C/C++ integration | 6 | C/C++ has gtest, catch2 |
| C# integration | 6 | .NET has xUnit, NUnit |
| AI-agent usefulness | 10 | Agents need to run tests |
| Performance advantage | 4 | Testing is not performance-critical |
| Safety advantage | 4 | Testing is not safety-critical |
| Ecosystem leverage | 8 | Every library needs tests |
| Cross-platform importance | 6 | Required on all platforms |
| Implementation feasibility | 6 | Requires test runner infrastructure |
| **Weighted score** | **7.0** | |

### 8. CLI

| Criterion | Score | Reasoning |
|-----------|-------|-----------|
| Developer demand | 7 | Many programs are CLI tools |
| Python integration | 6 | Python has argparse |
| C/C++ integration | 6 | C/C++ has getopt |
| C# integration | 4 | .NET has System.CommandLine |
| AI-agent usefulness | 6 | Agents need to parse CLI args |
| Performance advantage | 4 | CLI parsing is not performance-critical |
| Safety advantage | 4 | CLI parsing is safe by default |
| Ecosystem leverage | 6 | Some libraries are CLI tools |
| Cross-platform importance | 6 | Required on all platforms |
| Implementation feasibility | 8 | Well-understood problem |
| **Weighted score** | **6.0** | |

### 9. Strings (extended operations)

| Criterion | Score | Reasoning |
|-----------|-------|-----------|
| Developer demand | 9 | Every program uses strings |
| Python integration | 8 | Python has rich string operations |
| C/C++ integration | 6 | C has limited string operations |
| C# integration | 6 | .NET has rich string operations |
| AI-agent usefulness | 8 | Agents need string manipulation |
| Performance advantage | 8 | MINK can be faster than Python for strings |
| Safety advantage | 8 | Ownership prevents buffer overflows |
| Ecosystem leverage | 10 | Every library uses strings |
| Cross-platform importance | 8 | Required on all platforms |
| Implementation feasibility | 6 | Unicode is complex |
| **Weighted score** | **8.0** | |

### 10. Collections (HashMap, HashSet)

| Criterion | Score | Reasoning |
|-----------|-------|-----------|
| Developer demand | 9 | Every program uses hash maps |
| Python integration | 8 | Python has dict, set |
| C/C++ integration | 6 | C++ has std::unordered_map |
| C# integration | 6 | .NET has Dictionary, HashSet |
| AI-agent usefulness | 8 | Agents need hash maps |
| Performance advantage | 8 | MINK can be faster than Python for hash maps |
| Safety advantage | 8 | Ownership prevents use-after-free |
| Ecosystem leverage | 8 | Many libraries depend on hash maps |
| Cross-platform importance | 6 | Required on all platforms |
| Implementation feasibility | 6 | Hash table implementation is complex |
| **Weighted score** | **7.6** | |

### 11. Networking

| Criterion | Score | Reasoning |
|-----------|-------|-----------|
| Developer demand | 8 | Many programs need networking |
| Python integration | 6 | Python has socket |
| C/C++ integration | 6 | C has socket |
| C# integration | 6 | .NET has Socket |
| AI-agent usefulness | 4 | Agents rarely need networking |
| Performance advantage | 8 | MINK can be faster than Python for networking |
| Safety advantage | 8 | Ownership prevents socket leaks |
| Ecosystem leverage | 6 | Some libraries depend on networking |
| Cross-platform importance | 8 | Critical for cross-platform |
| Implementation feasibility | 4 | Networking is complex |
| **Weighted score** | **6.6** | |

### 12. Encoding (Base64, hex, TOML, CSV)

| Criterion | Score | Reasoning |
|-----------|-------|-----------|
| Developer demand | 7 | Many programs need encoding |
| Python integration | 6 | Python has base64, json |
| C/C++ integration | 4 | C has limited encoding support |
| C# integration | 4 | .NET has encoding support |
| AI-agent usefulness | 6 | Agents need encoding |
| Performance advantage | 6 | MINK can be faster than Python |
| Safety advantage | 6 | Ownership prevents buffer overflows |
| Ecosystem leverage | 6 | Some libraries depend on encoding |
| Cross-platform importance | 6 | Required on all platforms |
| Implementation feasibility | 6 | Encoding is well-understood |
| **Weighted score** | **6.0** | |

### 13. Crypto (SHA-256)

| Criterion | Score | Reasoning |
|-----------|-------|-----------|
| Developer demand | 6 | Some programs need crypto |
| Python integration | 4 | Python has hashlib |
| C/C++ integration | 4 | C has OpenSSL |
| C# integration | 4 | .NET has crypto |
| AI-agent usefulness | 4 | Agents rarely need crypto |
| Performance advantage | 8 | MINK can be faster than Python |
| Safety advantage | 8 | Ownership prevents buffer overflows |
| Ecosystem leverage | 6 | Some libraries depend on crypto |
| Cross-platform importance | 6 | Required on all platforms |
| Implementation feasibility | 4 | Crypto is complex and security-critical |
| **Weighted score** | **5.6** | |

### 14. Concurrency

| Criterion | Score | Reasoning |
|-----------|-------|-----------|
| Developer demand | 7 | Many programs need concurrency |
| Python integration | 4 | Python has GIL limitations |
| C/C++ integration | 6 | C/C++ has pthreads |
| C# integration | 6 | .NET has Task, async |
| AI-agent usefulness | 4 | Agents rarely need concurrency |
| Performance advantage | 8 | MINK can be faster than Python |
| Safety advantage | 8 | Ownership prevents data races |
| Ecosystem leverage | 6 | Some libraries depend on concurrency |
| Cross-platform importance | 8 | Critical for cross-platform |
| Implementation feasibility | 4 | Concurrency is complex |
| **Weighted score** | **6.0** | |

### 15. Logging

| Criterion | Score | Reasoning |
|-----------|-------|-----------|
| Developer demand | 7 | Many programs need logging |
| Python integration | 4 | Python has logging |
| C/C++ integration | 4 | C has printf, syslog |
| C# integration | 4 | .NET has Serilog, NLog |
| AI-agent usefulness | 6 | Agents need to understand logs |
| Performance advantage | 4 | Logging is not performance-critical |
| Safety advantage | 4 | Logging is safe by default |
| Ecosystem leverage | 6 | Some libraries depend on logging |
| Cross-platform importance | 6 | Required on all platforms |
| Implementation feasibility | 8 | Logging is well-understood |
| **Weighted score** | **5.4** | |

### 16. Regex

| Criterion | Score | Reasoning |
|-----------|-------|-----------|
| Developer demand | 7 | Many programs need regex |
| Python integration | 6 | Python has re |
| C/C++ integration | 6 | C++ has std::regex |
| C# integration | 4 | .NET has Regex |
| AI-agent usefulness | 6 | Agents need regex |
| Performance advantage | 6 | MINK can be faster than Python |
| Safety advantage | 6 | Ownership prevents ReDoS |
| Ecosystem leverage | 4 | Few libraries depend on regex |
| Cross-platform importance | 6 | Required on all platforms |
| Implementation feasibility | 4 | Regex is complex |
| **Weighted score** | **5.8** | |

### 17. Compression

| Criterion | Score | Reasoning |
|-----------|-------|-----------|
| Developer demand | 5 | Some programs need compression |
| Python integration | 4 | Python has gzip, zlib |
| C/C++ integration | 4 | C has zlib |
| C# integration | 4 | .NET has GZipStream |
| AI-agent usefulness | 2 | Agents rarely need compression |
| Performance advantage | 8 | MINK can be faster than Python |
| Safety advantage | 6 | Ownership prevents buffer overflows |
| Ecosystem leverage | 4 | Few libraries depend on compression |
| Cross-platform importance | 6 | Required on all platforms |
| Implementation feasibility | 4 | Compression is complex |
| **Weighted score** | **4.8** | |

### 18. Serialization

| Criterion | Score | Reasoning |
|-----------|-------|-----------|
| Developer demand | 7 | Many programs need serialization |
| Python integration | 6 | Python has pickle, json |
| C/C++ integration | 6 | C++ has protobuf |
| C# integration | 6 | .NET has System.Text.Json |
| AI-agent usefulness | 6 | Agents need serialization |
| Performance advantage | 6 | MINK can be faster than Python |
| Safety advantage | 6 | Ownership prevents buffer overflows |
| Ecosystem leverage | 6 | Some libraries depend on serialization |
| Cross-platform importance | 6 | Required on all platforms |
| Implementation feasibility | 4 | Serialization is complex |
| **Weighted score** | **6.0** | |

---

## 5. Ranked List

| Rank | Domain | Weighted Score | Rationale |
|------|--------|---------------|-----------|
| **#1** | **JSON** | **8.5** | Highest score. Ubiquitous, AI-friendly, enables many other libraries, high Python/C++/C# interop value. |
| **#2** | **Core primitives** | **8.3** | Foundation for everything. Already partially implemented. Every other library depends on these. |
| **#3** | **Filesystem** | **8.0** | Critical for cross-platform, high developer demand, enables process/networking libraries. |
| **#4** | **Strings (extended)** | **8.0** | High developer demand, high interop value, enables JSON/encoding/regex. |
| **#5** | **Collections (HashMap)** | **7.6** | High developer demand, high interop value, enables many libraries. |

---

## 6. Why #1 is JSON (Not Core Primitives)

### The case for JSON as #1

1. **Highest weighted score (8.5).** JSON scores highest across all criteria.

2. **Ubiquitous demand.** JSON is the de facto data interchange format. Every modern program uses it.

3. **Maximum AI-agent value.** AI agents need to parse and generate JSON constantly. A MINK JSON library makes MINK immediately useful for AI-generated code.

4. **Maximum interop value.** Python, C++, C#, and Rust all have excellent JSON libraries. MINK needs one to be competitive.

5. **Enables other libraries.** Many libraries depend on JSON (HTTP, configuration, logging, serialization).

6. **Performance advantage.** MINK can be faster than Python for JSON parsing, demonstrating MINK's value proposition.

7. **Safety advantage.** MINK's ownership model prevents buffer overflows in JSON parsing, a common security vulnerability.

### The case against core primitives as #1

1. **Already partially implemented.** Option<T>, Result<T,E>, and Vec<T> already exist. Methods can be added incrementally.

2. **Lower interop value.** Core primitives are MINK-specific; they don't directly help with Python/C++/C# interop.

3. **Lower AI-agent value.** AI agents can work without Option/Result methods; they cannot work without JSON.

4. **Lower ecosystem leverage.** Core primitives enable MINK-to-MINK libraries; JSON enables MINK-to-world interop.

### Resolution

Core primitives (#2) and JSON (#1) are complementary. Core primitive methods (Option/Result/Vec) should be implemented as part of the JSON work (since JSON parsing needs Result and Vec). The JSON library provides the external-facing value; core primitive methods provide the internal foundation.

---

## 7. Implementation Order

### Phase 1: Core primitives + JSON (combined)
1. Implement Option<T> methods (`.unwrap()`, `.map()`, `.is_some()`)
2. Implement Result<T,E> methods (`.unwrap()`, `.map()`, `.is_ok()`)
3. Implement Vec<T> methods (`.push()`, `.len()`, `.get()`)
4. Implement string formatting (`format!` or function)
5. Implement JSON parser (recursive descent, zero-copy)
6. Implement JSON serializer (streaming, efficient)
7. Add JSON to the standard library
8. Document, test, benchmark, security audit

### Phase 2: Filesystem + Strings
1. Implement Path type
2. Implement File operations (open, read, write, delete)
3. Implement directory operations (create, list, remove)
4. Implement extended string operations (split, trim, contains, replace)
5. Add filesystem and strings to the standard library
6. Document, test, benchmark, security audit

### Phase 3: Collections + Math
1. Implement HashMap<K,V>
2. Implement HashSet<T>
3. Implement basic math operations (abs, min, max, pow, sqrt)
4. Add collections and math to the standard library
5. Document, test, benchmark, security audit

### Phase 4: Process + CLI + Testing
1. Implement process spawn/wait/kill
2. Implement environment variable access
3. Implement CLI argument parsing
4. Implement test framework (assert, test runner)
5. Add process, CLI, and testing to the standard library
6. Document, test, benchmark, security audit

### Phase 5: Time + Encoding
1. Implement Duration, Instant, SystemTime
2. Implement Base64 encoding/decoding
3. Implement hex encoding/decoding
4. Implement TOML parser/serializer
5. Add time and encoding to the standard library
6. Document, test, benchmark, security audit

---

## 8. Quality Standard for Each Library

Every library must satisfy:

1. **Cross-platform.** Works on Windows, Linux, macOS.
2. **Documentation.** Every public API documented.
3. **Tests.** ≥90% code coverage.
4. **Benchmarks.** Performance-critical APIs have benchmarks.
5. **API stability.** Semantic versioning, deprecation policy.
6. **Error handling.** All errors use Result<T,E>.
7. **Ownership correctness.** No memory leaks, no use-after-free.
8. **Thread safety.** Documented thread safety guarantees.
9. **Security review.** Security-sensitive APIs reviewed.
10. **Fuzzing.** Parser/serialization libraries fuzz-tested.
11. **Examples.** Working examples for major APIs.
12. **AI discoverability.** Metadata machine-readable.
13. **Dependency minimization.** Minimal external dependencies.
14. **Deterministic builds.** Reproducible compilation.

---

*This specification is part of the MINK Ecosystem Architecture Design Pack (Session 50).*
*Do NOT implement until the design is frozen and reviewed.*
