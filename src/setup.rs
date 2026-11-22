//! `mink setup`: set up how Windows treats `.mink` files, from the command line.
//!
//! The work of writing the per-user registry entries is done by
//! `windows/register-mink-filetype.ps1`, which ships next to the compiler and
//! has its own permanent validator. This module is the front of that script: it
//! asks the questions, turns the answers into the script's switches, runs it and
//! reports what happened. The compiler crate forbids `unsafe` code and has no
//! dependencies, which is why the registry is not touched from Rust directly.
//!
//! Three ways to run it:
//!
//! * `mink setup` asks questions. Numbered lists take answers such as `1 2 3`,
//!   `1-3`, `all` or `none`; yes/no questions take `y` or `n`; Enter takes the
//!   default shown in brackets.
//! * `mink setup --yes` takes every default without asking.
//! * `mink setup --auto` is for the npm install step. It never asks, never takes
//!   `.mink` away from another program, keeps the choices of an earlier setup
//!   and only renews the stored paths.
//!
//! `--status` shows what is set up and `--remove` takes it all away again.

use std::io::{BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const SCRIPT_NAME: &str = "register-mink-filetype.ps1";
const PATH_SCRIPT_NAME: &str = "mink-path.ps1";

/// What the user asked `mink setup` to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    /// Ask questions (the default when run from a terminal).
    Ask,
    /// Take every default without asking.
    Yes,
    /// Quiet, non-interactive: refresh an existing setup, or apply defaults to a new one.
    Auto,
    /// Show what is set up and change nothing.
    Status,
    /// Remove everything `mink setup` added.
    Remove,
}

/// The editor choice given on the command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorArg {
    /// Not given: ask, or use the default.
    Unspecified,
    /// A specific program.
    Path(PathBuf),
    /// Notepad, the safe default.
    Notepad,
}

/// The parsed command line of `mink setup`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetupArgs {
    /// What to do.
    pub mode: Mode,
    /// Take `.mink` over from another program.
    pub force: bool,
    /// Print as little as possible.
    pub quiet: bool,
    /// The editor choice.
    pub editor: EditorArg,
    /// Skip the right-click entries.
    pub no_context_menu: bool,
    /// Skip New > MINK Source File.
    pub no_new_menu: bool,
    /// Skip the preview and search registration.
    pub no_preview: bool,
    /// Skip the Default apps entry.
    pub no_default_apps: bool,
    /// Skip the file type altogether (only PATH is considered).
    pub no_file_type: bool,
    /// `Some(true)` adds MINK to PATH, `Some(false)` never asks, `None` asks.
    pub path: Option<bool>,
}

impl Default for SetupArgs {
    fn default() -> Self {
        Self {
            mode: Mode::Ask,
            force: false,
            quiet: false,
            editor: EditorArg::Unspecified,
            no_context_menu: false,
            no_new_menu: false,
            no_preview: false,
            no_default_apps: false,
            no_file_type: false,
            path: None,
        }
    }
}

/// The usage text of `mink setup`.
pub const USAGE: &str = "\
usage: mink setup [options]

  --yes, -y            take every default without asking
  --auto               quiet and non-interactive: refresh an earlier setup, or apply
                       the defaults to a new one (used by the npm install step)
  --status             show what is set up and change nothing
  --remove             remove everything mink setup added
  --force              take .mink over from another program (undone by --remove)
  --editor <path|notepad>
                       what double-clicking a .mink file opens (to let Windows ask
                       instead, use --no-context-menu)
  --no-file-type       leave .mink file association alone
  --no-context-menu    no Run / Check / Build / Edit entries on right-click
  --no-new-menu        no New > MINK Source File
  --no-preview         no preview pane or search support
  --no-default-apps    do not list MINK under Settings > Default apps
  --path, --no-path    add MINK to your PATH (or never ask about it)
  --quiet, -q          print only errors";

/// Parses the arguments after `mink setup`.
pub fn parse_args(args: &[String]) -> Result<SetupArgs, String> {
    let mut parsed = SetupArgs::default();
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        match arg {
            "--yes" | "-y" => parsed.mode = Mode::Yes,
            "--auto" => parsed.mode = Mode::Auto,
            "--status" => parsed.mode = Mode::Status,
            "--remove" | "--uninstall" => parsed.mode = Mode::Remove,
            "--force" => parsed.force = true,
            "--quiet" | "-q" => parsed.quiet = true,
            "--no-file-type" => parsed.no_file_type = true,
            "--no-context-menu" => parsed.no_context_menu = true,
            "--no-new-menu" => parsed.no_new_menu = true,
            "--no-preview" => parsed.no_preview = true,
            "--no-default-apps" => parsed.no_default_apps = true,
            "--path" => parsed.path = Some(true),
            "--no-path" => parsed.path = Some(false),
            "--editor" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or("missing value after '--editor' (a path, or 'notepad')")?;
                parsed.editor = match value.to_ascii_lowercase().as_str() {
                    "notepad" => EditorArg::Notepad,
                    "windows" | "none" => {
                        return Err("'--editor windows' is not a choice: double-click always opens an editor so it can never run a file. \
                                    Use --no-context-menu to let Windows ask which app to use"
                            .to_string());
                    }
                    _ => EditorArg::Path(PathBuf::from(value)),
                };
            }
            other => return Err(format!("unknown option '{other}' for 'setup'")),
        }
        index += 1;
    }
    Ok(parsed)
}

// ---------------------------------------------------------------------------
// Answers
// ---------------------------------------------------------------------------

/// Reads a selection from a numbered list.
///
/// Accepts numbers separated by spaces or commas (`1 3`, `1,3`), ranges (`2-4`),
/// `all` and `none`. An empty answer returns `defaults`. A number outside
/// `1..=count` is an error that names it.
pub fn parse_selection(input: &str, count: usize, defaults: &[usize]) -> Result<Vec<usize>, String> {
    let text = input.trim().to_ascii_lowercase();
    if text.is_empty() {
        return Ok(defaults.to_vec());
    }
    if text == "all" || text == "a" {
        return Ok((1..=count).collect());
    }
    if text == "none" || text == "n" {
        return Ok(Vec::new());
    }
    let mut chosen: Vec<usize> = Vec::new();
    for piece in text.split(|c: char| c == ',' || c.is_whitespace()).filter(|p| !p.is_empty()) {
        let (first, last) = match piece.split_once('-') {
            Some((a, b)) => (a, b),
            None => (piece, piece),
        };
        let parse = |t: &str| {
            t.parse::<usize>()
                .map_err(|_| format!("'{piece}' is not a number from the list"))
        };
        let (first, last) = (parse(first)?, parse(last)?);
        if first == 0 || last > count || first > last {
            return Err(format!("'{piece}' is outside the list (choose 1 to {count})"));
        }
        for number in first..=last {
            if !chosen.contains(&number) {
                chosen.push(number);
            }
        }
    }
    chosen.sort_unstable();
    Ok(chosen)
}

/// Reads a yes/no answer. `None` means the text is neither.
pub fn parse_yes_no(input: &str, default: bool) -> Option<bool> {
    match input.trim().to_ascii_lowercase().as_str() {
        "" => Some(default),
        "y" | "yes" => Some(true),
        "n" | "no" => Some(false),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Editors
// ---------------------------------------------------------------------------

/// An editor MINK can bind to `.mink` files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Editor {
    /// The name shown in the list.
    pub name: String,
    /// The program to run.
    pub path: PathBuf,
}

/// Finds the editors installed in their usual places.
///
/// `env` reads an environment variable and `exists` tests a file, so tests can
/// supply both. Only programs that are really there are returned.
pub fn detect_editors(env: &dyn Fn(&str) -> Option<String>, exists: &dyn Fn(&Path) -> bool) -> Vec<Editor> {
    let local = env("LOCALAPPDATA").map(PathBuf::from);
    let program_files = env("ProgramFiles").map(PathBuf::from);
    let program_files_x86 = env("ProgramFiles(x86)").map(PathBuf::from);
    let mut candidates: Vec<(&str, PathBuf)> = Vec::new();
    let mut add = |name: &'static str, base: &Option<PathBuf>, tail: &str| {
        if let Some(base) = base {
            candidates.push((name, base.join(tail)));
        }
    };
    add("Visual Studio Code", &local, "Programs\\Microsoft VS Code\\Code.exe");
    add("Visual Studio Code", &program_files, "Microsoft VS Code\\Code.exe");
    add("VS Code Insiders", &local, "Programs\\Microsoft VS Code Insiders\\Code - Insiders.exe");
    add("VSCodium", &local, "Programs\\VSCodium\\VSCodium.exe");
    add("Notepad++", &program_files, "Notepad++\\notepad++.exe");
    add("Notepad++", &program_files_x86, "Notepad++\\notepad++.exe");
    add("Sublime Text", &program_files, "Sublime Text\\sublime_text.exe");
    let mut found: Vec<Editor> = Vec::new();
    for (name, path) in candidates {
        if exists(&path) && !found.iter().any(|e| e.name == name) {
            found.push(Editor {
                name: name.to_string(),
                path,
            });
        }
    }
    found
}

// ---------------------------------------------------------------------------
// The plan
// ---------------------------------------------------------------------------

/// What `mink setup` is going to ask the registration script to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    /// Register the file type at all.
    pub file_type: bool,
    /// Add the right-click entries.
    pub context_menu: bool,
    /// Add New > MINK Source File.
    pub new_menu: bool,
    /// Add the preview and search registration.
    pub preview: bool,
    /// Add the Default apps entry.
    pub default_apps: bool,
    /// The editor to bind, or `None` to use Notepad through the Edit entry.
    pub editor: Option<PathBuf>,
    /// Take `.mink` over from another program.
    pub force: bool,
    /// Add MINK's folder to the user PATH.
    pub add_to_path: bool,
}

impl Plan {
    /// The plan with every default on.
    pub fn defaults() -> Self {
        Self {
            file_type: true,
            context_menu: true,
            new_menu: true,
            preview: true,
            default_apps: true,
            editor: None,
            force: false,
            add_to_path: false,
        }
    }

    /// The switches to hand to `register-mink-filetype.ps1`.
    pub fn script_arguments(&self, mink_exe: &Path, quiet: bool) -> Vec<String> {
        let mut args = vec!["-MinkExe".to_string(), mink_exe.display().to_string()];
        if let Some(editor) = &self.editor {
            args.push("-Editor".to_string());
            args.push(editor.display().to_string());
        }
        if !self.context_menu {
            args.push("-NoContextMenu".to_string());
        }
        if !self.new_menu {
            args.push("-NoNewMenu".to_string());
        }
        if !self.preview {
            args.push("-NoPreview".to_string());
        }
        if !self.default_apps {
            args.push("-NoDefaultApps".to_string());
        }
        if self.force {
            args.push("-Force".to_string());
        }
        if quiet {
            args.push("-Quiet".to_string());
        }
        args
    }
}

// ---------------------------------------------------------------------------
// The outside world
// ---------------------------------------------------------------------------

/// Reading answers and printing text.
pub trait Console {
    /// Prints text without a trailing newline.
    fn print(&mut self, text: &str);
    /// Reads one line, or `None` at end of input.
    fn read_line(&mut self) -> Option<String>;
    /// Whether answers come from a person at a terminal.
    fn is_interactive(&self) -> bool;
}

/// The result of running a script.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptOutput {
    /// The exit code.
    pub code: i32,
    /// Everything the script printed.
    pub text: String,
}

/// Runs PowerShell scripts.
pub trait ScriptRunner {
    /// Runs `script` with `args` and returns its exit code and output.
    fn run(&self, script: &Path, args: &[String]) -> Result<ScriptOutput, String>;
}

/// The facts `setup` needs about the installation.
#[derive(Debug, Clone)]
pub struct Environment {
    /// The running `mink.exe`.
    pub mink_exe: PathBuf,
    /// `register-mink-filetype.ps1`.
    pub script: PathBuf,
    /// `mink-path.ps1`, when present.
    pub path_script: Option<PathBuf>,
    /// Installed editors.
    pub editors: Vec<Editor>,
    /// Whether the folder holding `mink.exe` is already on PATH.
    pub on_path: bool,
}

/// Finds a file of the Windows kit relative to the running executable.
///
/// The npm package and the installer both keep it in `<install>\windows`, next
/// to `<install>\bin\mink.exe`. A development checkout keeps it in
/// `npm\mink\windows`, two folders above `target\debug`.
pub fn find_kit_file(mink_exe: &Path, name: &str) -> Option<PathBuf> {
    let dir = mink_exe.parent()?;
    [
        dir.join("..").join("windows").join(name),
        dir.join("windows").join(name),
        // An installed version, `<install>\versions\<version>\bin\mink.exe`, shares the
        // kit at `<install>\windows`.
        dir.join("..").join("..").join("..").join("windows").join(name),
        dir.join("..").join("..").join("npm").join("mink").join("windows").join(name),
    ]
    .into_iter()
    .find(|candidate| candidate.is_file())
}

/// Whether `dir` is one of the entries of the PATH string.
pub fn path_contains(path_value: &str, dir: &Path) -> bool {
    let wanted = normalize_dir(&dir.display().to_string());
    path_value
        .split(';')
        .map(normalize_dir)
        .any(|entry| !entry.is_empty() && entry == wanted)
}

fn normalize_dir(text: &str) -> String {
    text.trim()
        .trim_matches('"')
        .trim_end_matches(['\\', '/'])
        .replace('/', "\\")
        .to_ascii_lowercase()
}

// ---------------------------------------------------------------------------
// Parsing the script's status
// ---------------------------------------------------------------------------

/// What the registration script reports with `-Status`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Status {
    /// Whether `.mink` currently belongs to MINK.
    pub registered: bool,
    /// The ProgID that owns `.mink`, if any.
    pub owner: String,
    /// The registered icon.
    pub icon: String,
    /// The bound editor, if any.
    pub editor: String,
    /// The double-click verb.
    pub default_verb: String,
    /// The right-click entries present.
    pub verbs: Vec<String>,
    /// Whether the New menu entry is MINK's.
    pub new_menu: bool,
    /// Whether the preview registration is MINK's.
    pub preview: bool,
    /// Whether MINK is listed under Default apps.
    pub default_apps: bool,
}

/// Reads the `name=value` lines of `-Status`.
pub fn parse_status(text: &str) -> Status {
    let mut status = Status::default();
    for line in text.lines() {
        let Some((name, value)) = line.trim().split_once('=') else {
            continue;
        };
        let value = value.trim();
        let flag = value.eq_ignore_ascii_case("true");
        match name {
            "registered" => status.registered = flag,
            "owner" => status.owner = value.to_string(),
            "icon" => status.icon = value.to_string(),
            "editor" => status.editor = value.to_string(),
            "default_verb" => status.default_verb = value.to_string(),
            "verbs" => {
                status.verbs = value
                    .split(',')
                    .filter(|v| !v.is_empty())
                    .map(str::to_string)
                    .collect()
            }
            "new_menu" => status.new_menu = flag,
            "preview" => status.preview = flag,
            "default_apps" => status.default_apps = flag,
            _ => {}
        }
    }
    status
}

fn verb_label(verb: &str) -> &str {
    match verb {
        "run" => "Run",
        "check" => "Check",
        "build" => "Build",
        "edit" => "Edit",
        other => other,
    }
}

fn describe_status(status: &Status) -> String {
    let mut out = String::from("MINK and Windows (this account only)\n");
    let state = if status.registered {
        "set up".to_string()
    } else if status.owner.is_empty() {
        "not set up".to_string()
    } else {
        format!("owned by another program ({})", status.owner)
    };
    out.push_str(&format!("  .mink file type : {state}\n"));
    if status.registered {
        out.push_str(&format!("  icon            : {}\n", status.icon.trim_end_matches(",0").trim_matches('"')));
        let opens = if !status.editor.is_empty() {
            status.editor.clone()
        } else if status.default_verb == "edit" {
            "Notepad".to_string()
        } else {
            "whatever Windows asks you to pick".to_string()
        };
        out.push_str(&format!("  double-click    : opens {opens}\n"));
        let verbs: Vec<&str> = status.verbs.iter().map(|v| verb_label(v)).collect();
        out.push_str(&format!(
            "  right-click     : {}\n",
            if verbs.is_empty() { "no MINK entries".to_string() } else { verbs.join(", ") }
        ));
        let yes_no = |on: bool| if on { "on" } else { "off" };
        out.push_str(&format!("  New menu        : {}\n", yes_no(status.new_menu)));
        out.push_str(&format!("  preview, search : {}\n", yes_no(status.preview)));
        out.push_str(&format!("  Default apps    : {}\n", yes_no(status.default_apps)));
    }
    out
}

// ---------------------------------------------------------------------------
// The run
// ---------------------------------------------------------------------------

/// Exit codes of `mink setup`.
pub mod exit {
    /// Everything asked for was done (or there was nothing to do).
    pub const OK: u8 = 0;
    /// The script reported a failure.
    pub const FAILED: u8 = 1;
    /// The arguments were not understood, or nothing could be asked.
    pub const USAGE: u8 = 2;
    /// This is not Windows, or the Windows kit could not be found.
    pub const UNAVAILABLE: u8 = 3;
}

struct Session<'a> {
    console: &'a mut dyn Console,
    runner: &'a dyn ScriptRunner,
    env: &'a Environment,
    args: &'a SetupArgs,
}

impl Session<'_> {
    fn say(&mut self, text: &str) {
        if !self.args.quiet {
            self.console.print(text);
        }
    }

    /// Asks until the answer parses. `None` means the input ended.
    fn ask<T>(&mut self, prompt: &str, parse: &dyn Fn(&str) -> Result<T, String>) -> Option<T> {
        loop {
            self.console.print(prompt);
            let line = self.console.read_line()?;
            match parse(&line) {
                Ok(value) => return Some(value),
                Err(why) => self.console.print(&format!("  {why}. Try again.\n")),
            }
        }
    }

    fn yes_no(&mut self, prompt: &str, default: bool) -> Option<bool> {
        let hint = if default { "[Y/n]" } else { "[y/N]" };
        self.ask(&format!("{prompt} {hint} "), &|text| {
            parse_yes_no(text, default).ok_or_else(|| "Please answer y or n".to_string())
        })
    }

    fn status(&self) -> Result<Status, String> {
        let output = self.runner.run(&self.env.script, &["-Status".to_string()])?;
        if output.code != 0 {
            return Err(format!("the registration script failed: {}", output.text.trim()));
        }
        Ok(parse_status(&output.text))
    }

    fn apply(&mut self, plan: &Plan) -> Result<(), String> {
        if plan.file_type {
            let args = plan.script_arguments(&self.env.mink_exe, self.args.quiet);
            let output = self.runner.run(&self.env.script, &args)?;
            if !self.args.quiet && !output.text.trim().is_empty() {
                self.console.print(&format!("{}\n", output.text.trim_end()));
            }
            if output.code != 0 {
                if self.args.quiet {
                    self.console.print(&format!("{}\n", output.text.trim_end()));
                }
                return Err("the file type could not be set up".to_string());
            }
        }
        if plan.add_to_path {
            let Some(path_script) = &self.env.path_script else {
                return Err("mink-path.ps1 is missing, so PATH was not changed".to_string());
            };
            let dir = self
                .env
                .mink_exe
                .parent()
                .map(|p| p.display().to_string())
                .unwrap_or_default();
            let output = self
                .runner
                .run(path_script, &["-Add".to_string(), "-Directory".to_string(), dir])?;
            if output.code != 0 {
                return Err(format!("PATH was not changed: {}", output.text.trim()));
            }
            self.say(&format!("{}\n", output.text.trim_end()));
        }
        Ok(())
    }

    // ----------------------------------------------------------- the modes

    fn show_status(&mut self) -> u8 {
        match self.status() {
            Ok(status) => {
                self.console.print(&describe_status(&status));
                let path = if self.env.on_path { "yes" } else { "no" };
                self.console.print(&format!("  on your PATH    : {path}\n"));
                exit::OK
            }
            Err(why) => {
                self.console.print(&format!("mink: error: {why}\n"));
                exit::FAILED
            }
        }
    }

    fn remove(&mut self) -> u8 {
        let args = vec!["-Uninstall".to_string(), "-Quiet".to_string()];
        let mut code = exit::OK;
        match self.runner.run(&self.env.script, &args) {
            Ok(output) if output.code == 0 => self.say("Removed the .mink file type and everything setup added.\n"),
            Ok(output) => {
                self.console
                    .print(&format!("mink: error: removal failed: {}\n", output.text.trim()));
                code = exit::FAILED;
            }
            Err(why) => {
                self.console.print(&format!("mink: error: {why}\n"));
                code = exit::FAILED;
            }
        }
        if let Some(path_script) = &self.env.path_script {
            let dir = self
                .env
                .mink_exe
                .parent()
                .map(|p| p.display().to_string())
                .unwrap_or_default();
            if let Ok(output) = self
                .runner
                .run(path_script, &["-Remove".to_string(), "-Directory".to_string(), dir])
            {
                if output.code == 0 && !output.text.trim().is_empty() {
                    self.say(&format!("{}\n", output.text.trim_end()));
                }
            }
        }
        code
    }

    fn auto(&mut self) -> u8 {
        let status = match self.status() {
            Ok(status) => status,
            Err(_) => return exit::FAILED,
        };
        if status.registered {
            let args = vec!["-Refresh".to_string(), "-Quiet".to_string(), "-MinkExe".to_string(), self.env.mink_exe.display().to_string()];
            return match self.runner.run(&self.env.script, &args) {
                Ok(output) if output.code == 0 => exit::OK,
                _ => exit::FAILED,
            };
        }
        if !status.owner.is_empty() && !self.args.force {
            // Another program owns .mink; an install step must not take it.
            return exit::OK;
        }
        let mut plan = self.plan_from_flags();
        plan.force = self.args.force;
        match self.apply(&plan) {
            Ok(()) => exit::OK,
            Err(_) => exit::FAILED,
        }
    }

    fn plan_from_flags(&self) -> Plan {
        let mut plan = Plan::defaults();
        plan.file_type = !self.args.no_file_type;
        plan.context_menu = !self.args.no_context_menu;
        plan.new_menu = !self.args.no_new_menu;
        plan.preview = !self.args.no_preview;
        plan.default_apps = !self.args.no_default_apps;
        match &self.args.editor {
            EditorArg::Path(path) => plan.editor = Some(path.clone()),
            EditorArg::Notepad | EditorArg::Unspecified => {}
        }
        plan.force = self.args.force;
        plan.add_to_path = self.args.path == Some(true) && !self.env.on_path;
        plan
    }

    fn yes(&mut self) -> u8 {
        let status = self.status().unwrap_or_default();
        if !status.registered && !status.owner.is_empty() && !self.args.force {
            self.console.print(&format!(
                "The .mink file type already belongs to another program ({}). It was left alone.\n\
                 Run `mink setup --yes --force` to take it over; `mink setup --remove` gives it back.\n",
                status.owner
            ));
            return exit::OK;
        }
        let mut plan = self.plan_from_flags();
        if self.args.editor == EditorArg::Unspecified {
            if let Some(editor) = self.env.editors.first() {
                plan.editor = Some(editor.path.clone());
            }
        }
        plan.add_to_path = self.args.path != Some(false) && !self.env.on_path && self.env.path_script.is_some();
        match self.apply(&plan) {
            Ok(()) => {
                self.say("Done. Run `mink setup --status` to review it, or `mink setup --remove` to undo it.\n");
                exit::OK
            }
            Err(why) => {
                self.console.print(&format!("mink: error: {why}\n"));
                exit::FAILED
            }
        }
    }

    fn interactive(&mut self) -> u8 {
        let status = self.status().unwrap_or_default();
        let mut plan = self.plan_from_flags();
        self.console.print(
            "MINK setup\n\
             This sets up how Windows treats .mink files, for your account only. It needs no\n\
             administrator rights, and `mink setup --remove` undoes all of it.\n\n",
        );
        if status.registered {
            self.console.print("MINK is already set up. Answering again updates it.\n\n");
        }

        // 1. What to set up.
        if !self.args.no_file_type {
            let items = [
                "File type and icon for .mink files",
                "Right-click entries: Run, Check, Build, Edit",
                "New > MINK Source File",
                "Preview pane and search for .mink files",
                "MINK under Settings > Apps > Default apps",
            ];
            let defaults: Vec<usize> = vec![
                1,
                if self.args.no_context_menu { 0 } else { 2 },
                if self.args.no_new_menu { 0 } else { 3 },
                if self.args.no_preview { 0 } else { 4 },
                if self.args.no_default_apps { 0 } else { 5 },
            ]
            .into_iter()
            .filter(|n| *n != 0)
            .collect();
            let mut menu = String::from("What do you want set up?\n");
            for (index, item) in items.iter().enumerate() {
                let mark = if defaults.contains(&(index + 1)) { "x" } else { " " };
                menu.push_str(&format!("  [{mark}] {}  {item}\n", index + 1));
            }
            menu.push_str(
                "Without 2, double-click makes Windows ask which app to use. Type numbers like\n\
                 \"1 2 3\", \"all\" or \"none\", or press Enter to keep the marked ones.\n> ",
            );
            let defaults_for_closure = defaults.clone();
            let Some(chosen) = self.ask(&menu, &move |text| parse_selection(text, items.len(), &defaults_for_closure))
            else {
                return self.input_ended();
            };
            plan.file_type = chosen.contains(&1);
            plan.context_menu = chosen.contains(&2);
            plan.new_menu = chosen.contains(&3);
            plan.preview = chosen.contains(&4);
            plan.default_apps = chosen.contains(&5);
            if !plan.file_type && chosen.iter().any(|n| *n != 1) {
                self.console.print(
                    "The other choices need the file type, so they were turned off.\n",
                );
                plan.context_menu = false;
                plan.new_menu = false;
                plan.preview = false;
                plan.default_apps = false;
            }
            self.console.print("\n");
        }

        // 2. The editor for double-click.
        if plan.file_type && plan.context_menu && self.args.editor == EditorArg::Unspecified {
            let mut options: Vec<(String, Option<PathBuf>, bool)> = Vec::new();
            for editor in &self.env.editors {
                options.push((format!("{}  ({})", editor.name, editor.path.display()), Some(editor.path.clone()), false));
            }
            options.push(("Notepad".to_string(), None, false));
            let default_index = 1;
            let mut menu = String::from("What should double-clicking a .mink file open?\n");
            for (index, (label, _, _)) in options.iter().enumerate() {
                menu.push_str(&format!("  {}  {label}\n", index + 1));
            }
            menu.push_str(&format!("Choose a number (Enter = {default_index}): "));
            let count = options.len();
            let Some(picked) = self.ask(&menu, &move |text| {
                let chosen = parse_selection(text, count, &[default_index])?;
                match chosen.as_slice() {
                    [one] => Ok(*one),
                    _ => Err("Choose just one number".to_string()),
                }
            }) else {
                return self.input_ended();
            };
            let (_, path, _) = options[picked - 1].clone();
            plan.editor = path;
            self.console.print("\n");
        }

        // 3. PATH.
        if !self.env.on_path && self.env.path_script.is_some() && self.args.path != Some(false) {
            plan.add_to_path = match self.args.path {
                Some(true) => true,
                _ => {
                    let Some(answer) = self.yes_no(
                        "Add MINK to your PATH, so `mink` works in any new terminal?",
                        true,
                    ) else {
                        return self.input_ended();
                    };
                    answer
                }
            };
            self.console.print("\n");
        }

        // 4. Another program owns .mink.
        if plan.file_type && !status.registered && !status.owner.is_empty() && !plan.force {
            let Some(take) = self.yes_no(
                &format!(
                    "Another program ({}) already owns .mink files. Take them over? You can undo it later.",
                    status.owner
                ),
                false,
            ) else {
                return self.input_ended();
            };
            if take {
                plan.force = true;
            } else {
                self.console.print("Leaving .mink with the other program.\n");
                plan.file_type = false;
                plan.context_menu = false;
                plan.new_menu = false;
                plan.preview = false;
                plan.default_apps = false;
            }
            self.console.print("\n");
        }

        if !plan.file_type && !plan.add_to_path {
            self.console.print("Nothing to do.\n");
            return exit::OK;
        }

        // 5. Confirm.
        let mut summary = String::from("Ready to apply:\n");
        if plan.file_type {
            summary.push_str("  - .mink file type and icon\n");
            if plan.context_menu {
                summary.push_str("  - right-click: Run, Check, Build, Edit\n");
            }
            if plan.new_menu {
                summary.push_str("  - New > MINK Source File\n");
            }
            if plan.preview {
                summary.push_str("  - preview pane and search\n");
            }
            if plan.default_apps {
                summary.push_str("  - Settings > Default apps entry\n");
            }
            match (&plan.editor, plan.context_menu) {
                (Some(path), true) => summary.push_str(&format!("  - double-click opens {}\n", path.display())),
                (None, true) => summary.push_str("  - double-click opens Notepad\n"),
                (_, false) => summary.push_str("  - double-click: Windows asks which app to use\n"),
            }
        }
        if plan.add_to_path {
            summary.push_str("  - add MINK to your PATH\n");
        }
        self.console.print(&summary);
        let Some(go) = self.yes_no("Apply these changes?", true) else {
            return self.input_ended();
        };
        if !go {
            self.console.print("Nothing was changed.\n");
            return exit::OK;
        }
        self.console.print("\n");
        match self.apply(&plan) {
            Ok(()) => {
                self.console.print("\nDone. ");
                if plan.add_to_path {
                    self.console.print("Open a new terminal so the PATH change takes effect. ");
                }
                self.console.print("`mink setup --status` shows what is set up; `mink setup --remove` undoes it.\n");
                exit::OK
            }
            Err(why) => {
                self.console.print(&format!("mink: error: {why}\n"));
                exit::FAILED
            }
        }
    }

    fn input_ended(&mut self) -> u8 {
        self.console
            .print("\nNo more input, so nothing was changed. Run `mink setup --yes` to take the defaults.\n");
        exit::USAGE
    }
}

/// Runs `mink setup` against the given console, script runner and installation.
pub fn run_with(args: &SetupArgs, env: &Environment, console: &mut dyn Console, runner: &dyn ScriptRunner) -> u8 {
    let mut args = args.clone();
    if args.mode == Mode::Auto {
        args.quiet = true;
    }
    let mut session = Session {
        console,
        runner,
        env,
        args: &args,
    };
    match args.mode {
        Mode::Status => session.show_status(),
        Mode::Remove => session.remove(),
        Mode::Auto => session.auto(),
        Mode::Yes => session.yes(),
        Mode::Ask => {
            if session.console.is_interactive() {
                session.interactive()
            } else {
                session.console.print(
                    "mink setup asks questions, but there is no terminal to answer them.\n\
                     Run `mink setup --yes` to take the defaults, or add options such as --editor.\n",
                );
                exit::USAGE
            }
        }
    }
}

// ---------------------------------------------------------------------------
// The real console and PowerShell
// ---------------------------------------------------------------------------

/// The real terminal.
pub struct StdConsole;

impl Console for StdConsole {
    fn print(&mut self, text: &str) {
        print!("{text}");
        let _ = std::io::stdout().flush();
    }

    fn read_line(&mut self) -> Option<String> {
        let mut line = String::new();
        match std::io::stdin().lock().read_line(&mut line) {
            Ok(0) | Err(_) => None,
            Ok(_) => Some(line.trim_end_matches(['\r', '\n']).to_string()),
        }
    }

    fn is_interactive(&self) -> bool {
        std::io::stdin().is_terminal()
    }
}

/// Runs scripts with Windows PowerShell, from its full path.
pub struct PowerShellRunner;

impl PowerShellRunner {
    fn executable() -> PathBuf {
        // The full path, so a program called powershell.exe earlier on PATH is never run.
        let root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_string());
        PathBuf::from(root)
            .join("System32")
            .join("WindowsPowerShell")
            .join("v1.0")
            .join("powershell.exe")
    }
}

impl ScriptRunner for PowerShellRunner {
    fn run(&self, script: &Path, args: &[String]) -> Result<ScriptOutput, String> {
        let output = Command::new(Self::executable())
            .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"])
            .arg(script)
            .args(args)
            .output()
            .map_err(|error| format!("could not start PowerShell: {error}"))?;
        let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
        text.push_str(&String::from_utf8_lossy(&output.stderr));
        Ok(ScriptOutput {
            code: output.status.code().unwrap_or(1),
            text,
        })
    }
}

/// Entry point of `mink setup`, called by the command line.
pub fn main(args: &[String]) -> ExitCode {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    let parsed = match parse_args(args) {
        Ok(parsed) => parsed,
        Err(message) => {
            eprintln!("mink: error: {message}\n{USAGE}");
            return ExitCode::from(exit::USAGE);
        }
    };
    if !cfg!(windows) {
        eprintln!("mink: error: `mink setup` sets up Windows file associations and only works on Windows");
        return ExitCode::from(exit::UNAVAILABLE);
    }
    let Ok(mink_exe) = std::env::current_exe() else {
        eprintln!("mink: error: cannot tell where mink.exe is");
        return ExitCode::from(exit::UNAVAILABLE);
    };
    let Some(script) = find_kit_file(&mink_exe, SCRIPT_NAME) else {
        eprintln!(
            "mink: error: cannot find {SCRIPT_NAME} next to this mink.exe (looked in ..\\windows and windows).\n\
             Reinstall MINK from a package that includes the windows folder."
        );
        return ExitCode::from(exit::UNAVAILABLE);
    };
    let path_script = find_kit_file(&mink_exe, PATH_SCRIPT_NAME);
    let env_var = |name: &str| std::env::var(name).ok();
    let editors = detect_editors(&env_var, &|path| path.is_file());
    let on_path = mink_exe
        .parent()
        .map(|dir| path_contains(&std::env::var("PATH").unwrap_or_default(), dir))
        .unwrap_or(false);
    let environment = Environment {
        mink_exe,
        script,
        path_script,
        editors,
        on_path,
    };
    let mut console = StdConsole;
    let code = run_with(&parsed, &environment, &mut console, &PowerShellRunner);
    ExitCode::from(code)
}

#[cfg(test)]
mod tests;
