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
  repl [path]     Start an interactive compile-eval session (Ctrl-D/:quit exits)
  explain [code]  Explain an error code (e.g., mink explain E-T01);
                  with no code, list all documented error codes
  version         Print the compiler version
  help            Print this help

Package commands (run in a project that has a mink.toml):
  init [path]             Write a starter mink.toml
  add <name> [--version <req> | --path <dir>]
                          Declare a dependency and install it
  remove <name>           Remove a declared dependency and uninstall what is
                          no longer required ('uninstall' is an alias)
  install [path] [--check]
                          Resolve dependencies, install them into the
                          project's packages directory, and write mink.lock
  update [path]           Re-resolve from the manifest, ignoring mink.lock
  env new <name>          Create (and activate) an isolated environment
  env use <name>          Activate an existing environment
  env list                List the project's environments
  env remove <name>       Remove an environment and its packages

Options:
  -h, --help      Print help
  -v, -V, --version
                  Print the compiler version
  --json          Output machine-readable JSON (for check)
  --target <name> Target to compile for (default: the host's native target)
  --version <req> Version requirement (for add), e.g. '^1.0.0'
  --path <dir>    Local package directory (for add)
  --check         Verify the installed packages without changing anything

Examples:
  mink run hello.mink        Compile and run a program
  mink build hello.mink      Compile without running
  mink check hello.mink      Check for errors
  mink test hello.mink       Run test functions in hello.mink
  mink repl                  Start an interactive session
  mink explain E-T01         Explain error E-T01
  mink init                  Start a project in the current directory
  mink add util --version ^1.0.0
  mink install               Install the declared dependencies

Exit codes:
  0   success
  1   usage, input, or compilation error
";

/// A parsed command-line invocation.
enum Command {
    Version,
    Help,
    Build {
        path: PathBuf,
        target: Target,
    },
    Run {
        path: PathBuf,
        target: Target,
    },
    Check {
        path: PathBuf,
        json: bool,
    },
    Test {
        path: PathBuf,
        target: Target,
    },
    Repl {
        path: Option<PathBuf>,
        target: Target,
    },
    Explain {
        code: Option<String>,
    },
    /// `mink init [path]`: write a starter manifest.
    Init {
        path: PathBuf,
        name: String,
        version: String,
    },
    /// `mink install [path] [--check]` and `mink update [path]`.
    PackageInstall {
        path: PathBuf,
        update: bool,
        check: bool,
    },
    /// `mink add <name> [--version <req>] [--path <dir>]`.
    PackageAdd {
        path: PathBuf,
        name: String,
        version: Option<String>,
        dependency_path: Option<PathBuf>,
    },
    /// `mink remove <name>`.
    PackageRemove {
        path: PathBuf,
        name: String,
    },
    /// `mink env <action> [name]`.
    PackageEnv {
        path: PathBuf,
        action: EnvCommand,
        name: Option<String>,
    },
}

/// The `mink env` subcommands, as typed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EnvCommand {
    /// `mink env new <name>`.
    New,
    /// `mink env use <name>`.
    Use,
    /// `mink env list`.
    List,
    /// `mink env remove <name>`.
    Remove,
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
        Ok(Command::Repl { path, target }) => run_repl(path.as_deref(), target),
        Ok(Command::Init {
            path,
            name,
            version,
        }) => run_init_command(&path, &name, &version),
        Ok(Command::PackageInstall {
            path,
            update,
            check,
        }) => run_install_command(&path, update, check),
        Ok(Command::PackageAdd {
            path,
            name,
            version,
            dependency_path,
        }) => run_add_command(&path, &name, version.as_deref(), dependency_path.as_deref()),
        Ok(Command::PackageRemove { path, name }) => run_remove_command(&path, &name),
        Ok(Command::PackageEnv { path, action, name }) => {
            run_env_command(&path, action, name.as_deref())
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
        | BuildError::NotAFile { .. }
        | BuildError::FrontEnd(_)
        | BuildError::Backend(_)
        | BuildError::Output { .. } => ExitCode::from(1),
    }
}

// ======================================================================
// Package commands
// ======================================================================

/// Reports a package-manager failure and returns the failure exit code.
fn package_failure(error: &crate::package::PackageError) -> ExitCode {
    eprintln!("mink: error[{}]: {error}", error.code());
    ExitCode::from(1)
}

/// Prints an install report's package list with a prefix.
fn print_installed(verb: &str, report: &crate::package::InstallReport) {
    for entry in &report.installed {
        println!(
            "  {verb} {} {} ({})",
            entry.name, entry.version, entry.origin
        );
    }
    for name in &report.removed {
        println!("  removed {name}");
    }
    if let Some(lock) = &report.lock {
        if !report.check_only {
            println!("  wrote {}", lock.display());
        }
    }
}

/// `mink init [path]`.
fn run_init_command(path: &PathBuf, name: &str, version: &str) -> ExitCode {
    match crate::package::initialise_project(path, name, version) {
        Ok(written) => {
            println!("mink: init: wrote {}", written.display());
            println!("  package '{name}' {version}");
            ExitCode::SUCCESS
        }
        Err(error) => package_failure(&error),
    }
}

/// `mink install [path] [--check]` and `mink update [path]`.
fn run_install_command(path: &PathBuf, update: bool, check: bool) -> ExitCode {
    let options = crate::package::InstallOptions {
        update,
        check_only: check,
    };
    let verb = if update { "update" } else { "install" };
    match crate::package::install_project(path, options) {
        Ok(report) => {
            println!(
                "mink: {verb}: {} into {}",
                report.summary(),
                report.packages.display()
            );
            print_installed(if check { "verified" } else { "installed" }, &report);
            ExitCode::SUCCESS
        }
        Err(error) => package_failure(&error),
    }
}

/// `mink add <name> [--version <req> | --path <dir>]`.
fn run_add_command(
    path: &PathBuf,
    name: &str,
    version: Option<&str>,
    dependency_path: Option<&std::path::Path>,
) -> ExitCode {
    // `add` edits the manifest at the project root, so accept either the
    // project directory or any file inside it.
    let project = crate::package::find_project_root(path).unwrap_or_else(|| path.clone());
    match crate::package::add_dependency(&project, name, version, dependency_path) {
        Ok(report) => {
            println!("mink: add: declared '{name}'");
            println!(
                "mink: install: {} into {}",
                report.summary(),
                report.packages.display()
            );
            print_installed("installed", &report);
            ExitCode::SUCCESS
        }
        Err(error) => package_failure(&error),
    }
}

/// `mink remove <name>`.
fn run_remove_command(path: &PathBuf, name: &str) -> ExitCode {
    let project = crate::package::find_project_root(path).unwrap_or_else(|| path.clone());
    match crate::package::remove_dependency(&project, name) {
        Ok(report) => {
            println!("mink: remove: undeclared '{name}'");
            print_installed("installed", &report);
            ExitCode::SUCCESS
        }
        Err(error) => package_failure(&error),
    }
}

/// `mink env <new|use|list|remove> [name]`.
fn run_env_command(path: &PathBuf, action: EnvCommand, name: Option<&str>) -> ExitCode {
    use crate::package::env;
    let project = crate::package::find_project_root(path).unwrap_or_else(|| path.clone());
    let result = match action {
        EnvCommand::New => {
            let name = name.expect("checked while parsing");
            env::create(&project, name).map(Some)
        }
        EnvCommand::Use => {
            let name = name.expect("checked while parsing");
            env::activate(&project, name)
                .and_then(|()| env::environment(&project, name))
                .map(Some)
        }
        EnvCommand::Remove => {
            let name = name.expect("checked while parsing");
            env::remove(&project, name).map(Some)
        }
        EnvCommand::List => env::list(&project).map(|list| {
            if list.is_empty() {
                println!("mink: env: no environments in {}", project.display());
            } else {
                for environment in &list {
                    let marker = if environment.active { "*" } else { " " };
                    println!(
                        "mink: env: {marker} {} ({})",
                        environment.name,
                        environment.packages_directory().display()
                    );
                }
            }
            None
        }),
    };
    match result {
        Ok(Some(environment)) => {
            let verb = match action {
                EnvCommand::New => "created",
                EnvCommand::Use => "activated",
                EnvCommand::Remove => "removed",
                EnvCommand::List => "listed",
            };
            println!(
                "mink: env: {verb} '{}' ({})",
                environment.name,
                environment.packages_directory().display()
            );
            ExitCode::SUCCESS
        }
        Ok(None) => ExitCode::SUCCESS,
        Err(error) => {
            let error = crate::package::PackageError::from(error);
            package_failure(&error)
        }
    }
}

/// Parses the arguments of the `build` command: a path plus an optional
/// `--target <name>` (or `--target=<name>`) in either order.
///
/// `command` is the name used in diagnostics. `mink test` shares this
/// grammar, so the message must name the command the user actually typed
/// rather than always saying `build`.
fn parse_build(args: &[String], command: &str) -> Result<Command, String> {
    let mut path: Option<PathBuf> = None;
    let mut target = Target::native();
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        if arg == "--target" {
            index += 1;
            let name = args.get(index).ok_or(format!(
                "missing target name after '--target' (usage: mink {command} <path> [--target <target>])"
            ))?;
            target = parse_target(name)?;
        } else if let Some(name) = arg.strip_prefix("--target=") {
            target = parse_target(name)?;
        } else if arg.starts_with('-') {
            return Err(format!("unknown option '{arg}' for '{command}'"));
        } else {
            if path.is_some() {
                return Err(format!("unexpected argument '{arg}' for '{command}'"));
            }
            path = Some(PathBuf::from(arg));
        }
        index += 1;
    }
    let path = path.ok_or(format!(
        "missing path argument for '{command}' (usage: mink {command} <path> [--target <target>])"
    ))?;
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
        "build" => parse_build(&args[1..], "build"),
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
        "repl" => parse_repl(&args[1..]),
        "test" => parse_build(&args[1..], "test").map(|cmd| match cmd {
            Command::Build { path, target } => Command::Test { path, target },
            _ => unreachable!(),
        }),
        "init" => parse_init(&args[1..]),
        "install" => parse_package_install(&args[1..], false),
        "update" => parse_package_install(&args[1..], true),
        "add" => parse_package_add(&args[1..]),
        "remove" | "uninstall" => {
            let (path, name) = parse_package_remove(&args[1..])?;
            Ok(Command::PackageRemove { path, name })
        }
        "env" => parse_package_env(&args[1..]),
        other => Err(format!("unknown command '{other}'")),
    }
}

/// Parses `mink init [path] [--name <name>]`.
fn parse_init(args: &[String]) -> Result<Command, String> {
    let mut path: Option<PathBuf> = None;
    let mut name: Option<String> = None;
    let mut version: Option<String> = None;
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        if arg == "--name" || arg == "--version" {
            index += 1;
            let value = args
                .get(index)
                .ok_or_else(|| format!("missing value after '{arg}' (usage: mink init [path])"))?
                .clone();
            if arg == "--name" {
                name = Some(value);
            } else {
                version = Some(value);
            }
        } else if arg.starts_with('-') {
            return Err(format!("unknown option '{arg}' for 'init'"));
        } else if path.is_none() {
            path = Some(PathBuf::from(arg));
        } else {
            return Err(format!("unexpected argument '{arg}' for 'init'"));
        }
        index += 1;
    }
    let path = path.unwrap_or_else(|| PathBuf::from("."));
    // The default name is the directory's name, lowercased and sanitised.
    let default_name = path
        .canonicalize()
        .unwrap_or_else(|_| path.clone())
        .file_name()
        .map(|name| name.to_string_lossy().to_lowercase())
        .unwrap_or_else(|| "project".to_string());
    let name = name.unwrap_or(default_name);
    Ok(Command::Init {
        path,
        name,
        version: version.unwrap_or_else(|| "0.1.0".to_string()),
    })
}

/// Parses `mink install [path] [--check]` and `mink update [path]`.
fn parse_package_install(args: &[String], update: bool) -> Result<Command, String> {
    let command = if update { "update" } else { "install" };
    let mut path: Option<PathBuf> = None;
    let mut check = false;
    for arg in args {
        if arg == "--check" {
            if update {
                return Err("unknown option '--check' for 'update'".to_string());
            }
            check = true;
        } else if arg.starts_with('-') {
            return Err(format!("unknown option '{arg}' for '{command}'"));
        } else if path.is_none() {
            path = Some(PathBuf::from(arg));
        } else {
            return Err(format!("unexpected argument '{arg}' for '{command}'"));
        }
    }
    Ok(Command::PackageInstall {
        path: path.unwrap_or_else(|| PathBuf::from(".")),
        update,
        check,
    })
}

/// Parses `mink add <name> [--version <req>] [--path <dir>]`.
fn parse_package_add(args: &[String]) -> Result<Command, String> {
    let mut path = PathBuf::from(".");
    let mut name: Option<String> = None;
    let mut version: Option<String> = None;
    let mut dependency_path: Option<PathBuf> = None;
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        if arg == "--version" || arg == "--path" {
            index += 1;
            let value = args.get(index).ok_or_else(|| {
                format!("missing value after '{arg}' (usage: mink add <name> [--version <req> | --path <dir>])")
            })?;
            if arg == "--version" {
                version = Some(value.clone());
            } else {
                dependency_path = Some(PathBuf::from(value));
            }
        } else if arg == "--project" {
            index += 1;
            let value = args
                .get(index)
                .ok_or("missing value after '--project'")?
                .clone();
            path = PathBuf::from(value);
        } else if arg.starts_with('-') {
            return Err(format!("unknown option '{arg}' for 'add'"));
        } else if name.is_none() {
            name = Some(arg.clone());
        } else {
            return Err(format!("unexpected argument '{arg}' for 'add'"));
        }
        index += 1;
    }
    let name = name.ok_or(
        "missing dependency name (usage: mink add <name> [--version <req> | --path <dir>])",
    )?;
    Ok(Command::PackageAdd {
        path,
        name,
        version,
        dependency_path,
    })
}

/// Parses `mink remove <name>`.
fn parse_package_remove(args: &[String]) -> Result<(PathBuf, String), String> {
    let mut path = PathBuf::from(".");
    let mut name: Option<String> = None;
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        if arg == "--project" {
            index += 1;
            let value = args
                .get(index)
                .ok_or("missing value after '--project'")?
                .clone();
            path = PathBuf::from(value);
        } else if arg.starts_with('-') {
            return Err(format!("unknown option '{arg}' for 'remove'"));
        } else if name.is_none() {
            name = Some(arg.clone());
        } else {
            return Err(format!("unexpected argument '{arg}' for 'remove'"));
        }
        index += 1;
    }
    let name = name.ok_or("missing dependency name (usage: mink remove <name>)")?;
    Ok((path, name))
}

/// Parses `mink env <new|use|list|remove> [name]`.
fn parse_package_env(args: &[String]) -> Result<Command, String> {
    let mut path = PathBuf::from(".");
    let mut action: Option<EnvCommand> = None;
    let mut name: Option<String> = None;
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        if arg == "--project" {
            index += 1;
            let value = args
                .get(index)
                .ok_or("missing value after '--project'")?
                .clone();
            path = PathBuf::from(value);
        } else if arg.starts_with('-') {
            return Err(format!("unknown option '{arg}' for 'env'"));
        } else if action.is_none() {
            action = Some(match arg.as_str() {
                "new" | "create" => EnvCommand::New,
                "use" | "activate" => EnvCommand::Use,
                "list" | "ls" => EnvCommand::List,
                "remove" | "rm" => EnvCommand::Remove,
                other => {
                    return Err(format!(
                        "unknown 'env' action '{other}' (expected new, use, list, remove)"
                    ));
                }
            });
        } else if name.is_none() {
            name = Some(arg.clone());
        } else {
            return Err(format!("unexpected argument '{arg}' for 'env'"));
        }
        index += 1;
    }
    let Some(action) = action else {
        return Err(
            "missing 'env' action (usage: mink env <new|use|list|remove> [name])".to_string(),
        );
    };
    match action {
        EnvCommand::New | EnvCommand::Use | EnvCommand::Remove => {
            if name.is_none() {
                return Err(format!(
                    "missing environment name (usage: mink env {} <name>)",
                    match action {
                        EnvCommand::New => "new",
                        EnvCommand::Use => "use",
                        _ => "remove",
                    }
                ));
            }
        }
        EnvCommand::List => {
            if name.is_some() {
                return Err("unexpected argument for 'env list'".to_string());
            }
        }
    }
    Ok(Command::PackageEnv { path, action, name })
}

/// Parses the arguments of the `repl` command: an optional initial source
/// file plus an optional `--target <name>` (or `--target=<name>`).
fn parse_repl(args: &[String]) -> Result<Command, String> {
    let mut path: Option<PathBuf> = None;
    let mut target = Target::native();
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        if arg == "--target" {
            index += 1;
            let name = args.get(index).ok_or(
                "missing target name after '--target' (usage: mink repl [path] [--target <target>])",
            )?;
            target = parse_target(name)?;
        } else if let Some(name) = arg.strip_prefix("--target=") {
            target = parse_target(name)?;
        } else if arg.starts_with('-') {
            return Err(format!("unknown option '{arg}' for 'repl'"));
        } else if path.is_none() {
            path = Some(PathBuf::from(arg));
        } else {
            return Err(format!("unexpected argument '{arg}' for 'repl'"));
        }
        index += 1;
    }
    Ok(Command::Repl { path, target })
}

/// Returns true when `line` opens a session *declaration* (rather than a
/// runnable statement). Declarations are accumulated into the REPL session;
/// statements are compiled and executed against the accumulated declarations.
fn repl_is_declaration(line: &str) -> bool {
    const PREFIXES: [&str; 14] = [
        "fn ",
        "pub fn ",
        "struct ",
        "pub struct ",
        "enum ",
        "pub enum ",
        "use ",
        "pub use ",
        "mod ",
        "pub mod ",
        "const ",
        "pub const ",
        "//",
        "/*",
    ];
    PREFIXES.iter().any(|prefix| line.starts_with(prefix))
}

/// Returns true when `line` reads as a statement rather than a bare
/// expression (so it must be placed directly in the generated `main`).
fn repl_is_statement(line: &str) -> bool {
    const PREFIXES: [&str; 13] = [
        "if ", "while ", "for ", "loop", "match ", "return", "break", "continue", "let ", "rt_",
        "assert_", ";", "//",
    ];
    PREFIXES.iter().any(|prefix| line.starts_with(prefix))
}

/// Returns true when every `{` in `text` has a matching `}`. Used to keep
/// reading continuation lines for multi-line declarations.
fn repl_braces_balanced(text: &str) -> bool {
    let mut depth = 0i32;
    for ch in text.chars() {
        match ch {
            '{' => depth += 1,
            '}' => depth -= 1,
            _ => {}
        }
    }
    depth == 0
}

/// Renders a build failure the way the CLI renders compile errors, into a
/// string the REPL can print without terminating the session.
fn render_repl_build_error(sources: &SourceMap, error: &BuildError) -> String {
    let mut out = String::new();
    match error {
        BuildError::FrontEnd(report) => {
            let Some(file) = sources.get(report.source_id) else {
                return format!("mink: error: {error}\n");
            };
            for item in &report.errors {
                let line_col = file.line_col(item.span().start());
                out.push_str(&format!(
                    "mink: error[{}]: {}\n  --> {}:{}:{}\n",
                    item.code(),
                    item,
                    file.name().display(),
                    line_col.line,
                    line_col.column
                ));
            }
        }
        BuildError::Backend(errors) => {
            for item in errors {
                out.push_str(&format!("mink: error[{}]: {item}\n", item.code()));
            }
        }
        other => out.push_str(&format!("mink: error: {other}\n")),
    }
    out
}

/// Builds `source` as a native program and runs it, forwarding the child's
/// stdout/stderr. Returns the rendered compile error on failure.
fn repl_build_and_run(
    source: &str,
    target: Target,
    counter: u64,
    attempt: u64,
) -> Result<(), String> {
    let path = std::env::temp_dir().join(format!(
        "mink_repl_{}_{}_{}.mink",
        std::process::id(),
        counter,
        attempt
    ));
    if let Err(error) = std::fs::write(&path, source) {
        return Err(format!(
            "mink: repl: failed to write temp source: {error}\n"
        ));
    }
    let mut sources = SourceMap::new();
    let options = driver::BuildOptions { target };
    let outcome = match driver::build(&mut sources, &path, options) {
        Ok(outcome) => outcome,
        Err(error) => {
            let _ = std::fs::remove_file(&path);
            let _ = std::fs::remove_file(path.with_extension("exe"));
            return Err(render_repl_build_error(&sources, &error));
        }
    };
    let result = std::process::Command::new(&outcome.output).output();
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&outcome.output);
    match result {
        Ok(output) => {
            use std::io::Write;
            let _ = std::io::stdout().write_all(&output.stdout);
            let _ = std::io::stdout().flush();
            let _ = std::io::stderr().write_all(&output.stderr);
            let _ = std::io::stderr().flush();
            Ok(())
        }
        Err(error) => Err(format!("mink: repl: failed to execute: {error}\n")),
    }
}

/// Compiles and runs one REPL input against the session declarations.
///
/// Bare expressions are printed by trying `rt_print_int(expr)` first and then
/// `rt_print_str(expr)`, so an interactive line produces a value without the
/// user writing a print call explicitly.
fn repl_eval(defs: &str, input: &str, target: Target, counter: u64) {
    let candidates: Vec<String> = if input.ends_with(';') || repl_is_statement(input) {
        vec![format!("fn main() {{\n{input}\n}}\n")]
    } else {
        vec![
            format!("fn main() {{\nrt_print_int({input});\n}}\n"),
            format!("fn main() {{\nrt_print_str({input});\n}}\n"),
        ]
    };
    let mut last_error = String::new();
    for (index, body) in candidates.iter().enumerate() {
        let source = format!("{defs}\n{body}");
        match repl_build_and_run(&source, target, counter, index as u64) {
            Ok(()) => return,
            Err(message) => last_error = message,
        }
    }
    eprint!("{last_error}");
}

/// Runs an interactive compile-eval session: declarations accumulate, bare
/// expressions and statements are compiled and executed against them.
fn run_repl(initial: Option<&std::path::Path>, target: Target) -> ExitCode {
    use std::io::{BufRead, Write};

    let mut defs = String::new();
    if let Some(path) = initial {
        match std::fs::read_to_string(path) {
            Ok(source) => {
                defs.push_str(&strip_main_fn(&source));
                defs.push('\n');
            }
            Err(error) => {
                eprintln!("mink: error: failed to read '{}': {error}", path.display());
                return ExitCode::from(1);
            }
        }
    }

    println!("mink {VERSION} interactive (compile-eval). Type :help for help, :quit to exit.");
    let stdin = std::io::stdin();
    let mut lines = stdin.lock().lines();
    let mut counter: u64 = 0;
    loop {
        print!(">>> ");
        let _ = std::io::stdout().flush();
        let Some(Ok(raw)) = lines.next() else {
            println!();
            break;
        };
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(command) = trimmed.strip_prefix(':') {
            match command.trim() {
                "quit" | "q" | "exit" => break,
                "help" | "h" => {
                    println!("Commands:");
                    println!("  :help          Show this help");
                    println!("  :show          Show the accumulated session declarations");
                    println!("  :clear         Clear the accumulated session declarations");
                    println!("  :quit, :q      Exit the session (Ctrl-D / EOF also exits)");
                    println!(
                        "Enter a declaration (fn/struct/enum/use/mod/const) to add it to the session,"
                    );
                    println!("or an expression or statement to compile and run it.");
                }
                "show" => {
                    if defs.trim().is_empty() {
                        println!("(no session declarations)");
                    } else {
                        print!("{defs}");
                    }
                }
                "clear" => {
                    defs.clear();
                    println!("(session cleared)");
                }
                other => eprintln!("mink: repl: unknown command ':{other}'"),
            }
            continue;
        }
        if repl_is_declaration(trimmed) {
            let mut pending = String::from(raw.trim_end());
            pending.push('\n');
            while !repl_braces_balanced(&pending) {
                print!("... ");
                let _ = std::io::stdout().flush();
                match lines.next() {
                    Some(Ok(more)) => {
                        pending.push_str(&more);
                        pending.push('\n');
                    }
                    _ => break,
                }
            }
            defs.push_str(&pending);
            continue;
        }
        counter += 1;
        repl_eval(&defs, trimmed, target, counter);
    }
    ExitCode::SUCCESS
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
