//! MINK compiler binary entry point.
//!
//! All logic lives in the `mink` library so it can be reused by the test
//! suite and future tooling (LSP, formatter, etc.).

#![forbid(unsafe_code)]

fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    mink::cli::main(&args)
}
