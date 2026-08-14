//! Command-line interface for the MINK compiler.
//!
//! Parses the process arguments, dispatches to the requested command, and
//! maps outcomes to process exit codes. Intentionally dependency-free and
//! minimal; it will grow alongside the commands it serves.

use std::path::PathBuf;
use std::process::ExitCode;

use crate::backend::{BackendError, TARGET_NAMES, Target};
use crate::driver::{self, BuildError, CheckError, CheckReport};
use crate::semantics::SemanticErrorKind;
use crate::source::{SourceFile, SourceMap, Span};

/// Version string from the package manifest (e.g. `0.1.0`).
const VERSION: &str = env!("CARGO_PKG_VERSION");

const USAGE: &str = "\
MINK compiler — implementation foundation

Usage:
  mink <command> [arguments]

Commands:
  build <path> [--target <target>]
                  Compile a MINK source file into a native executable
  check <path>    Lex, parse, semantically analyze, type check, lower, and
                  optimize a MINK source file through HIR and MIR
  run <path>      Build and execute a MINK source file (not yet implemented)
  test [path]     Run MINK tests (not yet implemented)
  fmt [path]      Format MINK source (not yet implemented)
  version         Print the compiler version
  help            Print this help

Options:
  -h, --help      Print help
  -V, --version   Print version
  --target <name> Target to compile for (default: the host's native target)

Targets:
  x86_64-windows-pe       (implemented)
  x86_64-linux-elf        (not yet implemented)
  aarch64-linux-elf       (not yet implemented)

Exit codes:
  0   success
  1   usage, input, or compilation error
  2   command not yet implemented
";

/// A parsed command-line invocation.
enum Command {
    Version,
    Help,
    Build {
        path: PathBuf,
        target: Target,
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
        Ok(Command::Build { path, target }) => {
            let mut sources = SourceMap::new();
            let options = driver::BuildOptions { target };
            match driver::build(&mut sources, &path, options) {
                Ok(outcome) => {
                    println!(
                        "mink: build: '{}' -> '{}' (target: {}, {} function(s), {} binding(s))",
                        path.display(),
                        outcome.output.display(),
                        outcome.target,
                        outcome.functions,
                        outcome.statics
                    );
                    ExitCode::SUCCESS
                }
                Err(BuildError::FrontEnd(report)) => {
                    print_errors(&sources, &report);
                    ExitCode::from(1)
                }
                Err(BuildError::Backend(errors)) => {
                    print_backend_errors(&sources, &errors);
                    ExitCode::from(1)
                }
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
                            "mink: check: '{}' passed parsing, semantic analysis, type checking, HIR lowering, MIR lowering, and MIR optimization ({} tokens)",
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
/// whether it is lexical, syntactic, semantic, or a type error; errors that
/// reference a related location (such as the original declaration of a
/// duplicate definition, or the target of a mismatched assignment) print a
/// note for that location too.
fn print_errors(sources: &SourceMap, report: &CheckReport) {
    let Some(file) = sources.get(report.source_id) else {
        return;
    };
    for error in &report.errors {
        eprintln!("mink: error[{}]: {}", error.code(), error);
        print_span_location(file, error.span());
        if let Some(related) = error.related_span() {
            let note = match error {
                CheckError::Semantic(semantic)
                    if semantic.kind() == SemanticErrorKind::DuplicateDefinition =>
                {
                    "previous declaration is here"
                }
                _ => "related location is here",
            };
            eprintln!("  = note: {note}");
            print_span_location(file, related);
        }
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

/// Prints backend diagnostics to stderr, each with its stable code and
/// source location.
fn print_backend_errors(sources: &SourceMap, errors: &[BackendError]) {
    for error in errors {
        eprintln!("mink: error[{}]: {error}", error.code());
        if let Some(file) = sources.get(error.span().file()) {
            print_span_location(file, error.span());
        }
    }
}

/// Maps a build failure to a process exit code.
fn build_error_exit_code(error: &BuildError) -> ExitCode {
    match error {
        BuildError::Io { .. }
        | BuildError::FrontEnd(_)
        | BuildError::Backend(_)
        | BuildError::Output { .. } => ExitCode::from(1),
    }
}

/// Parses the arguments of the `build` command: a path plus an optional
/// `--target <name>` (or `--target=<name>`) in either order.
fn parse_build(args: &[String]) -> Result<Command, String> {
    let mut path: Option<PathBuf> = None;
    let mut target = Target::native();
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        if arg == "--target" {
            index += 1;
            let name = args
                .get(index)
                .ok_or("missing target name after '--target' (usage: mink build <path> [--target <target>])")?;
            target = parse_target(name)?;
        } else if let Some(name) = arg.strip_prefix("--target=") {
            target = parse_target(name)?;
        } else if arg.starts_with('-') {
            return Err(format!("unknown option '{arg}' for 'build'"));
        } else {
            if path.is_some() {
                return Err(format!("unexpected argument '{arg}' for 'build'"));
            }
            path = Some(PathBuf::from(arg));
        }
        index += 1;
    }
    let path = path.ok_or(
        "missing path argument for 'build' (usage: mink build <path> [--target <target>])",
    )?;
    Ok(Command::Build { path, target })
}

/// Parses a `--target` name, listing the recognized targets on failure.
fn parse_target(name: &str) -> Result<Target, String> {
    Target::parse(name).ok_or_else(|| {
        format!(
            "unknown target '{name}' (supported targets: {})",
            TARGET_NAMES.join(", ")
        )
    })
}

/// Parses `args` (everything after the program name) into a [`Command`].
fn parse(args: &[String]) -> Result<Command, String> {
    let Some(first) = args.first() else {
        return Ok(Command::Help);
    };
    match first.as_str() {
        "help" | "-h" | "--help" => Ok(Command::Help),
        "version" | "-V" | "--version" => Ok(Command::Version),
        "build" => parse_build(&args[1..]),
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
