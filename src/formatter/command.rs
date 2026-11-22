//! The `mink fmt` command line.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use super::files::{collect_files, format_file, write_atomically, FileOutcome};
use super::{format_source, unified_diff};

/// The usage text of `mink fmt`.
pub const USAGE: &str = "\
usage: mink fmt [options] [path ...]

  Formats MINK source into one canonical layout. It never changes what a program says:
  only white space moves, and it refuses to write a file if its own result does not
  match the input token for token and comment for comment.

  path             a .mink file, or a folder to search (`mink fmt .` formats the current folder)
  --check          write nothing; list files that would change; exit 1 if any do
  --diff           write nothing; show what would change; exit 1 if anything would
  --stdin          read source from standard input and write the result to standard output
  --stdout         write the result for one file to standard output instead of the file
  --quiet, -q      print only errors";

/// The parsed command line.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FmtArgs {
    /// Files and folders.
    pub paths: Vec<PathBuf>,
    /// List files that would change, change nothing.
    pub check: bool,
    /// Show a diff, change nothing.
    pub diff: bool,
    /// Read from standard input.
    pub stdin: bool,
    /// Write one file's result to standard output.
    pub stdout: bool,
    /// Print less.
    pub quiet: bool,
}

/// Parses the arguments after `mink fmt`.
pub fn parse_args(args: &[String]) -> Result<FmtArgs, String> {
    let mut parsed = FmtArgs::default();
    for arg in args {
        match arg.as_str() {
            "--check" => parsed.check = true,
            "--diff" => parsed.diff = true,
            "--stdin" => parsed.stdin = true,
            "--stdout" => parsed.stdout = true,
            "--quiet" | "-q" => parsed.quiet = true,
            other if other.starts_with('-') && other != "-" => {
                return Err(format!("unknown option '{other}' for 'fmt'"));
            }
            other => parsed.paths.push(PathBuf::from(other)),
        }
    }
    if parsed.stdin && !parsed.paths.is_empty() {
        return Err("'--stdin' reads from standard input and takes no paths".to_string());
    }
    if parsed.stdout && (parsed.check || parsed.diff) {
        return Err("'--stdout' cannot be combined with '--check' or '--diff'".to_string());
    }
    Ok(parsed)
}

/// Entry point of `mink fmt`, called by the command line.
pub fn main(args: &[String]) -> ExitCode {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    let parsed = match parse_args(args) {
        Ok(parsed) => parsed,
        Err(message) => {
            eprintln!("mink: error: {message}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    ExitCode::from(run(&parsed))
}

fn run(args: &FmtArgs) -> u8 {
    if args.stdin {
        return run_stdin(args);
    }
    // Formatting rewrites files, so a path must be named: running `mink fmt` by mistake in the
    // wrong folder must not touch everything under it.
    if args.paths.is_empty() {
        eprintln!("mink: error: say what to format, for example `mink fmt .` or `mink fmt src\\main.mink`\n{USAGE}");
        return 2;
    }
    let paths = args.paths.clone();
    let files = match collect_files(&paths) {
        Ok(files) => files,
        Err(why) => {
            eprintln!("mink: error: {why}");
            return 1;
        }
    };
    if files.is_empty() {
        if !args.quiet {
            println!("No .mink files found.");
        }
        return 0;
    }
    if args.stdout && files.len() != 1 {
        eprintln!("mink: error: '--stdout' works on exactly one file, but {} were found", files.len());
        return 2;
    }

    let (mut changed, mut unchanged, mut failed) = (0usize, 0usize, 0usize);
    for file in &files {
        match format_file(file) {
            FileOutcome::Unchanged => {
                unchanged += 1;
                if args.stdout {
                    if let Ok(text) = std::fs::read_to_string(file) {
                        print!("{text}");
                    }
                }
            }
            FileOutcome::Failed(why) => {
                failed += 1;
                eprintln!("mink: error: {}: {why}", file.display());
            }
            FileOutcome::Changed { old, new } => {
                changed += 1;
                if args.stdout {
                    print!("{new}");
                } else if args.diff {
                    print!("{}", unified_diff(&file.display().to_string(), &old, &new));
                } else if args.check {
                    println!("would reformat {}", file.display());
                } else {
                    match write_atomically(file, &new) {
                        Ok(()) => {
                            if !args.quiet {
                                println!("formatted {}", file.display());
                            }
                        }
                        Err(error) => {
                            changed -= 1;
                            failed += 1;
                            eprintln!("mink: error: {}: cannot write ({error})", file.display());
                        }
                    }
                }
            }
        }
    }
    let _ = std::io::stdout().flush();

    if !args.quiet && !args.stdout {
        let verb = if args.check || args.diff { "would change" } else { "formatted" };
        println!("{changed} file(s) {verb}, {unchanged} already formatted{}", if failed > 0 { format!(", {failed} failed") } else { String::new() });
    }
    if failed > 0 || ((args.check || args.diff) && changed > 0) {
        1
    } else {
        0
    }
}

fn run_stdin(args: &FmtArgs) -> u8 {
    let mut text = String::new();
    if let Err(error) = std::io::stdin().read_to_string(&mut text) {
        eprintln!("mink: error: cannot read standard input: {error}");
        return 1;
    }
    match format_source(&text) {
        Ok(formatted) => {
            if args.check {
                return u8::from(formatted != text);
            }
            print!("{formatted}");
            let _ = std::io::stdout().flush();
            0
        }
        Err(error) => {
            eprintln!("mink: error: <stdin>: {error}");
            1
        }
    }
}
