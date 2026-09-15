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

/// Version string from the package manifest (e.g. `1.0.0`).
const VERSION: &str = env!("CARGO_PKG_VERSION");

const USAGE: &str = "\
MINK compiler — a general-purpose systems programming language

Usage:
  mink <command> [arguments]

Commands:
  build <path> [--target <target>]
                  Compile a MINK source file into a native executable
  run <path>      Compile and execute a MINK source file
  check <path> [--json]
                  Analyze a MINK source file without producing output
  test <path>     Discover and run test functions (fn test_*) in a source file
  explain [code]  Explain an error code (e.g., mink explain E-T01);
                  with no code, list all documented error codes
  version         Print the compiler version
  help            Print this help

Options:
  -h, --help      Print help
  -v, -V, --version
                  Print the compiler version
  --json          Output machine-readable JSON (for check)
  --target <name> Target to compile for (default: the host's native target)

Examples:
  mink run hello.mink        Compile and run a program
  mink build hello.mink      Compile without running
  mink check hello.mink      Check for errors
  mink test hello.mink       Run test functions in hello.mink
  mink explain E-T01         Explain error E-T01

Exit codes:
  0   success
  1   usage, input, or compilation error
";

/// A parsed command-line invocation.
enum Command {
    Version,
    Help,
    Build { path: PathBuf, target: Target },
    Run { path: PathBuf, target: Target },
    Check { path: PathBuf, json: bool },
    Test { path: PathBuf, target: Target },
    Explain { code: Option<String> },
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
        Ok(Command::Check { path, json }) => {
            let mut sources = SourceMap::new();
            match driver::check(&mut sources, &path) {
                Ok(report) => {
                    if json {
                        // Machine-readable JSON output.
                        print!("{}", crate::diagnostics::render_json(&report, &sources));
                        if report.errors.is_empty() {
                            ExitCode::SUCCESS
                        } else {
                            ExitCode::from(1)
                        }
                    } else if report.errors.is_empty() {
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
        Ok(Command::Explain { code: None }) => {
            // No code given: list all documented error codes.
            let mut codes = crate::diagnostics::all_codes();
            codes.sort_unstable();
            codes.dedup();
            println!("MINK documented error codes ({}):", codes.len());
            for chunk in codes.chunks(8) {
                println!("  {}", chunk.join(" "));
            }
            ExitCode::SUCCESS
        }
        Ok(Command::Explain { code: Some(code) }) => match crate::diagnostics::explain(&code) {
            Some(doc) => {
                println!("Error {}: {}", doc.code, doc.title);
                println!();
                println!("Category: {}", doc.category);
                println!();
                println!("{}", doc.description);
                println!();
                println!("Common causes:");
                for cause in doc.common_causes {
                    println!("  - {cause}");
                }
                println!();
                println!("Suggested fixes:");
                for fix in doc.suggested_fixes {
                    println!("  - {fix}");
                }
                ExitCode::SUCCESS
            }
            None => {
                eprintln!("mink: error: unknown error code '{code}'");
                eprintln!("Run 'mink explain' without arguments to see available codes.");
                ExitCode::from(1)
            }
        },
        Ok(Command::Run { path, target }) => {
            let mut sources = SourceMap::new();
            let options = driver::BuildOptions { target };
            match driver::build(&mut sources, &path, options) {
                Ok(outcome) => {
                    // Execute the generated binary and forward its exit code.
                    let result = std::process::Command::new(&outcome.output).status();
                    match result {
                        Ok(status) => {
                            // Clean up the generated executable.
                            let _ = std::fs::remove_file(&outcome.output);
                            ExitCode::from(status.code().unwrap_or(1) as u8)
                        }
                        Err(error) => {
                            eprintln!(
                                "mink: error: failed to execute '{}': {error}",
                                outcome.output.display()
                            );
                            let _ = std::fs::remove_file(&outcome.output);
                            ExitCode::from(1)
                        }
                    }
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
        Ok(Command::Test { path, target }) => run_test_command(&path, target),
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
        | BuildError::NotAFile { .. }
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

/// Parses the arguments of the `run` command: a path plus an optional
/// `--target <name>` (or `--target=<name>`) in either order.
fn parse_run(args: &[String]) -> Result<Command, String> {
    let mut path: Option<PathBuf> = None;
    let mut target = Target::native();
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        if arg == "--target" {
            index += 1;
            let name = args.get(index).ok_or(
                "missing target name after '--target' (usage: mink run <path> [--target <target>])",
            )?;
            target = parse_target(name)?;
        } else if let Some(name) = arg.strip_prefix("--target=") {
            target = parse_target(name)?;
        } else if arg.starts_with('-') {
            return Err(format!("unknown option '{arg}' for 'run'"));
        } else {
            if path.is_some() {
                return Err(format!("unexpected argument '{arg}' for 'run'"));
            }
            path = Some(PathBuf::from(arg));
        }
        index += 1;
    }
    let path =
        path.ok_or("missing path argument for 'run' (usage: mink run <path> [--target <target>])")?;
    Ok(Command::Run { path, target })
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
    let no_extra = |command: &str| -> Result<(), String> {
        if args.len() > 1 {
            Err(format!("unexpected argument '{}' for '{command}'", args[1]))
        } else {
            Ok(())
        }
    };
    match first.as_str() {
        "help" => {
            no_extra("help")?;
            Ok(Command::Help)
        }
        "-h" | "--help" => Ok(Command::Help),
        "version" => {
            no_extra("version")?;
            Ok(Command::Version)
        }
        "-v" | "-V" | "--version" => Ok(Command::Version),
        "build" => parse_build(&args[1..]),
        "check" => {
            let mut path: Option<PathBuf> = None;
            let mut json = false;
            for arg in &args[1..] {
                if arg == "--json" {
                    json = true;
                } else if arg.starts_with('-') {
                    return Err(format!("unknown option '{arg}' for 'check'"));
                } else if path.is_none() {
                    path = Some(PathBuf::from(arg));
                } else {
                    return Err(format!("unexpected argument '{arg}' for 'check'"));
                }
            }
            let path = path
                .ok_or("missing path argument for 'check' (usage: mink check <path> [--json])")?;
            Ok(Command::Check { path, json })
        }
        "explain" => {
            let code = args.get(1).cloned();
            if code.is_some() && args.len() > 2 {
                return Err(format!("unexpected argument '{}' for 'explain'", args[2]));
            }
            Ok(Command::Explain { code })
        }
        "run" => parse_run(&args[1..]),
        "test" => parse_build(&args[1..]).map(|cmd| match cmd {
            Command::Build { path, target } => Command::Test { path, target },
            _ => unreachable!(),
        }),
        other => Err(format!("unknown command '{other}'")),
    }
}

/// Strips the `fn main()` function from source text so the test runner
/// can replace it with its own wrapper. This is a simple brace-counting
/// approach — it finds `fn main()` and skips to the matching closing brace
/// at the top level.
fn strip_main_fn(source: &str) -> String {
    let lines: Vec<&str> = source.lines().collect();
    let mut result = Vec::new();
    let mut skip = false;
    let mut brace_depth = 0i32;

    for line in &lines {
        if skip {
            // Count braces to find the end of the function body.
            for ch in line.chars() {
                match ch {
                    '{' => brace_depth += 1,
                    '}' => {
                        brace_depth -= 1;
                        if brace_depth == 0 {
                            skip = false;
                        }
                    }
                    _ => {}
                }
            }
        } else {
            let trimmed = line.trim_start();
            if trimmed.starts_with("fn main") {
                // Check if this line opens a body.
                if line.contains('{') {
                    skip = true;
                    for ch in line.chars() {
                        match ch {
                            '{' => brace_depth += 1,
                            '}' => {
                                brace_depth -= 1;
                                if brace_depth == 0 {
                                    skip = false;
                                }
                            }
                            _ => {}
                        }
                    }
                }
            } else {
                result.push(*line);
            }
        }
    }
    result.join("\n")
}

/// Discovers `fn test_*()` functions in `path` and runs each one as a
/// separate build+run cycle. Reports pass/fail counts.
fn run_test_command(path: &PathBuf, target: Target) -> ExitCode {
    let source = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("mink: error: failed to read '{}': {e}", path.display());
            return ExitCode::from(1);
        }
    };

    // Discover test functions by scanning for `fn test_` at the start of a line
    // (or after whitespace).  We require the function name to start with
    // `test_` and take no parameters (aside from the implicit unit return).
    let mut test_names: Vec<String> = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("fn test_") {
            // Extract function name: `fn test_xxx(...`
            if let Some(rest) = trimmed.strip_prefix("fn ") {
                if let Some(name_end) = rest.find('(') {
                    let name = rest[..name_end].trim().to_string();
                    // Only accept zero-parameter test functions.
                    if !name.is_empty() && !rest[name_end..].contains(':') {
                        test_names.push(name);
                    }
                }
            }
        }
    }

    if test_names.is_empty() {
        eprintln!("mink: no test functions found in '{}'", path.display());
        eprintln!("Test functions must be named `fn test_*(...)`.");
        return ExitCode::from(1);
    }

    println!(
        "mink: test: found {} test(s) in '{}'",
        test_names.len(),
        path.display()
    );

    let mut passed = 0u32;
    let mut failed = 0u32;
    let mut failures: Vec<String> = Vec::new();

    // Strip any existing `fn main` from the source so the wrapper can provide its own.
    let source_no_main = strip_main_fn(&source);

    for test_name in &test_names {
        // Build a wrapper source that includes the original source (minus main)
        // and adds a main function calling the test function.
        let wrapper = format!(
            "{}\n\nfn main() {{\n    {}();\n    rt_exit(0);\n}}\n",
            source_no_main, test_name
        );

        let wrapper_path = std::env::temp_dir().join(format!("mink_test_{}.mink", test_name));
        if let Err(e) = std::fs::write(&wrapper_path, &wrapper) {
            eprintln!("mink: error: failed to write temp file: {e}");
            failed += 1;
            failures.push(test_name.clone());
            continue;
        }

        let mut sources = SourceMap::new();
        let options = driver::BuildOptions { target };
        match driver::build(&mut sources, &wrapper_path, options) {
            Ok(outcome) => {
                // Run the test executable.
                let result = std::process::Command::new(&outcome.output).output();
                let _ = std::fs::remove_file(&wrapper_path);
                let _ = std::fs::remove_file(&outcome.output);
                match result {
                    Ok(output) => {
                        let code = output.status.code().unwrap_or(1);
                        if code == 0 {
                            println!("  PASS: {test_name}");
                            passed += 1;
                        } else {
                            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                            let combined = format!("{stdout}{stderr}");
                            // Extract the first meaningful line for the failure message.
                            let msg = combined
                                .lines()
                                .find(|l| !l.is_empty())
                                .unwrap_or("(no output)");
                            println!("  FAIL: {test_name} — {msg}");
                            failed += 1;
                            failures.push(test_name.clone());
                        }
                    }
                    Err(e) => {
                        let _ = std::fs::remove_file(&wrapper_path);
                        eprintln!("  FAIL: {test_name} — failed to execute: {e}");
                        failed += 1;
                        failures.push(test_name.clone());
                    }
                }
            }
            Err(error) => {
                let _ = std::fs::remove_file(&wrapper_path);
                let msg = match error {
                    BuildError::FrontEnd(report) => {
                        // Collect error descriptions.
                        report
                            .errors
                            .iter()
                            .map(|e| format!("{e}"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    }
                    BuildError::Backend(errors) => errors
                        .iter()
                        .map(|e| format!("{e}"))
                        .collect::<Vec<_>>()
                        .join(", "),
                    _ => format!("{error}"),
                };
                println!("  FAIL: {test_name} — compile error: {msg}");
                failed += 1;
                failures.push(test_name.clone());
            }
        }
    }

    println!();
    println!(
        "mink: test result: {} passed, {} failed, {} total",
        passed,
        failed,
        passed + failed
    );

    if !failures.is_empty() {
        println!("Failed tests:");
        for name in &failures {
            println!("  {name}");
        }
    }

    if failed > 0 {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}
