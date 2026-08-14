//! Native target selection and validation.
//!
//! The backend is target-independent up to the emission boundary: MIR is
//! lowered into a portable instruction representation
//! ([`ir`](super::ir)) that any target consumes, and each [`Target`]
//! supplies an emitter that turns those instructions into machine code.
//! This milestone implements exactly one native target —
//! `x86_64-windows-pe`, a 64-bit x86-64 executable image for Windows — and
//! recognizes the other mainstream targets so requesting them fails with a
//! structured error instead of silently emitting the wrong output.
//!
//! The x86-64 Windows target is the first because the build environment is
//! x86-64 Windows and the target needs no external toolchain: the emitter
//! produces a complete PE (Portable Executable) image directly. The target
//! abstraction keeps the door open for `x86_64-linux-elf`, `aarch64`, and
//! other backends (see `docs/implementation/NATIVE_BACKEND_IMPLEMENTATION.md`).

use std::fmt;

/// A compilation target: an instruction-set/operating-system/format triple
/// the backend can emit for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Target {
    /// 64-bit x86-64, Windows, PE executable image. The first implemented
    /// native target.
    X86_64WindowsPe,
    /// 64-bit x86-64, Linux, ELF executable. Recognized; not implemented
    /// yet.
    X86_64LinuxElf,
    /// 64-bit ARM, Linux, ELF executable. Recognized; not implemented yet.
    AArch64LinuxElf,
}

impl Target {
    /// The host's native target.
    ///
    /// The first milestone implements a single target
    /// (`x86_64-windows-pe`), so `native()` currently selects it on every
    /// host. On non-Windows hosts the emitted image is a Windows PE and
    /// must be run on a Windows system; `--target` selects explicitly.
    pub fn native() -> Self {
        Self::X86_64WindowsPe
    }

    /// Parses a target name (the `--target` CLI argument) into a
    /// [`Target`].
    ///
    /// Returns `None` for names that are not recognized at all; recognized
    /// but unimplemented targets parse successfully and fail later at
    /// emission with a structured error ([`BackendErrorKind::UnsupportedTarget`](super::BackendErrorKind::UnsupportedTarget)).
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "x86_64-windows-pe" => Some(Self::X86_64WindowsPe),
            "x86_64-linux-elf" => Some(Self::X86_64LinuxElf),
            "aarch64-linux-elf" => Some(Self::AArch64LinuxElf),
            _ => None,
        }
    }

    /// The canonical name of this target, as accepted by [`Target::parse`].
    pub fn name(self) -> &'static str {
        match self {
            Self::X86_64WindowsPe => "x86_64-windows-pe",
            Self::X86_64LinuxElf => "x86_64-linux-elf",
            Self::AArch64LinuxElf => "aarch64-linux-elf",
        }
    }

    /// Whether this target has an emitter implementation in this milestone.
    pub fn is_implemented(self) -> bool {
        matches!(self, Self::X86_64WindowsPe)
    }
}

impl fmt::Display for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// The recognized target names, for CLI help and diagnostics.
pub const TARGET_NAMES: &[&str] = &[
    "x86_64-windows-pe (implemented)",
    "x86_64-linux-elf (not yet implemented)",
    "aarch64-linux-elf (not yet implemented)",
];
