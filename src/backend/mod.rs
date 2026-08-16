//! The native backend: optimized MIR → machine code.
//!
//! This is the code-generation stage of the pipeline. It consumes the
//! optimized [`MirProgram`] and produces an executable machine image:
//!
//! ```text
//! Optimized MIR → lowering → backend instructions → verification
//!     → emission → machine image
//! ```
//!
//! The module is organized around a target-independent core and a
//! target-specific emission layer:
//!
//! - [`lower`] walks the optimized MIR once and produces the portable
//!   instruction representation [`ir`] — a canonical instruction stream
//!   with classified value types, explicit storage, and preserved source
//!   spans. Everything outside the first native subset (floating point,
//!   strings, member/index places, …) is rejected here with a structured
//!   error instead of being miscompiled;
//! - [`verify`] checks the lowered program's structural integrity
//!   defensively, so malformed hand-built or mutated instructions fail
//!   cleanly (`E-B07`);
//! - [`emit`] turns verified instructions into a machine image for a
//!   selected [`Target`]. The first milestone implements one target —
//!   `x86_64-windows-pe`, a self-contained x86-64 code generator that
//!   assembles a complete Windows PE image with no external toolchain —
//!   and reports the recognized-but-unimplemented targets with a
//!   structured error (`E-B11`).
//!
//! [`compile`] is the public entry point: it finds and validates the entry
//! function (`main`), lowers, verifies, and emits. Diagnostics carry stable
//! codes `E-B01`…`E-B12` (see [`error`] and
//! `docs/implementation/NATIVE_BACKEND_IMPLEMENTATION.md`).

mod emit;
mod error;
mod ir;
mod lower;
mod target;
mod verify;

use crate::mir::MirProgram;
use crate::source::{SourceId, SourceMap, Span};

pub use emit::EmittedImage;
pub use error::{BackendError, BackendErrorKind};
pub use ir::{
    BBlock, BFunction, BInst, BInstKind, BLocal, BOperand, BProgram, BStatic, BTerminator, BType,
    RuntimeService,
};
pub use target::{TARGET_NAMES, Target};

/// Lowers an optimized [`MirProgram`] into the backend instruction
/// representation, rejecting constructs outside the native subset with
/// structured errors (`E-B01`…`E-B06`). See [`compile`] for the full
/// pipeline.
pub fn lower(program: &MirProgram, sources: &SourceMap) -> Result<BProgram, Vec<BackendError>> {
    lower::lower(program, sources)
}

/// Verifies the structural integrity of a [`BProgram`], reporting every
/// problem as a [`BackendError`] (`E-B07`) instead of panicking.
///
/// Lowering always produces valid programs; this defends the pipeline and
/// tooling against malformed hand-built or mutated instructions.
pub fn verify(program: &BProgram) -> Result<(), Vec<BackendError>> {
    verify::verify(program)
}

/// A span for errors that have no source location (the missing entry
/// function).
fn no_location() -> Span {
    Span::new(SourceId::new(0), 0..0)
}

/// Compiles an optimized [`MirProgram`] into an executable machine image
/// for `target`.
///
/// Returns the image, or every [`BackendError`] collected by lowering,
/// verification, and emission in deterministic order. A program is only
/// compiled when it stays entirely inside the supported native subset;
/// anything else fails with a structured error instead of producing
/// incorrect output.
pub fn compile(
    program: &MirProgram,
    sources: &SourceMap,
    target: Target,
) -> Result<EmittedImage, Vec<BackendError>> {
    let lowered = lower::lower(program, sources)?;
    verify::verify(&lowered)?;
    let entry = entry_function(&lowered).map_err(|error| vec![error])?;
    emit::emit(&lowered, target, entry)
}

/// Finds and validates the program's entry function, returning its index in
/// the lowered function list.
///
/// The first native subset requires a module-level `fn main()` with no
/// parameters; its result (an integer, boolean, or nothing) becomes the
/// process exit code.
fn entry_function(lowered: &BProgram) -> Result<usize, BackendError> {
    let Some(index) = lowered.functions.iter().position(|f| f.name == "main") else {
        return Err(BackendError::no_entry_point(no_location()));
    };
    let main = &lowered.functions[index];
    if !main.params.is_empty() {
        return Err(BackendError::invalid_entry_point(
            main.span,
            "the entry function `main` must not take parameters",
        ));
    }
    // The entry stub passes `main`'s result in `rax` to the exit service;
    // an aggregate result (a struct, array, or tagged-union enum — even a
    // one-word struct — or a two-word `Range`) cannot become an exit
    // code, so every aggregate result is rejected here (E-B09).
    if matches!(
        main.result,
        BType::Range | BType::Ptr | BType::Str | BType::Struct | BType::Array | BType::Enum
    ) || main.result_words > 1
    {
        return Err(BackendError::invalid_entry_point(
            main.span,
            "the entry function `main` must produce an integer, a boolean, or nothing",
        ));
    }
    Ok(index)
}
