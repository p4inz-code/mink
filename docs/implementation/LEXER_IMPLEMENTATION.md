# MINK — Lexer Implementation

**Status:** Implementation
**Version:** 0.1.0
**Session:** 02 — Lexer + Token System

## 1. Objective

The lexer converts MINK source text into a deterministic stream of tokens
with accurate source spans, reporting lexical problems as structured errors
instead of panicking. It is the first stage of the compiler pipeline
(`docs/compiler/COMPILER_ARCHITECTURE.md` §2):

    Source → Lexer → Parser → ...

At the end of this session the following works end to end:

    MINK source → SourceFile → Lexer → Token stream → spans + lexical diagnostics

The parser, semantic analysis, and type system are intentionally out of scope.

## 2. Token Architecture

Tokens are defined in `src/lexer/token.rs`.

### `Token`

```rust
pub struct Token {
    kind: TokenKind,
    span: Span,
}
```

- `Token` is `Copy`, `Eq`, `Hash`, and carries **no source text**. The exact
  text a token covers is recovered from the owning `SourceFile` through its
  `Span` (`SourceFile::span_text`). This keeps tokens small, allocation-free,
  and cheap to buffer — properties a future parser needs for arbitrary
  lookahead, and tooling (formatting, highlighting, LSP) needs for fidelity.
- `TokenKind` is a plain unit enum; keyword recognition happens at lex time,
  so the parser never compares strings to find keywords.

### `TokenKind`

The kind enum covers every lexical category the current specification needs:

| Category            | Kinds                                                                 |
| ------------------- | --------------------------------------------------------------------- |
| Identifiers         | `Ident`                                                               |
| Integer literals    | `Int`                                                                 |
| Float literals      | `Float`                                                               |
| String literals     | `Str`                                                                 |
| Char literals       | `Char`                                                                |
| Boolean literals    | `True`, `False`                                                       |
| Null literal        | `Null`                                                                |
| Keywords            | `Fn Let Const Type Struct Enum Trait Impl Mod Use Pub If Else Match Loop While For In Return Break Continue Async Await Unsafe` |
| Delimiters          | `( ) { } [ ]`                                                         |
| Punctuation         | `, ; : . :: -> =>`                                                    |
| Arithmetic ops      | `+ - * / %` and `+= -= *= /= %=`                                      |
| Comparison/equality | `== != < <= > >=`                                                     |
| Logical ops         | `&& || !`                                                             |
| Bitwise ops         | `& | ^ ~ << >>`                                                       |
| Assignment          | `=`                                                                    |
| Range ops           | `.. ..=`                                                              |
| Optional op         | `?`                                                                    |
| End of input        | `Eof`                                                                 |

Helpers `TokenKind::is_keyword()` and `TokenKind::is_literal()` classify
kinds for parser and tooling use.

### Keywords

Keywords live in a sorted static table in `src/lexer/keywords.rs` and are
looked up by binary search over the raw identifier text — no allocation, no
interning required at this stage. Matching is exact and case-sensitive:

- `fn` is the `Fn` keyword; `Fn`, `fn_`, `fnn` are identifiers.
- `true`, `false`, `null` are reserved words but classified as literals.

The keyword set is **provisional**: the core language specification
(`docs/language/CORE_LANGUAGE.md` §5) deliberately keeps the reserved set
small and defers freezing it until the grammar is designed. Every keyword
currently reserved names a construct the specification describes. Changing
the set is a one-place edit in `keywords.rs`.

## 3. Lexer Architecture

The lexer lives in `src/lexer/scanner.rs` and scans the source in a single
forward pass with no backtracking, using byte offsets into the UTF-8 text.

### APIs

Two usage styles are provided:

- **One-shot**: `mink::lexer::lex(&SourceFile) -> Lexed` returns every token
  (including the final `Eof`) plus every lexical error. Used by the driver.
- **Pull-based**: `Lexer::new(&SourceFile)` + `Lexer::next_token() ->
  Option<Token>` returns tokens one at a time, ending with the `Eof` token
  and then `None`. Errors accumulate in `Lexer::errors()`. This is the shape
  a future parser consumes, and because tokens are `Copy`, lookahead is a
  simple buffered window.

`Lexed` exposes `tokens()`, `errors()`, `is_valid()`, `into_parts()`, and
`into_errors()`.

### Scan loop

For each token the lexer:

1. skips trivia — whitespace (Unicode-aware), `//` line comments, and
   `/* ... */` block comments (non-nesting);
2. dispatches on the first character: identifier/keyword, number, string,
   char, operator/punctuation, or unexpected character;
3. returns a `Token` covering the exact consumed byte range.

Trivia never becomes a token. A block comment that reaches end of input
without `*/` is an `UnterminatedBlockComment` error.

## 4. Lexical Decisions

The specification does not yet freeze exact lexical forms; the following
decisions are the simplest technically sound representations, chosen
conventionally and **subject to revision when the grammar freezes**.

### Identifiers

- ASCII only: `[A-Za-z_][A-Za-z0-9_]*`.
- Unicode identifiers are **deferred**. The spec says identifiers "should
  support Unicode where technically safe" but does not define a policy; per
  the session rules we do not silently introduce a complex Unicode policy.
  Non-ASCII letters in code produce `UnexpectedCharacter` errors and are
  documented as a known limitation (see §8). Unicode is fully supported
  inside strings, chars, and comments, and byte offsets are preserved.

### Numbers

- Decimal integers: `0`, `42`, `007` (leading zeros accepted lexically).
- Radix integers: `0x1F`, `0o17`, `0b1010` (case-insensitive prefix).
- Separators: single `_` between digits (`1_000`, `0xDEAD_BEEF`). Malformed
  separators (`1__2`, `1_`) are `MalformedNumber` errors.
- Floats: decimal only — `1.5`, `0.5`, `1.5e3`, `1E5`, `1e-2`, `1.5e+3`.
  The fraction requires a digit after `.`; a `.` followed by a non-digit is
  a member-access dot or range operator (`1.` → `Int` + `Dot`, `1..5` →
  `Int` `..` `Int`). A leading `.` is never a number (`.5` → `Dot` `Int`).
- A number immediately followed by an identifier character is malformed:
  `123abc`, `0b12`, `0x1G` all produce `MalformedNumber` errors.
- Lexing validates **syntax only**; numeric interpretation is deferred to a
  later compiler stage.

### Strings and characters

- Strings are double-quoted, single-line literals. Unescaped newlines and
  end-of-input before the closing quote are `UnterminatedString` errors.
- Chars are single-quoted and must contain exactly one character or escape:
  `''` and `'ab'` are `InvalidCharLiteral` errors; `'a` is unterminated.
- Escapes: `\n \r \t \0 \\ \" \'`, `\xHH` (two hex digits), and
  `\u{...}` (one to six hex digits, valid scalar value ≤ 0x10FFFF, no
  surrogates). Unknown escapes (`\q`), malformed `\x`/`\u` forms, and
  out-of-range scalar values are errors.
- Escape **decoding is deferred**: the token preserves the raw source span;
  the AST stage will interpret literal values.

### Operators and punctuation

The set in §2 is the conventional minimal set covering the categories the
specification names (arithmetic, comparison, equality, boolean logic,
assignment, bitwise, range construction, optional handling). No speculative
operators were added. `->` (return type), `=>` (match arm), and `::` (path
separator) are provisional conventional choices.

### Comments

- `//` line comments and `/* ... */` block comments; block comments do not
  nest. Unterminated block comments are errors.
- Documentation comments (`///`, `//!`) are lexically ordinary line
  comments; surfacing them as structured metadata is deferred to tooling
  work (they are intentionally not tokens).

## 5. Span Handling

- Every token uses the shared `Span` type — no second span system.
- Spans are half-open byte ranges `[start, end)` into one source file.
- The `Eof` token carries an empty span at the end of the source text.
- Byte offsets are preserved across Unicode, multiline source, comments, and
  malformed input; spans never split a UTF-8 character (asserted in tests).

## 6. Error Recovery

Errors are modeled in `src/lexer/error.rs` (`LexErrorKind` + `LexError`).
The lexer **never panics** and never blocks on the first problem:

| Error                    | Code   | Recovery                                 |
| ------------------------ | ------ | ---------------------------------------- |
| `UnexpectedCharacter`    | E-L01  | consume the char, continue               |
| `UnterminatedString`     | E-L02  | partial `Str` token, continue            |
| `UnterminatedChar`       | E-L03  | partial `Char` token, continue           |
| `UnterminatedBlockComment` | E-L04 | consume to end of input                 |
| `InvalidEscape`          | E-L05  | skip the escape, finish the literal      |
| `InvalidUnicodeEscape`   | E-L06  | skip the escape, finish the literal      |
| `InvalidCharLiteral`     | E-L07  | partial `Char` token, continue           |
| `MalformedNumber`        | E-L08  | partial `Int`/`Float` token, continue    |

Malformed literals still emit a token covering the consumed text so the
stream stays dense; unexpected characters emit no token at all. Multiple
errors in one file are all reported in a single run.

Error codes are provisional until the full diagnostic engine defines the
final namespace.

## 7. CLI Integration

`mink check <path>` loads a `.mink` file and runs lexical analysis:

- valid input → prints `passed lexical analysis`, exit code 0;
- invalid input → prints each diagnostic (code, message, `--> path:line:col`)
  to stderr, exit code 1;
- unreadable file → I/O error, exit code 1;
- never panics.

Diagnostics are currently rendered with a minimal ad-hoc formatter in
`src/cli.rs`; the structured diagnostic engine (per
`docs/language/ERROR_SYSTEM.md`) will replace it. `mink build` remains
`NotImplemented` — compilation is not part of this session.

## 8. Deferred / Not Implemented

Intentionally deferred, with rationale:

- **Unicode identifiers** — no spec-defined policy yet; ASCII-only with
  `UnexpectedCharacter` errors for non-ASCII identifier characters.
- **Byte literals** (`b"..."`) — spec lists byte sequences as a literal
  category but no syntax; deferred until the grammar freezes.
- **Raw strings** — not specified; deferred.
- **Escape/literal decoding** — raw text preserved via spans; interpretation
  belongs to the AST stage.
- **Numeric base prefixes for floats** (e.g. hex floats) — not specified.
- **Nested block comments** — spec does not require nesting; simplest
  non-nesting form chosen.
- **Doc-comment metadata** — `///`/`//!` lex as comments; structured
  extraction deferred to tooling.
- **Full fuzzing harness** — the project is dependency-free; a full
  `cargo-fuzz` target would add a framework this milestone does not need.
  Instead a deterministic pseudo-random malformed-input corpus plus a
  targeted malformed corpus are part of the test suite, asserting the
  invariant that arbitrary input never panics and spans stay in bounds.
  Adding a `cargo-fuzz` target is the next testing enhancement.
- **UTF-8 BOM handling** — a leading `U+FEFF` is not treated as whitespace
  (it is not in Unicode's `White_Space` property), so a BOM-prefixed file
  reports an `UnexpectedCharacter` error at offset 0. Skipping a leading BOM
  is a possible future convenience; the current behavior is a deliberate,
  explicit failure rather than silent acceptance.

## 9. Performance Notes

- Single forward pass, linear in source size.
- No regex, no per-character allocation, no full-string copying: tokens are
  `Copy` values referencing source via spans, and keyword lookup is a binary
  search over a static table.
- Error collection allocates only when errors actually occur.

## 10. Quality Gates

Development commands (see `docs/implementation/ENGINEERING_FOUNDATION.md`):

    cargo fmt --check
    cargo clippy --all-targets -- -D warnings
    cargo test
    cargo build
    git diff --check
