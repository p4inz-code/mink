# MINK COMPREHENSIVE VERIFICATION STANDARD

VERSION: POST-SESSION-100
STATUS: MANDATORY FOR ALL FUTURE MINK WORK

======================================================================
0. PURPOSE
======================================================================

This standard defines how every MINK feature, subsystem, runtime service,
compiler capability, standard-library module, package feature, platform
feature, and shipped artifact must be verified.

The objective is not:

"Does the feature work?"

The objective is:

"Can this feature behave correctly, safely, deterministically, and
predictably across every realistic class of valid input, invalid input,
boundary condition, failure path, ownership state, environment, packaging
state, and native execution condition?"

MINK is being developed as production systems software.

Passing a unit test is not enough.

Compiling successfully is not enough.

Working once is not enough.

Working only inside the repository is not enough.

Working only under the coding-agent environment is not enough.

A capability may only be called VERIFIED when evidence supports the claim.

======================================================================
1. CURRENT PROJECT STATE
======================================================================

Current verified baseline:

* Windows x86_64 base: COMPLETE / STABLE
* Windows Python official capability parity: PARTIAL
* Linux development: FROZEN
* Linux behavior must not be modified unless explicitly unfrozen later
* Current Session 100 final commit: 218bc1b
* Current deterministic test baseline: 2574 passing
* Current P0 introduced by active Windows work: 0
* Current P1 introduced by active Windows work: 0

Session 99 verified:

* environment get/has/set/remove
* argv
* stdin read-all
* sleep
* stderr write
* float to Str
* str_format
* bundled npm stdlib
* automatic module resolution

Session 100 verified:

* runtime error location metadata R06
* home-directory discovery W14

All future sessions must preserve these verified capabilities.

======================================================================
2. PERMANENT TESTING PRINCIPLE
==============================

Every feature must be tested from multiple directions.

At minimum, consider:

A. normal use
B. minimum valid input
C. maximum practical input
D. empty input
E. zero input
F. negative input
G. very small input
H. very large input
I. boundary input
J. invalid input
K. malformed input
L. missing input
M. duplicate input
N. repeated use
O. long-running use
P. partial failure
Q. total failure
R. resource exhaustion
S. ownership
T. lifetime
U. memory leak behavior
V. double-free resistance
W. use-after-free resistance
X. integer overflow
Y. integer underflow
Z. path edge cases
AA. spaces
AB. punctuation
AC. Unicode
AD. environment differences
AE. multiple working directories
AF. copied executable behavior
AG. npm-installed behavior
AH. standalone behavior
AI. release-build behavior
AJ. deterministic repeated behavior
AK. interaction with other subsystems
AL. cleanup after failure
AM. process exit code
AN. stdout/stderr separation
AO. native OS behavior
AP. malformed embedded metadata
AQ. source path behavior
AR. repeated compiler invocation
AS. cold clean-user environment

If a category does not apply, document why.

Do not silently skip applicable categories.

======================================================================
3. THE VERIFICATION LADDER
==========================

Use the following verification ladder.

LEVEL 0 - EXISTS
Source code exists.

LEVEL 1 - COMPILES
Compiler/project builds.

LEVEL 2 - UNIT TESTED
Internal unit tests pass.

LEVEL 3 - INTEGRATION TESTED
Subsystem interactions pass.

LEVEL 4 - NATIVE EXECUTION VERIFIED
A generated native Windows PE executes and proves the behavior.

LEVEL 5 - FAILURE VERIFIED
Invalid and failure paths are exercised.

LEVEL 6 - OWNERSHIP VERIFIED
Allocation, lifetime, cleanup, and leak behavior are tested.

LEVEL 7 - STRESS VERIFIED
Repeated and large workloads are exercised.

LEVEL 8 - CLEAN INSTALL VERIFIED
Capability works from a fresh npm installation.

LEVEL 9 - STANDALONE VERIFIED
Generated executable works outside the repository and installation.

LEVEL 10 - REAL APPLICATION VERIFIED
A realistic MINK program uses the capability successfully.

A feature should not be labeled simply VERIFIED unless all applicable
levels have been satisfied.

If some levels do not apply, state why.

======================================================================
4. NATIVE EXECUTION IS MANDATORY
================================

Session 100 proved again that source inspection and compiler tests are not
enough.

Two W14 bugs survived source-level reasoning:

* missing test rax,rax after Windows fallback queries
* dead copy loops caused by misplaced jump guards

These bugs were discovered only by native execution.

Therefore:

Any runtime service, code generator change, emitter change, ABI change,
allocator change, OS service, compiler lowering behavior, or generated-code
change must be executed in a real generated Windows PE whenever technically
possible.

Never conclude correctness only from:

* emitter source inspection
* generated byte inspection
* Rust unit tests
* IR assertions
* compiler success

Native execution is mandatory evidence.

======================================================================
5. EMITTER AND CODEGEN VERIFICATION
===================================

Emitter changes are high-risk.

For every emitter/codegen change test:

* generated code executes
* register assumptions
* condition flags
* branch conditions
* stack alignment
* stack cleanup
* calling convention
* preserved registers
* volatile registers
* return register values
* zero/null checks
* allocation failures
* OS API failure values
* loop entry
* loop termination
* off-by-one behavior
* empty loops
* one-element loops
* exact-length copies
* string length handling
* ownership transfer
* all error paths

Do not assume flags survive across instructions.

Explicitly test values before conditional branches when required.

Do not assume API return values leave useful condition flags.

Do not assume a copy loop is reachable merely because its instructions exist.

Native tests must exercise every newly introduced branch whenever practical.

======================================================================
6. MEMORY AND ALLOCATOR TESTING
===============================

Every memory-affecting change must test:

* zero-length allocation
* smallest allocation
* normal allocation
* large allocation
* repeated allocation
* repeated free
* alternating allocation/free
* fragmentation
* free-list reuse
* reuse after differently sized frees
* near-arena-limit allocation
* allocation failure
* error-path cleanup
* caller-owned values
* runtime-owned values
* ownership transfer
* nested calls
* repeated calls
* early exit
* failure after allocation
* multiple returned strings
* leak checker

Every allocator bug discovered must receive a permanent regression test.

Never accept:

"the leak is small"

or:

"the process exits anyway"

as justification.

======================================================================
7. STRING TESTING
=================

Test every applicable string operation with:

* empty string
* one byte
* one character
* ASCII
* spaces
* punctuation
* quotes
* braces
* backslashes
* newline
* carriage return
* tab
* repeated whitespace
* long strings
* very long strings
* UTF-8
* non-ASCII text
* embedded NUL where contract permits
* malformed input
* exact boundary lengths
* repeated operations

Verify:

* exact length
* exact bytes
* no accidental C-string assumptions
* ownership
* lifetime
* allocation
* free behavior
* output equality
* deterministic formatting

MINK strings are length-based.

Any interaction with Windows APIs that require NUL termination must perform
the necessary conversion explicitly and safely.

======================================================================
8. FLOAT AND NUMBER TESTING
===========================

For numeric conversion and formatting test:

* zero
* negative zero
* positive integer-like float
* negative integer-like float
* fractional values
* very small finite values
* very large finite values
* precision boundaries
* repeated conversion
* NaN if supported
* positive infinity if supported
* negative infinity if supported

For integer arithmetic test:

* zero
* one
* negative one
* min value
* max value
* overflow boundaries
* division by zero
* remainder by zero
* signed edge cases

Runtime faults must use the expected error path rather than uncontrolled OS
faults where MINK defines a runtime error.

======================================================================
9. FILESYSTEM TESTING
=====================

Test:

* empty file
* one-byte file
* small file
* large file
* nonexistent file
* file where directory expected
* directory where file expected
* missing parent
* nested directories
* spaces
* punctuation
* Unicode path where supported
* relative path
* absolute path
* dot
* dot-dot
* repeated separators
* overwrite
* append
* delete
* rename
* permission failure
* repeated operations
* copied executable working directory
* source directory different from current directory

For every filesystem call verify:

* return value
* ownership
* cleanup
* exact bytes
* exact path semantics
* failure behavior

======================================================================
10. WINDOWS PATH TESTING
========================

Windows path behavior must explicitly include:

* spaces
* drive letters
* root paths
* relative paths
* absolute paths
* current directory changes
* source path different from executable path
* npm installation path different from project path
* standalone executable path different from source path
* directory containing spaces
* file containing spaces
* mixed slash inputs where accepted
* Unicode path behavior where APIs support it

Long path behavior remains a separate capability until explicitly verified.

Do not claim full Unicode Windows path support while relevant APIs remain
ANSI-only.

======================================================================
11. ENVIRONMENT VARIABLE TESTING
================================

Test environment operations with:

* missing variable
* existing variable
* empty value
* short value
* long value
* spaces
* punctuation
* Unicode where supported by current Windows API layer
* overwrite
* delete
* repeated set/get
* repeated remove
* get after remove
* independent variable names
* process cleanup

Tests must use unique names.

Tests must restore or remove variables after execution.

Do not pollute the user's permanent environment.

======================================================================
12. HOME AND TEMP DIRECTORY TESTING
===================================

For home-directory discovery verify:

* USERPROFILE available
* USERPROFILE missing
* HOMEDRIVE + HOMEPATH available
* all sources missing
* repeated calls
* ownership
* leak behavior
* path with spaces

For future temp-directory discovery verify:

* Windows native API behavior
* returned path length
* trailing separator behavior
* repeated calls
* ownership
* nonexistence assumptions
* clean failure
* spaces
* realistic user environment

Do not hardcode C:\Users or C:\Temp.

======================================================================
13. PROCESS TESTING
===================

Test:

* valid executable
* nonexistent executable
* zero arguments
* one argument
* many arguments
* empty argument
* spaces
* punctuation
* Unicode where supported
* normal stdout
* empty stdout
* small stdout
* pipe-boundary stdout
* large stdout
* very large stdout
* normal stderr
* large stderr
* simultaneous stdout/stderr
* exit 0
* exit non-zero
* repeated execution
* long-running child
* child writing continuously
* child output larger than pipe capacity
* cleanup after process creation failure
* cleanup after pipe creation failure

The Session 97 deadlock family must remain permanently covered.

Never return to:

wait for child -> read output

if that can allow pipe deadlock.

======================================================================
14. STDIN TESTING
=================

For stdin APIs test:

* empty input
* EOF immediately
* one byte
* one line
* multiple lines
* no trailing newline
* trailing newline
* long input
* redirected input
* pipe input
* exact byte preservation
* repeated process execution
* no hangs

Current read-all semantics must remain documented until richer stdin APIs are
implemented.

======================================================================
15. STDOUT AND STDERR TESTING
=============================

Verify:

* stdout exact bytes
* stderr exact bytes
* empty output
* multiple writes
* long writes
* stdout/stderr separation
* process capture
* exit status
* order semantics where guaranteed
* no accidental stream merging

Do not claim global ordering guarantees across independent OS streams unless
MINK explicitly provides them.

======================================================================
16. NETWORK TESTING
===================

For TCP, UDP, and HTTP test applicable cases:

* initialization
* operation without initialization
* valid host
* invalid host
* valid port
* invalid port
* refused connection
* peer close
* premature peer close
* zero-byte payload
* one-byte payload
* small payload
* large payload
* fragmented receive
* multiple receives
* multiple sends
* repeated connections
* repeated close
* error cleanup
* HTTP GET
* HTTP POST
* empty POST
* large POST
* HTTP 200
* HTTP 404
* other response codes
* missing body
* large body
* malformed response
* incomplete response

Local deterministic test servers should be preferred over external internet
services for regression tests.

External internet tests may exist as separate integration tests but cannot
be the only evidence.

======================================================================
17. CRYPTO AND RANDOM TESTING
=============================

For cryptographic and random APIs verify:

* expected output length
* empty input
* small input
* large input
* known vectors where applicable
* repeated operations
* exact encoding
* deterministic algorithms reproduce vectors
* random APIs do not accidentally become deterministic
* error paths
* ownership
* leak behavior

Cryptographic correctness claims require known-vector evidence where
applicable.

======================================================================
18. CLI TESTING
===============

Test:

* no arguments
* --help
* --version
* -v
* -V
* valid command
* invalid command
* missing argument
* extra argument
* nonexistent source
* directory used as source
* empty source
* syntax error
* type error
* runtime error
* relative path
* absolute path
* path with spaces
* source outside current directory
* execution from another cwd
* repeated commands

Verify:

* stdout
* stderr
* exit code
* no panic
* no hang
* stable diagnostics

======================================================================
19. MODULE RESOLUTION TESTING
=============================

Current expected normal search behavior must remain covered:

1. source-directory sibling
2. bundled stdlib near installed compiler
3. cwd/stdlib fallback where contract permits

Test:

* local module
* bundled stdlib module
* missing module
* module outside cwd
* source path with spaces
* clean npm install
* completely different cwd
* no repository checkout

No normal user workflow may require a developer-machine absolute path.

======================================================================
20. NPM PACKAGE TESTING
=======================

For every release-impacting session verify:

* npm pack succeeds
* expected files included
* no unwanted development files
* compiler executable included
* bundled stdlib included
* package version correct
* CLI version correct
* executable hash corresponds to intended release binary
* clean install in new directory
* command shims work
* --version
* -v
* -V
* check
* build
* run
* module import
* standalone executable production

Do not test only against an existing node_modules directory.

======================================================================
21. STANDALONE EXECUTABLE TESTING
=================================

A standalone proof must run:

* outside the repository
* outside the npm project
* from an empty directory
* from a path containing spaces
* without Rust
* without Cargo
* without source checkout
* without development stdlib
* without repo-relative paths

Where relevant also test:

* args
* stdin
* stdout
* stderr
* environment
* filesystem
* runtime diagnostics

A single copied main.exe must remain functional for capabilities that are
supposed to be standalone.

======================================================================
22. RUNTIME ERROR LOCATION TESTING
==================================

R06 is VERIFIED as of Session 100 and must remain protected.

Test runtime diagnostics for:

* exact source filename
* exact source line
* callee failure reports callee location
* top-level failure
* nested call failure
* copied executable
* source unavailable at runtime
* path with spaces
* deterministic repeats
* success produces no location
* errors without valid source location do not fabricate one
* leak/runtime-global failures without a source location remain safe

Embedded metadata must not require source files at runtime.

Malformed or missing metadata must not cause a secondary crash.

======================================================================
23. SOURCE PATH AND DETERMINISTIC BUILD RULE
============================================

R06 embeds source path information.

Therefore byte-identical output may legitimately differ if compilation input
paths differ.

For deterministic golden tests:

* compile repeated samples from a fixed path
* keep all other inputs equal
* verify byte-identical output under identical compilation conditions

Do not "fix" correct source metadata by removing paths simply to satisfy an
old deterministic-build test.

When metadata intentionally depends on source path, deterministic tests must
control that input.

======================================================================
24. DETERMINISM TESTING
=======================

Important deterministic operations must be repeated.

Test:

* repeated compiler invocation
* repeated native execution
* repeated allocation/free
* repeated process launch
* repeated formatting
* repeated module resolution
* repeated diagnostics

For deterministic behavior compare:

* bytes
* output
* exit code
* diagnostic location
* generated artifact where applicable

If results differ, explain exactly why.

======================================================================
25. FLAKY TEST POLICY
=====================

A failing test must never automatically be dismissed as flaky.

Required process:

1. reproduce
2. isolate
3. determine whether product or test infrastructure
4. inspect timing/resource sensitivity
5. run independently
6. run serially
7. run under full suite load
8. classify with evidence
9. fix product issues
10. document genuine infrastructure flakes

Known Session 98 loopback timing tests may be executed single-threaded for
deterministic full-suite verification only because their behavior has already
been investigated and documented.

This exception must not be generalized to new failures.

Never solve a product failure by merely switching the entire suite to
single-threaded mode without root-cause analysis.

======================================================================
26. PARALLEL TEST INTERFERENCE
==============================

Tests that manipulate shared process state must be reviewed for interference:

* environment variables
* current working directory
* fixed TCP ports
* fixed UDP ports
* shared temporary paths
* shared files
* process-global runtime state

Use:

* unique environment names
* dynamic ports
* unique temp paths
* cleanup
* serialization only where truly required

A test that passes only because another test accidentally prepares state is
invalid.

======================================================================
27. MALFORMED METADATA TESTING
==============================

Any embedded format introduced into MINK executables must eventually be
tested for:

* missing metadata
* wrong magic
* wrong version
* truncated header
* truncated table
* invalid offset
* invalid count
* offset overflow
* size overflow
* out-of-range string data
* duplicate entries if relevant
* unknown version

The runtime must fail safely.

Embedded metadata parsing must never become a memory-safety vulnerability.

======================================================================
28. COMPILER AND LANGUAGE TESTING
=================================

For every language feature test:

* smallest legal program
* normal usage
* multiple uses
* nested use
* use in functions
* use in expressions
* interaction with types
* interaction with control flow
* malformed syntax
* missing tokens
* unexpected tokens
* invalid types
* invalid operations
* boundary literals
* deeply nested forms
* unusual whitespace
* comments
* empty blocks
* long identifiers
* generated native behavior

Parser success alone is not completion.

Type-checker success alone is not completion.

Code generation success alone is not completion.

Native behavior must agree with language semantics.

======================================================================
29. PARSER AND MALFORMED INPUT RESILIENCE
=========================================

Parser tests should include:

* random malformed token sequences
* missing delimiters
* extra delimiters
* deeply nested delimiters
* incomplete files
* invalid UTF-8 handling according to contract
* huge identifiers
* huge literals
* huge whitespace
* repeated operators
* invalid declarations

Expected outcome:

* valid program
  or
* controlled compiler diagnostic

Unexpected panic, uncontrolled recursion failure, or memory corruption is a
bug.

======================================================================
30. FUZZING
===========

Where technically practical, introduce fuzz or property testing for:

* lexer
* parser
* string functions
* JSON parser
* formatting parser
* module/path parser
* runtime metadata parser
* archive/parser code when added
* regex implementation when added
* network protocol parsers when added

Properties should include:

* no unexpected panic
* no invalid memory access
* malformed input fails safely
* valid round trips remain valid
* parser never hangs indefinitely
* bounded resource behavior where practical

Every fuzz-discovered bug must become a deterministic regression test.

======================================================================
31. STRESS TESTING
==================

Stress tests should exercise:

* thousands of allocations
* repeated free-list reuse
* large strings
* repeated string formatting
* large file IO
* many process launches
* large process output
* repeated TCP connections
* repeated HTTP operations
* repeated module compilation
* repeated CLI invocation
* repeated standalone execution

Stress tests must have reasonable upper bounds.

Do not create infinite or uncontrolled CI tests.

======================================================================
32. RESOURCE FAILURE TESTING
============================

Where practical test:

* allocation failure
* file open failure
* file read/write failure
* pipe creation failure
* process creation failure
* network initialization failure
* socket failure
* connection failure
* short read
* short write
* partial data
* OS API returning zero/null/failure
* missing environment values

All partial initialization paths must clean up already-acquired resources.

======================================================================
33. REAL APPLICATION PROOF
==========================

Tests alone do not prove usability.

Maintain real MINK applications exercising major capabilities.

Current examples include capability proof applications.

Future proof categories should include:

* command-line utility
* filesystem utility
* JSON/data processor
* process utility
* HTTP client
* networking utility
* automation program
* substantial pure-MINK application

Eventually MINK must prove itself through applications large enough to expose
architecture problems that small tests cannot reveal.

======================================================================
34. CROSS-SUBSYSTEM TESTING
===========================

Create tests and proof applications combining systems such as:

argv
-> environment
-> filesystem
-> JSON
-> string formatting
-> output

stdin
-> parser
-> formatting
-> file write

network
-> receive
-> JSON
-> string handling
-> file output

process
-> stdout/stderr capture
-> parsing
-> report generation

filesystem
-> allocation
-> string processing
-> crypto
-> output

runtime error
-> nested function
-> embedded location metadata
-> stderr
-> exit code

Test the seams between subsystems.

Many production bugs exist at subsystem boundaries rather than inside
isolated functions.

======================================================================
35. FAILURE AFTER SUCCESSFUL PARTIAL WORK
=========================================

Every complex operation should test failure after partial progress.

Examples:

* allocation succeeded, second allocation failed
* file opened, read failed
* pipe created, process creation failed
* socket created, connection failed
* output string allocated, formatting failed
* metadata found, table entry invalid

Verify all already-acquired resources are released correctly.

======================================================================
36. ERROR CODE TESTING
======================

MINK runtime error codes must be stable where documented.

For every runtime error:

* trigger intentionally
* verify expected exit code
* verify expected error identity
* verify stderr
* verify source location where applicable
* verify cleanup
* verify no secondary crash

Do not accidentally replace a MINK runtime error with a raw Windows fault.

======================================================================
37. SECURITY-STYLE ADVERSARIAL TESTING
======================================

Where applicable test inputs designed to break assumptions:

* enormous lengths
* malformed lengths
* integer wraparound
* negative-to-unsigned conversions
* unterminated external strings
* invalid offsets
* corrupted metadata
* deeply nested parser input
* huge repeated formatting placeholders
* path traversal forms
* unexpected environment values
* protocol truncation
* adversarial JSON

The goal is robustness, not only correctness under friendly inputs.

======================================================================
38. TOOLCHAIN CLEANLINESS
=========================

Before every commit ensure no accidental files such as:

* main.exe
* scratch probes
* temporary test files
* generated tarballs
* debug dumps
* copied binaries
* temporary source programs
* editor files

Session 100 required removing scratch artifacts.

This must remain a permanent final gate.

======================================================================
39. TEST ARTIFACT CLEANUP
=========================

All tests must clean:

* temporary directories
* environment variables
* child processes
* sockets
* handles
* files
* pipes
* generated executables

Cleanup must happen on both:

* success
* failure

When practical use unique temporary paths to prevent cross-test collisions.

======================================================================
40. FULL SUITE QUALITY GATES
============================

Before session completion run:

1. targeted tests
2. new regression tests
3. relevant subsystem tests
4. full test suite
5. debug build
6. release build
7. cargo fmt
8. cargo clippy
9. native PE execution
10. leak checks
11. npm pack where relevant
12. clean npm install where relevant
13. standalone executable proof where relevant
14. real example/proof application where relevant

No new clippy warnings in changed code.

No hidden failing tests.

No disabled regressions.

======================================================================
41. CHANGE-IMPACT TESTING
=========================

Before implementing a change identify what it could break.

Examples:

Compiler emitter change may affect:

* arithmetic
* functions
* strings
* runtime calls
* PE generation
* diagnostics
* determinism

Allocator change may affect:

* strings
* filesystem
* process
* networking
* JSON
* environment
* diagnostics

Driver/module change may affect:

* check
* build
* run
* npm install
* stdlib imports
* relative imports

Run tests according to actual blast radius, not only files directly changed.

======================================================================
42. RISK MATRIX FOR EVERY SESSION
=================================

Before coding classify each planned change:

LOW
Documentation or isolated low-risk logic.

MEDIUM
Compiler semantics or stdlib behavior with bounded effect.

HIGH
Emitter, allocator, ownership, parser, PE layout, OS API integration,
process, networking.

CRITICAL
ABI, concurrency, async runtime, memory model, package resolver security,
unsafe metadata parsing, cross-thread ownership.

Higher risk requires stronger evidence.

======================================================================
43. BUG REGRESSION RULE
=======================

Every meaningful bug follows:

BUG
-> ROOT CAUSE
-> MINIMAL REPRODUCTION
-> FIX
-> DURABLE REGRESSION TEST
-> NATIVE EXECUTION IF APPLICABLE
-> FULL REGRESSION
-> DOCUMENTATION

Examples already protected include:

* Windows process output deadlock
* env free-list reuse regression
* network operations without net_init
* home-dir stale flags
* home-dir dead copy loops
* runtime source-location behavior

Never fix and forget.

======================================================================
44. PRODUCTION VERIFICATION MATRIX
==================================

Maintain a durable matrix for every major capability containing:

* source implemented
* compiler wired
* codegen wired
* unit tested
* integration tested
* native Windows tested
* empty input tested
* boundary tested
* malformed tested
* failure tested
* ownership tested
* leak tested
* stress tested
* deterministic tested
* packaged tested
* clean install tested
* standalone tested
* real application tested
* known limitations
* severity of remaining gaps
* final status

Possible final statuses:

MISSING
PARTIAL
IMPLEMENTED
EXECUTION VERIFIED
PRODUCTION VERIFIED

Do not compress all of these into a misleading boolean.

======================================================================
45. DEFINITION OF PRODUCTION VERIFIED
=====================================

A capability can be marked PRODUCTION VERIFIED only after all applicable
evidence exists:

SOURCE
-> COMPILER
-> CODEGEN
-> NATIVE EXECUTION
-> HAPPY PATH
-> EDGE CASE
-> INVALID INPUT
-> FAILURE PATH
-> OWNERSHIP
-> LEAK CHECK
-> STRESS
-> DETERMINISM
-> REGRESSION
-> CLEAN ENVIRONMENT
-> DISTRIBUTION
-> STANDALONE
-> REAL APPLICATION

Not every capability requires every category.

Every omitted category must have a documented reason.

======================================================================
46. CLAIM DISCIPLINE
====================

Never claim:

"supports Unicode"

from one non-ASCII test if core Windows APIs remain ANSI.

Never claim:

"fully compatible"

from partial feature coverage.

Never claim:

"deterministic"

without controlling all deterministic inputs.

Never claim:

"standalone"

if the executable depends on repository files.

Never claim:

"production ready"

from unit tests alone.

Never claim:

"bug fixed"

without reproducing and regression-testing the bug whenever practical.

======================================================================
47. WINDOWS-SPECIFIC RULE
=========================

Windows is the current active parity platform.

Every current feature must prioritize real Windows semantics.

Test:

* Windows environment
* Windows paths
* Windows process APIs
* Windows networking
* Windows console behavior
* Windows executable behavior
* Windows exit codes
* Windows packaging
* Windows clean installations

Linux remains FROZEN.

Shared abstractions may compile for Linux only when necessary.

Do not implement new Linux behavior.

Do not quietly expand Linux scope.

======================================================================
48. FUTURE WIDE-API MIGRATION RULE
==================================

Current ANSI-only Windows APIs are known parity limitations in some areas.

When migrating to Windows wide APIs later test:

* ASCII
* Latin extended characters
* CJK
* Devanagari
* emoji where Windows filesystem/console permits
* spaces
* long paths
* conversion failures
* invalid UTF sequences according to MINK contract

Do not mark Windows Unicode parity complete before such proof exists.

======================================================================
49. FUTURE MAP/SET/VEC STANDARD
===============================

When Wave B implements Map, Set, or full typed Vec, tests must include:

* empty collection
* one element
* many elements
* duplicate values
* insert
* lookup
* missing lookup
* remove
* repeated remove
* growth
* capacity boundaries
* iteration
* nested collections
* ownership
* contained strings
* contained aggregates
* function interactions
* mutation during valid lifecycle
* large collection stress
* malformed compiler use
* type errors
* memory cleanup

For Map specifically test:

* duplicate key replacement semantics
* collision behavior
* hash consistency
* missing keys
* large key sets

For Set:

* duplicate insertion
* membership
* removal
* large sets

For Vec:

* growth
* indexing
* out-of-bounds
* mutation
* nested Vec
* element ownership
* resize/reallocation behavior

======================================================================
50. FUTURE CONCURRENCY STANDARD
===============================

When threads/concurrency are eventually implemented, testing requirements
increase substantially.

Must include:

* race detection strategy
* shared allocator behavior
* ownership across threads
* repeated thread creation
* thread join
* failure to create thread
* synchronization
* deadlock scenarios
* starvation scenarios
* concurrent strings
* concurrent collections
* concurrent filesystem
* concurrent process/network where supported

Concurrency must not be marked production ready from happy-path tests.

======================================================================
51. FUTURE ASYNC STANDARD
=========================

When async is implemented test:

* completed immediately
* pending
* cancellation
* nested async
* failure
* multiple tasks
* large task counts
* wakeups
* ordering semantics
* resource cleanup
* cancellation cleanup
* timeout behavior
* interaction with IO

Async architecture must be audited for allocator and ownership safety before
shipping.

======================================================================
52. FUTURE PACKAGE MANAGER STANDARD
===================================

When package management begins test:

* empty project
* one dependency
* many dependencies
* transitive dependency
* duplicate dependency
* version conflict
* missing package
* malformed manifest
* malformed lockfile
* offline behavior
* corrupted package
* interrupted install
* deterministic lockfile
* cache behavior
* path dependencies
* security validation
* dependency cycles
* clean reproduction

Package manager security and determinism are release-critical.

======================================================================
53. FUTURE FFI STANDARD
=======================

When FFI begins test:

* C call into MINK
* MINK call into C
* integers
* floats
* pointers
* strings
* arrays
* structs
* null pointers
* ownership transfer
* caller-owned memory
* callee-owned memory
* callbacks
* error handling
* ABI version mismatch
* repeated calls
* dynamic library load/unload
* Windows DLL behavior

ABI claims require binary-level interoperability evidence.

======================================================================
54. REAL USER WORKFLOW PRINCIPLE
================================

Always ask:

"Would this work for somebody who only installed MINK and does not have the
MINK repository?"

For shipped functionality, that user's workflow is authoritative.

Expected chain:

npm install
-> mink
-> check
-> build
-> run
-> standalone executable

Developer checkout success is secondary evidence.

======================================================================
55. FULL RELEASE AUDIT REQUIREMENT
==================================

Before declaring Windows Python official capability parity complete, perform
a dedicated final audit that does not trust previous session claims.

Re-test from scratch:

* language
* compiler
* runtime
* stdlib
* CLI
* packaging
* Windows integration
* standalone execution
* real applications
* memory
* diagnostics
* malformed input
* stress
* deterministic behavior

Cross-check:

documentation
versus
source
versus
tests
versus
native execution
versus
shipped npm package

Any mismatch must be corrected before completion.

======================================================================
56. TEST COUNTS ARE NOT THE GOAL
================================

A higher test count does not prove higher quality.

2574 passing tests are useful evidence, but not proof of every possible
situation.

Prefer:

10 carefully designed adversarial tests

over:

100 trivial happy-path tests

when the former provide more meaningful coverage.

Measure coverage by risk and behavior, not number alone.

======================================================================
57. SESSION START PROCEDURE
===========================

At the beginning of every future session:

1. confirm starting commit
2. confirm clean tree
3. read previous session report
4. identify exact capabilities being changed
5. create a risk matrix
6. identify applicable verification dimensions
7. identify subsystem blast radius
8. identify existing regression tests that must remain green
9. identify native execution proof required
10. identify packaging/standalone proof required
11. then implement

======================================================================
58. SESSION COMPLETION PROCEDURE
================================

Before completing every session:

1. implementation complete
2. targeted tests pass
3. boundary tests pass
4. invalid-input tests pass
5. failure-path tests pass
6. ownership audit complete
7. leak tests pass
8. stress tests pass where applicable
9. native Windows execution verified
10. subsystem interaction tests pass
11. deterministic behavior verified
12. full suite passes
13. fmt clean
14. clippy clean for changed code
15. debug build clean
16. release build clean
17. npm verification where applicable
18. standalone verification where applicable
19. docs updated only after evidence
20. parity matrix updated only after evidence
21. temporary files removed
22. git diff audited
23. commit created
24. final git tree clean

======================================================================
59. REQUIRED FINAL REPORT ADDITION
==================================

Every future MINK session report must include:

COMPREHENSIVE VERIFICATION REPORT

1. Capabilities changed:
2. Risk classification:
3. Happy-path tests:
4. Boundary tests:
5. Empty/zero tests:
6. Invalid-input tests:
7. Malformed-input tests:
8. Failure-path tests:
9. Ownership tests:
10. Leak tests:
11. Stress tests:
12. Repetition tests:
13. Determinism tests:
14. Native Windows execution:
15. Cross-subsystem tests:
16. Clean npm installation:
17. Standalone executable:
18. Real application proof:
19. Bugs discovered only through native execution:
20. New permanent regression tests:
21. Known untested situations:
22. Known limitations:
23. P0:
24. P1:
25. P2:
26. P3:
27. Test count:
28. Quality gates:
29. Final verification status:

No field may be omitted silently.

Use N/A with justification when genuinely inapplicable.

======================================================================
60. UNKNOWN BEHAVIOR RULE
=========================

Unknown behavior is not success.

If a situation has not been tested:

mark it:

UNVERIFIED

or:

NEEDS EXECUTION PROOF

Do not infer correctness from nearby tests.

Do not convert "probably works" into "VERIFIED".

======================================================================
61. NO FALSE COMPLETION
=======================

Never:

* hide a failure
* delete a failing regression test
* weaken an assertion to pass
* classify a product bug as infrastructure without evidence
* disable a difficult test
* skip native execution
* depend on dev checkout for shipped behavior
* invent source locations
* fabricate compatibility claims
* mark partially implemented behavior VERIFIED
* ignore ownership because output looks correct
* ignore a leak because the process exits
* ignore intermittent hangs
* ignore nondeterminism without root-cause analysis
* use Linux implementation to distract from Windows parity scope

======================================================================
62. FINAL RULE
==============

For every feature ask:

WHAT HAPPENS IF EVERYTHING GOES RIGHT?

Then ask:

WHAT HAPPENS IF THE INPUT IS EMPTY?

WHAT HAPPENS AT THE BOUNDARY?

WHAT HAPPENS IF THE INPUT IS WRONG?

WHAT HAPPENS IF THE OS CALL FAILS?

WHAT HAPPENS IF MEMORY IS LOW?

WHAT HAPPENS IF THIS RUNS 1000 TIMES?

WHAT HAPPENS IF THE RESULT IS HUGE?

WHAT HAPPENS IF THE PATH HAS SPACES?

WHAT HAPPENS IF THE PATH IS NON-ASCII?

WHAT HAPPENS IF THE USER RUNS IT OUTSIDE THE REPOSITORY?

WHAT HAPPENS FROM A CLEAN NPM INSTALL?

WHAT HAPPENS AFTER COPYING THE EXE ELSEWHERE?

WHAT HAPPENS AFTER PARTIAL SUCCESS?

WHAT GETS FREED?

WHAT CAN LEAK?

WHAT CAN HANG?

WHAT CAN OVERFLOW?

WHAT CAN BECOME NONDETERMINISTIC?

WHAT OTHER SUBSYSTEM CAN THIS BREAK?

WHAT ASSUMPTION HAVE WE NOT TESTED?

Then prove the answers with tests and native execution.

======================================================================
63. MINK DEVELOPMENT PRIORITY
=============================

When choosing between:

A. adding another large feature

and

B. closing a meaningful verification gap in an existing feature

prefer B when the existing feature still contains realistic untested risk.

The goal is not to make MINK look complete.

The goal is to make MINK actually reliable.

======================================================================
64. BINDING STATUS
==================

This document is mandatory for:

* Session 101
* every later Windows Python parity session
* final Windows parity audit
* future Linux work after Linux is unfrozen
* package ecosystem work
* FFI work
* concurrency work
* async work
* tooling work
* production release audits

Future session prompts must explicitly reference and enforce this standard.

No capability may bypass it merely because implementation appears simple.

END OF MINK COMPREHENSIVE VERIFICATION STANDARD