//! Lexical analysis (placeholder).
//!
//! Planned responsibility: turn MINK source text into a token stream,
//! preserving an accurate source [`Span`](crate::source::Span) on every
//! token and emitting lexical diagnostics through the diagnostics
//! subsystem.
//!
//! Reference: `docs/compiler/COMPILER_ARCHITECTURE.md` §3 (Frontend).
//!
//! Not yet implemented; this module exists to establish the subsystem
//! layout so later sessions can implement it incrementally.
