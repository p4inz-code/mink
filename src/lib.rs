//! MINK compiler library.
//!
//! MINK is a general-purpose programming language (see
//! `docs/core/MINK_MASTER_SPEC.md`). This crate hosts the compiler
//! implementation: the CLI driver, the compiler pipeline, and the shared
//! source infrastructure that every later stage builds upon.
//!
//! The lexer, the parser/AST, semantic analysis, the type-system
//! foundation, the HIR layer, the MIR layer, and the MIR optimization
//! pipeline are implemented (see
//! `docs/implementation/LEXER_IMPLEMENTATION.md`,
//! `docs/implementation/PARSER_IMPLEMENTATION.md`,
//! `docs/implementation/SEMANTIC_ANALYSIS_IMPLEMENTATION.md`,
//! `docs/implementation/TYPE_SYSTEM_IMPLEMENTATION.md`,
//! `docs/implementation/HIR_IMPLEMENTATION.md`,
//! `docs/implementation/MIR_IMPLEMENTATION.md`, and
//! `docs/implementation/OPTIMIZATION_IMPLEMENTATION.md`); the `diagnostics`
//! module is a structural placeholder at this stage; see
//! `docs/implementation/ENGINEERING_FOUNDATION.md` for the layout rationale.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod ast;
pub mod cli;
pub mod diagnostics;
pub mod driver;
pub mod hir;
pub mod lexer;
pub mod mir;
pub mod parser;
pub mod semantics;
pub mod source;
pub mod typecheck;
