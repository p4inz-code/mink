//! Command-line interface for the MINK compiler.
//!
//! Parses the process arguments, dispatches to the requested command, and
//! maps outcomes to process exit codes. Intentionally dependency-free and
//! minimal; it will grow alongside the commands it serves.

use std::path::PathBuf;
use std::process::ExitCode;

use crate::driver::{self, BuildError, CheckReport};
use crate::source::{SourceFile, SourceMap, Span};

/// Version string from the package manifest (e.g. `0.1.0`).
const VERSION: &str = env!("CARGO_PKG_VERSION");

const USAGE: &str = "\
MINK compiler — implementation foundation

Usage:
  mink <command> [arguments]

Commands:
  build <path>    Load a MINK source file and run the build pipeline
  check <path>    Lex and parse a MINK source file
  run <path>      Build and execute a MINK source file (not yet implemented)
  test [path]     Run MINK tests (not yet implemented)
  fmt [path]      Format MINK source (not yet implemented)
  version         Print the compiler version
  help            Print this help

Options:
  -h, --help      Print help
  -V, --version   Print version

Exit codes:
  0   success
  1   usage or input error
  2   command not yet implemented
";

/// A parsed command-line invocation.
enum Command {
    Version,
    Help,
    Build {
        path: PathBuf,
    },
    Check {
        path: PathBuf,
    },
    /// A recognized command whose implementation has not landed yet.
    NotImplemented {
        name: &'static str,
    },
}

/// Entry point for the compiler process. Returns the process exit code.
///
/// Prints help and version information to stdout; reports all errors to
/// stderr.
pub fn main(args: &[String]) -> ExitCode {
    match parse(args) {
        Ok(Command::Version) => {
            println!("mink {VERSION}");
            ExitCode::SUCCESS
        }
        Ok(Command::Help) => {
            print!("{USAGE}");
            ExitCode::SUCCESS
        }
        Ok(Command::Build { path }) => {
            let mut sources = SourceMap::new();
            match driver::build(&mut sources, &path) {
                Ok(_source_id) => ExitCode::SUCCESS,
                Err(error) => {
                    eprintln!("mink: error: {error}");
                    build_error_exit_code(&error)
                }
            }
        }
        Ok(Command::Check { path }) => {
            let mut sources = SourceMap::new();
            match driver::check(&mut sources, &path) {
                Ok(report) => {
                    if report.errors.is_empty() {
                        println!(
                            "mink: check: '{}' passed parsing ({} tokens)",
                            path.display(),
                            report.token_count
                        );
                        ExitCode::SUCCESS
                    } else {
                        print_errors(&sources, &report);
                        ExitCode::from(1)
                    }
                }
                Err(error) => {
                    eprintln!("mink: error: {error}");
                    build_error_exit_code(&error)
                }
            }
        }
        Ok(Command::NotImplemented { name }) => {
            eprintln!("mink: error: '{name}' is not yet implemented");
            ExitCode::from(2)
        }
        Err(message) => {
            eprintln!("mink: error: {message}");
            eprintln!("Run 'mink help' for usage.");
            ExitCode::from(1)
        }
    }
}

/// Prints diagnostics for `report` to stderr.
///
/// This is a minimal, temporary rendering until the structured diagnostic
/// engine lands (see `docs/implementation/PARSER_IMPLEMENTATION.md`). Each
/// error is printed with its stable code, message, and source location,
/// whether it is lexical or syntactic.
fn print_errors(sources: &SourceMap, report: &CheckReport) {
    let Some(file) = sources.get(report.source_id) else {
        return;
    };
    for error in &report.errors {
        eprintln!("mink: error[{}]: {}", error.code(), error);
        print_span_location(file, error.span());
    }
}

/// Prints a `--> file:line:column` location line for `span`.
fn print_span_location(file: &SourceFile, span: Span) {
    let line_col = file.line_col(span.start());
    eprintln!(
        "  --> {}:{}:{}",
        file.name().display(),
        line_col.line,
        line_col.column
    );
}

/// Maps a build failure to a process exit code.
fn build_error_exit_code(error: &BuildError) -> ExitCode {
    match error {
        BuildError::Io { .. } => ExitCode::from(1),
        BuildError::NotImplemented => ExitCode::from(2),
    }
}

/// Parses `args` (everything after the program name) into a [`Command`].
fn parse(args: &[String]) -> Result<Command, String> {
    let Some(first) = args.first() else {
        return Ok(Command::Help);
    };
    match first.as_str() {
        "help" | "-h" | "--help" => Ok(Command::Help),
        "version" | "-V" | "--version" => Ok(Command::Version),
        "build" => {
            let path = args
                .get(1)
                .ok_or("missing path argument for 'build' (usage: mink build <path>)")?;
            Ok(Command::Build {
                path: PathBuf::from(path),
            })
        }
        "check" => {
            let path = args
                .get(1)
                .ok_or("missing path argument for 'check' (usage: mink check <path>)")?;
            Ok(Command::Check {
                path: PathBuf::from(path),
            })
        }
        "run" => Ok(Command::NotImplemented { name: "run" }),
        "test" => Ok(Command::NotImplemented { name: "test" }),
        "fmt" => Ok(Command::NotImplemented { name: "fmt" }),
        other => Err(format!("unknown command '{other}'")),
    }
}
