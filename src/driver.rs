//! Compiler pipeline orchestration.
//!
//! Owns the sequence Source → Lexer → Parser → AST → Semantic Analysis →
//! Backend (see `docs/compiler/COMPILER_ARCHITECTURE.md` §2). At this stage
//! the driver runs the source-loading and lexical-analysis stages; the
//! parser and everything after it are not yet implemented.

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use crate::lexer::{self, LexError, TokenKind};
use crate::source::{SourceId, SourceMap};

/// Errors produced while running the build pipeline.
#[derive(Debug)]
pub enum BuildError {
    /// The source file could not be read from disk.
    Io {
        /// The path that could not be read.
        path: PathBuf,
        /// The underlying I/O error.
        source: io::Error,
    },
    /// The pipeline has not been implemented past the lexical-analysis stage.
    NotImplemented,
}

impl fmt::Display for BuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => {
                write!(f, "failed to read '{}': {source}", path.display())
            }
            Self::NotImplemented => {
                write!(f, "the build pipeline is not yet implemented")
            }
        }
    }
}

impl std::error::Error for BuildError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::NotImplemented => None,
        }
    }
}

/// The result of running lexical analysis on one source file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckReport {
    /// The id of the checked source file.
    pub source_id: SourceId,
    /// Number of tokens produced, excluding the final `Eof` token.
    pub token_count: usize,
    /// Lexical errors, in source order. Empty for lexically valid input.
    pub errors: Vec<LexError>,
}

/// Loads `path` and runs lexical analysis over it.
///
/// On success returns a [`CheckReport`] describing the token stream and any
/// lexical errors; the caller decides how to surface them. An I/O failure to
/// read the file is reported as [`BuildError::Io`].
pub fn check(sources: &mut SourceMap, path: &Path) -> Result<CheckReport, BuildError> {
    let source_id = sources.load(path).map_err(|source| BuildError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let file = sources
        .get(source_id)
        .expect("the file id returned by load is always registered");
    let lexed = lexer::lex(file);
    let token_count = lexed
        .tokens()
        .iter()
        .filter(|token| token.kind() != TokenKind::Eof)
        .count();
    Ok(CheckReport {
        source_id,
        token_count,
        errors: lexed.into_errors(),
    })
}

/// Runs the compiler pipeline for a single MINK source file.
///
/// Returns the id of the source file registered in `sources`. Current status:
/// the driver registers the file, which exercises the driver → source
/// infrastructure wiring end to end, and then reports
/// [`BuildError::NotImplemented`] because the parser and all later stages are
/// not implemented yet (see `docs/implementation/ENGINEERING_FOUNDATION.md`).
pub fn build(sources: &mut SourceMap, path: &Path) -> Result<SourceId, BuildError> {
    let _id = sources.load(path).map_err(|source| BuildError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    Err(BuildError::NotImplemented)
}
