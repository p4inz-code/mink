//! MINK compiler library.
//!
//! MINK is a general-purpose programming language (see
//! `docs/core/MINK_MASTER_SPEC.md`). This crate hosts the compiler
//! implementation: the CLI driver, the compiler pipeline, and the shared
//! source infrastructure that every later stage builds upon.
//!
//! Subsystem modules (`lexer`, `parser`, `ast`, `semantics`, `diagnostics`)
//! are structural placeholders at this stage; see
//! `docs/implementation/ENGINEERING_FOUNDATION.md` for the layout rationale.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod ast;
pub mod cli;
pub mod diagnostics;
pub mod driver;
pub mod lexer;
pub mod parser;
pub mod semantics;
pub mod source;
