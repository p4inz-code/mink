//! Code emission: turning verified backend instructions into a machine
//! image for a selected [`Target`](super::Target).
//!
//! Emission is the only target-specific part of the backend. The
//! instruction representation ([`BProgram`](super::ir::BProgram)) is
//! target-independent; each target implements an emitter that produces its
//! native image. This milestone implements exactly one emitter — x86-64
//! machine code in a Windows PE container — and reports every other target
//! with a structured error instead of emitting the wrong output.

pub(crate) mod pe;
pub(crate) mod x86_64;

use crate::source::{SourceId, Span};

use super::error::BackendError;
use super::ir::BProgram;
use super::target::Target;

/// The result of emitting a program for a target: a complete machine image
/// plus the metadata needed by the driver and future tooling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmittedImage {
    /// The complete executable image (for the first target, a PE file).
    pub bytes: Vec<u8>,
    /// The number of emitted functions.
    pub functions: usize,
    /// The number of emitted module bindings.
    pub statics: usize,
    /// The name of the entry function the image starts at.
    pub entry: String,
}

/// A span for errors that have no source location (target selection).
fn no_location() -> Span {
    Span::new(SourceId::new(0), 0..0)
}

/// Emits `program` for `target`, with `entry` the index of the entry
/// function (validated by the caller).
pub(crate) fn emit(
    program: &BProgram,
    target: Target,
    entry: usize,
) -> Result<EmittedImage, Vec<BackendError>> {
    match target {
        Target::X86_64WindowsPe => Ok(x86_64::emit_pe(program, entry)),
        other => Err(vec![BackendError::unsupported_target(
            no_location(),
            format!(
                "the target `{}` is recognized but not implemented yet",
                other.name()
            ),
        )]),
    }
}
