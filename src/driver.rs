//! Compiler pipeline orchestration.
//!
//! Owns the sequence Source → Lexer → Parser → AST → Semantic Analysis →
//! Backend (see `docs/compiler/COMPILER_ARCHITECTURE.md` §2). At this stage
//! the driver wires the entry point to the source infrastructure and reports
//! that the remaining stages are not yet implemented.

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

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
    /// The pipeline has not been implemented past the source-loading stage.
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

/// Runs the compiler pipeline for a single MINK source file.
///
/// Returns the id of the source file registered in `sources` once the full
/// pipeline has run. Current status: the driver registers the file, which
/// exercises the driver → source infrastructure wiring end to end, and then
/// reports [`BuildError::NotImplemented`] because the lexer, parser, and all
/// later stages are not implemented yet (see
/// `docs/implementation/ENGINEERING_FOUNDATION.md`).
pub fn build(sources: &mut SourceMap, path: &Path) -> Result<SourceId, BuildError> {
    let _id = sources.load(path).map_err(|source| BuildError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    Err(BuildError::NotImplemented)
}
