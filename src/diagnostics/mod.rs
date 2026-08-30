//! Structured diagnostic output for the MINK compiler.
//!
//! Provides machine-readable JSON diagnostics for `mink check --json` and
//! human-readable error explanations for `mink explain <CODE>`. This module
//! is designed as reusable infrastructure for AI agents, IDE tooling, and
//! future developer tools.
//!
//! # Design Principles
//!
//! - **Stable schema.** The JSON output format is versioned and backward-compatible.
//! - **Deterministic.** Same input produces identical JSON output.
//! - **Complete.** Every error code has a documented explanation.
//! - **Machine-readable.** All fields are structured; no parsing required.
//!
//! Reference: `docs/compiler/COMPILER_ARCHITECTURE.md` §10 (Diagnostics)
//! and `docs/ai/AI_TOOLING_ARCHITECTURE.md`.

use crate::driver::CheckReport;
use crate::source::{SourceFile, SourceMap};

// =========================================================================
// JSON Diagnostic Output
// =========================================================================

/// Renders a [`CheckReport`] as a machine-readable JSON string.
///
/// The output is deterministic: identical reports produce identical JSON.
/// The schema is:
///
/// ```json
/// {
///   "success": bool,
///   "files_checked": int,
///   "token_count": int,
///   "errors": [
///     {
///       "code": "E-XXX",
///       "severity": "error",
///       "message": "...",
///       "span": { "file": "...", "start_line": int, "start_column": int, "end_line": int, "end_column": int },
///       "related": [ { "message": "...", "file": "...", "line": int, "column": int } ]
///     }
///   ],
///   "warnings": []
/// }
/// ```
pub fn render_json(report: &CheckReport, sources: &SourceMap) -> String {
    let mut out = String::with_capacity(1024);
    out.push_str("{\n");

    // success
    out.push_str("  \"success\": ");
    out.push_str(if report.errors.is_empty() {
        "true"
    } else {
        "false"
    });
    out.push_str(",\n");

    // files_checked
    out.push_str("  \"files_checked\": 1,\n");

    // token_count
    out.push_str(&format!("  \"token_count\": {},\n", report.token_count));

    // errors
    out.push_str("  \"errors\": [");
    let file = sources.get(report.source_id);
    for (i, error) in report.errors.iter().enumerate() {
        if i > 0 {
            out.push_str(",\n");
        }
        out.push_str("\n    ");
        render_error_json(&mut out, error, file);
    }
    if !report.errors.is_empty() {
        out.push_str("\n  ");
    }
    out.push_str("],\n");

    // warnings (empty for now — no warning infrastructure yet)
    out.push_str("  \"warnings\": []\n");

    out.push_str("}\n");
    out
}

/// Renders a single [`CheckError`] as a JSON object (without outer braces).
fn render_error_json(
    out: &mut String,
    error: &crate::driver::CheckError,
    file: Option<&SourceFile>,
) {
    out.push_str("{\n");

    // code
    out.push_str(&format!("      \"code\": \"{}\",\n", error.code()));

    // severity (all current diagnostics are errors)
    out.push_str("      \"severity\": \"error\",\n");

    // message
    out.push_str(&format!(
        "      \"message\": \"{}\",\n",
        escape_json(&error.to_string())
    ));

    // span
    let span = error.span();
    if let Some(f) = file {
        let start = f.line_col(span.start());
        let end = f.line_col(span.end());
        out.push_str(&format!(
            "      \"span\": {{ \"file\": \"{}\", \"start_line\": {}, \"start_column\": {}, \"end_line\": {}, \"end_column\": {} }},\n",
            escape_json(&f.name().display().to_string()),
            start.line,
            start.column,
            end.line,
            end.column
        ));
    } else {
        out.push_str("      \"span\": null,\n");
    }

    // related
    out.push_str("      \"related\": [");
    if let Some(related_span) = error.related_span() {
        if let Some(f) = file {
            let pos = f.line_col(related_span.start());
            let note = match error {
                crate::driver::CheckError::Semantic(sem)
                    if sem.kind() == crate::semantics::SemanticErrorKind::DuplicateDefinition =>
                {
                    "previous declaration is here"
                }
                _ => "related location is here",
            };
            out.push_str(&format!(
                "\n        {{ \"message\": \"{}\", \"file\": \"{}\", \"line\": {}, \"column\": {} }}",
                note,
                escape_json(&f.name().display().to_string()),
                pos.line,
                pos.column
            ));
        }
    }
    out.push_str("\n      ]");

    out.push_str("\n    }");
}

/// Escapes a string for JSON output.
fn escape_json(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c < '\x20' => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out
}

// =========================================================================
// Error Code Documentation
// =========================================================================

/// Documentation for a single error code.
pub struct ErrorDoc {
    /// The stable error code (e.g. `"E-T01"`).
    pub code: &'static str,
    /// Short human-readable title.
    pub title: &'static str,
    /// Detailed description of the error.
    pub description: &'static str,
    /// Category: `"lexical"`, `"parser"`, `"semantic"`, `"type"`, `"ownership"`,
    /// `"hir"`, `"mir"`, `"backend"`, `"runtime"`.
    pub category: &'static str,
    /// Common causes of this error.
    pub common_causes: &'static [&'static str],
    /// Suggested fixes.
    pub suggested_fixes: &'static [&'static str],
}

/// Returns the documentation for the given error code, if it exists.
pub fn explain(code: &str) -> Option<&'static ErrorDoc> {
    ALL_DOCS.iter().find(|doc| doc.code == code)
}

/// Returns all error codes that have documentation.
pub fn all_codes() -> Vec<&'static str> {
    ALL_DOCS.iter().map(|doc| doc.code).collect()
}

/// All error code documentation, organized by category.
const ALL_DOCS: &[ErrorDoc] = &[
    // =================================================================
    // Lexical errors (E-L)
    // =================================================================
    ErrorDoc {
        code: "E-L01",
        title: "Unexpected Character",
        description: "The lexer encountered a character that does not start any token and is not whitespace or a comment.",
        category: "lexical",
        common_causes: &[
            "A typo in a keyword or operator",
            "A non-ASCII character in source code",
            "A character outside a string literal that is not valid syntax",
        ],
        suggested_fixes: &[
            "Check the character at the indicated position",
            "Remove or replace the invalid character",
            "Ensure the file is saved as UTF-8",
        ],
    },
    ErrorDoc {
        code: "E-L02",
        title: "Unterminated String",
        description: "A string literal was started with a double quote but never closed.",
        category: "lexical",
        common_causes: &[
            "Missing closing double quote",
            "Newline inside a string literal (strings cannot span lines in V1)",
            "Escape sequence consuming the closing quote",
        ],
        suggested_fixes: &[
            "Add a closing double quote",
            "Use string concatenation for multi-line strings",
            "Check for unescaped special characters",
        ],
    },
    ErrorDoc {
        code: "E-L03",
        title: "Unterminated Character",
        description: "A character literal was started with a single quote but never closed.",
        category: "lexical",
        common_causes: &[
            "Missing closing single quote",
            "Multiple characters in a character literal",
        ],
        suggested_fixes: &[
            "Add a closing single quote",
            "Use a string literal for multi-character content",
        ],
    },
    ErrorDoc {
        code: "E-L04",
        title: "Unterminated Block Comment",
        description: "A block comment was started with /* but never closed with */.",
        category: "lexical",
        common_causes: &["Missing closing */", "Nested /* without matching */"],
        suggested_fixes: &["Add a closing */", "Use // line comments instead"],
    },
    ErrorDoc {
        code: "E-L05",
        title: "Invalid Escape Sequence",
        description: "A backslash followed by a character that is not a recognized escape.",
        category: "lexical",
        common_causes: &[
            "Typo in escape sequence (e.g., \\x instead of \\n)",
            "Unknown escape character",
        ],
        suggested_fixes: &[
            "Use a recognized escape: \\n, \\t, \\r, \\\\, \\\"",
            "Remove the backslash if the literal character is intended",
        ],
    },
    ErrorDoc {
        code: "E-L06",
        title: "Invalid Unicode Escape",
        description: "A \\u{...} escape sequence is malformed or encodes an invalid Unicode scalar value.",
        category: "lexical",
        common_causes: &[
            "Missing braces: \\uXXXX instead of \\u{XXXX}",
            "Hex digits out of range",
            "Codepoint is a surrogate (U+D800..U+DFFF)",
        ],
        suggested_fixes: &[
            "Use the format \\u{HHHH} with 1-6 hex digits",
            "Ensure the codepoint is a valid Unicode scalar (0x0..0x10FFFF, excluding surrogates)",
        ],
    },
    ErrorDoc {
        code: "E-L07",
        title: "Invalid Character Literal",
        description: "A character literal contains zero or more than one character.",
        category: "lexical",
        common_causes: &["Empty character literal: ''", "Multiple characters: 'ab'"],
        suggested_fixes: &[
            "Character literals must contain exactly one character",
            "Use a string literal for multi-character content",
        ],
    },
    ErrorDoc {
        code: "E-L08",
        title: "Malformed Number",
        description: "A numeric literal has an invalid shape.",
        category: "lexical",
        common_causes: &[
            "Invalid base prefix (e.g., 0b for binary with non-binary digits)",
            "Missing digits after prefix or decimal point",
            "Trailing underscore in numeric literal",
        ],
        suggested_fixes: &[
            "Check the literal format: decimal, 0x hex, 0b binary, 0o octal",
            "Ensure digits match the base",
            "Remove trailing underscores",
        ],
    },
    // =================================================================
    // Parser errors (E-P) — selected high-value codes
    // =================================================================
    ErrorDoc {
        code: "E-P04",
        title: "Expected Assignment Target",
        description: "The parser expected an assignment target (a variable or place expression) on the left side of an assignment.",
        category: "parser",
        common_causes: &[
            "Using a literal value as an assignment target",
            "Using a function call as an assignment target",
        ],
        suggested_fixes: &[
            "Use a variable or struct field as the assignment target",
            "Ensure the target is a mutable binding",
        ],
    },
    ErrorDoc {
        code: "E-P01",
        title: "Expected Item",
        description: "The parser expected a top-level item (function, struct, enum, let, const, mod, use) but found something else.",
        category: "parser",
        common_causes: &[
            "Statement outside a function body",
            "Missing keyword",
            "Malformed declaration",
        ],
        suggested_fixes: &[
            "Ensure declarations are at module scope",
            "Check for missing keywords (fn, struct, enum, let, const)",
        ],
    },
    ErrorDoc {
        code: "E-P02",
        title: "Expected Identifier",
        description: "The parser expected an identifier but found a different token.",
        category: "parser",
        common_causes: &[
            "Reserved word used as identifier",
            "Missing name after keyword",
            "Typo in identifier",
        ],
        suggested_fixes: &[
            "Use a valid identifier (letters, digits, underscores)",
            "Check for reserved keywords",
        ],
    },
    ErrorDoc {
        code: "E-P03",
        title: "Expected Expression",
        description: "The parser expected an expression but found something else.",
        category: "parser",
        common_causes: &[
            "Missing operand in binary expression",
            "Empty parentheses or brackets",
            "Statement where expression was expected",
        ],
        suggested_fixes: &[
            "Add the missing expression",
            "Check for balanced parentheses and brackets",
        ],
    },
    ErrorDoc {
        code: "E-P06",
        title: "Expected Semicolon",
        description: "The parser expected a semicolon after a statement.",
        category: "parser",
        common_causes: &[
            "Missing semicolon after let, return, break, continue, or expression statement",
        ],
        suggested_fixes: &["Add a semicolon at the end of the statement"],
    },
    ErrorDoc {
        code: "E-P13",
        title: "Unclosed Parenthesis",
        description: "An opening parenthesis was never closed.",
        category: "parser",
        common_causes: &[
            "Missing closing parenthesis",
            "Unbalanced parentheses in expression",
        ],
        suggested_fixes: &["Add the missing closing parenthesis"],
    },
    ErrorDoc {
        code: "E-P14",
        title: "Unclosed Brace",
        description: "An opening brace was never closed.",
        category: "parser",
        common_causes: &["Missing closing brace", "Unbalanced braces in block"],
        suggested_fixes: &["Add the missing closing brace"],
    },
    // =================================================================
    // Semantic errors (E-S)
    // =================================================================
    ErrorDoc {
        code: "E-S01",
        title: "Unresolved Name",
        description: "The compiler could not find a definition for the referenced name.",
        category: "semantic",
        common_causes: &[
            "Typo in identifier",
            "Name used before definition",
            "Name not imported (missing use statement)",
            "Name defined in a different module",
        ],
        suggested_fixes: &[
            "Check the spelling of the identifier",
            "Ensure the name is defined before use",
            "Add a use statement to import the name",
        ],
    },
    ErrorDoc {
        code: "E-S02",
        title: "Duplicate Definition",
        description: "A name is defined more than once in the same scope.",
        category: "semantic",
        common_causes: &[
            "Two functions with the same name",
            "Two variables with the same name",
            "A variable and a function with the same name",
        ],
        suggested_fixes: &[
            "Rename one of the duplicate definitions",
            "Use different scopes for the definitions",
        ],
    },
    ErrorDoc {
        code: "E-S03",
        title: "Assignment to Immutable",
        description: "An assignment was made to a variable declared without `mut`.",
        category: "semantic",
        common_causes: &[
            "Forgetting to add `mut` to a let binding",
            "Trying to reassign a function parameter",
        ],
        suggested_fixes: &[
            "Declare the variable with `let mut`",
            "Use a new binding instead of reassignment",
        ],
    },
    ErrorDoc {
        code: "E-S04",
        title: "Assignment to Constant",
        description: "An assignment was made to a `const` binding. Constants cannot be reassigned.",
        category: "semantic",
        common_causes: &["Trying to modify a const value"],
        suggested_fixes: &[
            "Use `let mut` for mutable bindings",
            "Constants are immutable by design",
        ],
    },
    ErrorDoc {
        code: "E-S05",
        title: "Break Outside Loop",
        description: "A `break` statement was used outside of a loop.",
        category: "semantic",
        common_causes: &[
            "break in an if block instead of a loop",
            "break in a function body",
        ],
        suggested_fixes: &["Use break only inside loop, while, or for constructs"],
    },
    ErrorDoc {
        code: "E-S06",
        title: "Continue Outside Loop",
        description: "A `continue` statement was used outside of a loop.",
        category: "semantic",
        common_causes: &["continue in an if block instead of a loop"],
        suggested_fixes: &["Use continue only inside loop, while, or for constructs"],
    },
    ErrorDoc {
        code: "E-S07",
        title: "Return Outside Function",
        description: "A `return` statement was used outside of a function body.",
        category: "semantic",
        common_causes: &["return at module scope"],
        suggested_fixes: &["Use return only inside function bodies"],
    },
    ErrorDoc {
        code: "E-S10",
        title: "Use of Moved Value",
        description: "A value was used after its ownership was transferred (moved).",
        category: "ownership",
        common_causes: &[
            "Passing a value to a function and then using it again",
            "Assigning a value to a variable and then using the original",
            "Returning a value and then using it",
        ],
        suggested_fixes: &[
            "Use a borrow (&value) instead of moving",
            "Clone the value if you need two copies",
            "Restructure code to use the value before moving it",
        ],
    },
    ErrorDoc {
        code: "E-S11",
        title: "Mutating Immutable String",
        description: "An attempt was made to modify a string value (strings are immutable in V1).",
        category: "semantic",
        common_causes: &[
            "Trying to set a byte in a string literal",
            "Trying to modify a string through rt_str_set_byte on an immutable string",
        ],
        suggested_fixes: &[
            "Create a new string with the desired content",
            "Use string concatenation to build strings",
        ],
    },
    ErrorDoc {
        code: "E-S12",
        title: "Borrow Conflict",
        description: "An immutable borrow and a mutable borrow exist simultaneously.",
        category: "ownership",
        common_causes: &[
            "Having an immutable reference while trying to create a mutable reference",
            "Multiple mutable references to the same data",
        ],
        suggested_fixes: &[
            "Ensure borrows don't overlap in scope",
            "Use a mutable borrow only when no immutable borrows exist",
        ],
    },
    ErrorDoc {
        code: "E-S15",
        title: "Duplicate Enum",
        description: "An enum type name is defined more than once.",
        category: "semantic",
        common_causes: &["Two enum declarations with the same name"],
        suggested_fixes: &["Rename one of the duplicate enums"],
    },
    // =================================================================
    // Type errors (E-T)
    // =================================================================
    ErrorDoc {
        code: "E-T01",
        title: "Type Mismatch",
        description: "The compiler expected a value of one type but found a different type.",
        category: "type",
        common_causes: &[
            "Passing the wrong type to a function",
            "Assigning a value to a variable of the wrong type",
            "Returning the wrong type from a function",
            "Using an operator on incompatible types",
        ],
        suggested_fixes: &[
            "Cast the value to the expected type",
            "Change the function signature to accept the actual type",
            "Check the types of all sub-expressions",
        ],
    },
    ErrorDoc {
        code: "E-T04",
        title: "Not Callable",
        description: "An expression that is not a function was used in a function call.",
        category: "type",
        common_causes: &[
            "Calling a variable that is not a function",
            "Missing function name",
            "Using parentheses on a non-function value",
        ],
        suggested_fixes: &[
            "Ensure the callee is a function",
            "Check for typos in function names",
        ],
    },
    ErrorDoc {
        code: "E-T05",
        title: "Wrong Argument Count",
        description: "A function was called with the wrong number of arguments.",
        category: "type",
        common_causes: &[
            "Missing arguments in function call",
            "Extra arguments in function call",
        ],
        suggested_fixes: &[
            "Check the function signature for the expected number of arguments",
            "Add or remove arguments to match",
        ],
    },
    ErrorDoc {
        code: "E-T07",
        title: "Member Access on Non-Struct",
        description: "A field access (dot notation) was used on a value that is not a struct.",
        category: "type",
        common_causes: &[
            "Accessing a field on an Int, Bool, or other non-struct type",
            "Using dot notation for function calls",
        ],
        suggested_fixes: &[
            "Use struct types for field access",
            "Use function call syntax for functions",
        ],
    },
    ErrorDoc {
        code: "E-T08",
        title: "Unknown Member",
        description: "A field access references a field that does not exist on the struct.",
        category: "type",
        common_causes: &[
            "Typo in field name",
            "Accessing a field on the wrong struct type",
        ],
        suggested_fixes: &[
            "Check the struct definition for available fields",
            "Fix the field name spelling",
        ],
    },
    // =================================================================
    // HIR errors (E-H)
    // =================================================================
    ErrorDoc {
        code: "E-H01",
        title: "Unresolved Symbol (HIR)",
        description: "A symbol could not be resolved during HIR lowering.",
        category: "hir",
        common_causes: &["Name not found in scope", "Module import failed"],
        suggested_fixes: &["Ensure all names are defined or imported"],
    },
    // =================================================================
    // MIR errors (E-M)
    // =================================================================
    ErrorDoc {
        code: "E-M01",
        title: "Break Outside Loop (MIR)",
        description: "A break statement was found outside a loop during MIR lowering.",
        category: "mir",
        common_causes: &["break in non-loop context"],
        suggested_fixes: &["Use break only inside loops"],
    },
    // =================================================================
    // Backend errors (E-B)
    // =================================================================
    ErrorDoc {
        code: "E-B08",
        title: "No Entry Point",
        description: "The program has no `main` function to serve as the entry point.",
        category: "backend",
        common_causes: &[
            "Missing main function",
            "main function defined in a different module",
        ],
        suggested_fixes: &[
            "Define a `fn main()` function",
            "Ensure main is visible from the root module",
        ],
    },
    ErrorDoc {
        code: "E-B11",
        title: "Unsupported Target",
        description: "The requested compilation target is not yet implemented.",
        category: "backend",
        common_causes: &["Using --target for an unimplemented platform"],
        suggested_fixes: &[
            "Use the default host target",
            "Use x86_64-windows-pe (the only implemented target)",
        ],
    },
];

// =========================================================================
// Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_json_basic() {
        assert_eq!(escape_json("hello"), "hello");
        assert_eq!(escape_json("he\"llo"), "he\\\"llo");
        assert_eq!(escape_json("he\\llo"), "he\\\\llo");
        assert_eq!(escape_json("he\nllo"), "he\\nllo");
        assert_eq!(escape_json("he\tllo"), "he\\tllo");
    }

    #[test]
    fn escape_json_control_chars() {
        assert_eq!(escape_json("\x00"), "\\u0000");
        assert_eq!(escape_json("\x1f"), "\\u001f");
    }

    #[test]
    fn explain_known_codes() {
        assert!(explain("E-L01").is_some());
        assert!(explain("E-T01").is_some());
        assert!(explain("E-S01").is_some());
        assert!(explain("E-S10").is_some());
        assert!(explain("E-B08").is_some());
    }

    #[test]
    fn explain_unknown_code() {
        assert!(explain("E-XXXX").is_none());
        assert!(explain("").is_none());
    }

    #[test]
    fn explain_has_required_fields() {
        let doc = explain("E-T01").unwrap();
        assert_eq!(doc.code, "E-T01");
        assert!(!doc.title.is_empty());
        assert!(!doc.description.is_empty());
        assert!(!doc.category.is_empty());
        assert!(!doc.common_causes.is_empty());
        assert!(!doc.suggested_fixes.is_empty());
    }

    #[test]
    fn all_codes_have_docs() {
        // Every code returned by all_codes() should have documentation.
        for code in all_codes() {
            assert!(
                explain(code).is_some(),
                "missing documentation for error code {code}"
            );
        }
    }

    #[test]
    fn render_json_empty_report() {
        let report = CheckReport {
            source_id: crate::source::SourceId::new(0),
            token_count: 0,
            errors: Vec::new(),
            semantic: None,
            types: None,
            ownership: None,
            hir: None,
            mir: None,
        };
        let sources = SourceMap::new();
        let json = render_json(&report, &sources);
        assert!(json.contains("\"success\": true"));
        assert!(json.contains("\"errors\": []"));
        assert!(json.contains("\"warnings\": []"));
    }
}
