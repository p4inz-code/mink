//! Syntax representation / AST (placeholder).
//!
//! Planned responsibility: the typed syntax tree produced by the parser and
//! consumed by name resolution, type checking, and semantic analysis. Every
//! node will carry a source [`Span`](crate::source::Span).
//!
//! Reference: `docs/compiler/COMPILER_ARCHITECTURE.md` §2 (Pipeline) and
//! §3 (Frontend).
//!
//! Not yet implemented; this module exists to establish the subsystem
//! layout so later sessions can implement it incrementally.
