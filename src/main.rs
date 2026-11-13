//! MINK compiler binary entry point.
//!
//! All logic lives in the `mink` library so it can be reused by the test
//! suite and future tooling (LSP, formatter, etc.).

#![forbid(unsafe_code)]

use std::process::ExitCode;

/// Stack reserved for the worker thread that runs the command.
///
/// Every compiler stage is recursive over nested constructs: the parser, both
/// type passes, ownership analysis, HIR and MIR lowering, and the backend all
/// walk nested syntax with their own stack frames. Running the command on a
/// thread with a generous stack keeps legal (if unusual) input such as
/// generated expressions from reaching an implementation stack limit before
/// the parser's documented nesting limit, which is the only bound a program
/// can hit and it produces a diagnostic rather than a crash.
///
/// Windows reserves this address space and commits pages only as the stack
/// grows, so the ceiling costs nothing for ordinary programs.
const COMPILE_STACK_BYTES: usize = 64 * 1024 * 1024;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let worker_args = args.clone();
    match std::thread::Builder::new()
        .name("mink".to_string())
        .stack_size(COMPILE_STACK_BYTES)
        .spawn(move || mink::cli::main(&worker_args))
    {
        // A panic in the compiler has already printed its message; report the
        // conventional panic status instead of a silent success.
        Ok(handle) => match handle.join() {
            Ok(code) => code,
            Err(_) => ExitCode::from(101),
        },
        // The thread could not be created (e.g. address-space exhaustion): run
        // on the main thread rather than refusing to compile.
        Err(_) => mink::cli::main(&args),
    }
}
