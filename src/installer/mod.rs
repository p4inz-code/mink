//! `mink installer`: installs, upgrades and removes MINK for the current user.
//!
//! This is the program the Windows setup file (`mink-Setup-<version>.exe`)
//! starts. The setup file is a thin wrapper that unpacks a payload folder (this
//! `mink.exe`, the standard library, the Windows kit and a manifest of SHA-256
//! digests) and runs `mink.exe installer` from it, the same way the Kinoz
//! installer starts its own setup window. The window is drawn by the page in
//! `assets/installer/`, served locally and shown in an Edge app window; see
//! [`server`].
//!
//! It can also be run directly:
//!
//! ```text
//! mink installer                       open the setup window
//! mink installer --silent              install with the defaults, no window
//! mink installer --uninstall           remove MINK
//! ```
//!
//! Silent installs take every option as a switch, so a script can decide what is
//! set up. Exit codes: 0 done, 1 failed, 2 bad arguments, 3 running as
//! administrator, 4 MINK is in use, 6 folder not allowed, 7 damaged installer
//! files, 8 cancelled.

pub mod engine;
pub mod flow;
pub mod host;
pub mod json;
pub mod manifest;
pub mod server;

use std::io::Write;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::sync::Arc;
use std::time::Duration;

use engine::{Choices, InstallError, Layout, Progress, Record};
use flow::InstallRequest;
use host::{Host, WindowsHost};
use manifest::{Manifest, MANIFEST_NAME};

use crate::setup::{detect_editors, parse_yes_no, path_contains, Console, StdConsole};

/// The usage text of `mink installer`.
pub const USAGE: &str = "\
usage: mink installer [options]

  (no options)         open the setup window
  --silent             install with no window (every choice has a switch below)
  --uninstall          remove MINK (add --silent for no window)
  --dir <folder>       install folder (default: %LOCALAPPDATA%\\Programs\\MINK)
  --payload <folder>   the folder holding the files to install
  --editor <path|notepad>
                       what double-click opens (default: Notepad)
  --no-association     do not set up the .mink file type
  --no-context-menu    no Run / Check / Build / Edit entries
  --no-new-menu        no New > MINK Source File
  --no-preview         no preview pane or search support
  --no-default-apps    do not list MINK under Default apps
  --no-path            do not add MINK to your PATH
  --log <file>         append what happens to this file
  --no-window          do not open a window; print the address of the setup page
  --make-manifest <folder> --version <x.y.z>
                       write payload-manifest.txt for a payload folder";

/// The parsed command line.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InstallerArgs {
    /// No window.
    pub silent: bool,
    /// Remove instead of install.
    pub uninstall: bool,
    /// The install folder.
    pub dir: Option<PathBuf>,
    /// The folder holding the payload.
    pub payload: Option<PathBuf>,
    /// The editor.
    pub editor: Option<String>,
    /// Skip the file type.
    pub no_association: bool,
    /// Skip right-click entries.
    pub no_context_menu: bool,
    /// Skip the New menu entry.
    pub no_new_menu: bool,
    /// Skip preview and search.
    pub no_preview: bool,
    /// Skip Default apps.
    pub no_default_apps: bool,
    /// Skip PATH.
    pub no_path: bool,
    /// A log file.
    pub log: Option<PathBuf>,
    /// Do not open a window.
    pub no_window: bool,
    /// Fixed port for the page (tests).
    pub port: Option<u16>,
    /// How long the page may stay quiet, in seconds (tests).
    pub idle_seconds: Option<u64>,
    /// Allow running as administrator (tests and CI).
    pub allow_elevated: bool,
    /// Where to remove from, when removing.
    pub root: Option<PathBuf>,
    /// Set when this process is the temporary copy that does the removal.
    pub from_temp: bool,
    /// Write a manifest for this folder and stop.
    pub make_manifest: Option<PathBuf>,
    /// The version for `--make-manifest`.
    pub version: Option<String>,
}

/// Parses the arguments after `mink installer`.
pub fn parse_args(args: &[String]) -> Result<InstallerArgs, String> {
    let mut parsed = InstallerArgs::default();
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        let mut value = |name: &str| -> Result<String, String> {
            index += 1;
            args.get(index)
                .cloned()
                .ok_or(format!("missing value after '{name}'"))
        };
        match arg {
            "--silent" | "/S" | "/s" => parsed.silent = true,
            "--uninstall" => parsed.uninstall = true,
            "--dir" => parsed.dir = Some(PathBuf::from(value("--dir")?)),
            "--payload" => parsed.payload = Some(PathBuf::from(value("--payload")?)),
            "--editor" => parsed.editor = Some(value("--editor")?),
            "--no-association" => parsed.no_association = true,
            "--no-context-menu" => parsed.no_context_menu = true,
            "--no-new-menu" => parsed.no_new_menu = true,
            "--no-preview" => parsed.no_preview = true,
            "--no-default-apps" => parsed.no_default_apps = true,
            "--no-path" => parsed.no_path = true,
            "--log" => parsed.log = Some(PathBuf::from(value("--log")?)),
            "--no-window" => parsed.no_window = true,
            "--port" => {
                parsed.port = Some(value("--port")?.parse().map_err(|_| "'--port' needs a number".to_string())?)
            }
            "--idle-seconds" => {
                parsed.idle_seconds = Some(
                    value("--idle-seconds")?
                        .parse()
                        .map_err(|_| "'--idle-seconds' needs a number".to_string())?,
                )
            }
            "--allow-elevated" => parsed.allow_elevated = true,
            "--root" => parsed.root = Some(PathBuf::from(value("--root")?)),
            "--from-temp" => parsed.from_temp = true,
            "--make-manifest" => parsed.make_manifest = Some(PathBuf::from(value("--make-manifest")?)),
            "--version" => parsed.version = Some(value("--version")?),
            other => return Err(format!("unknown option '{other}' for 'installer'")),
        }
        index += 1;
    }
    Ok(parsed)
}

fn choices_from(args: &InstallerArgs) -> Choices {
    let mut choices = Choices::defaults();
    choices.file_type = !args.no_association;
    choices.context_menu = !args.no_context_menu;
    choices.new_menu = !args.no_new_menu;
    choices.preview = !args.no_preview;
    choices.default_apps = !args.no_default_apps;
    choices.add_to_path = !args.no_path;
    choices.editor = match args.editor.as_deref() {
        None | Some("notepad") => None,
        Some(path) => Some(PathBuf::from(path)),
    };
    choices
}

/// A progress receiver that prints lines and writes a log file.
struct Printer {
    quiet: bool,
    log: Option<std::fs::File>,
    last: u8,
}

impl Progress for Printer {
    fn update(&mut self, percent: u8, message: &str) {
        if let Some(file) = &mut self.log {
            let _ = writeln!(file, "[{percent:>3}%] {message}");
        }
        if !self.quiet && (percent / 10 != self.last / 10 || percent == 100) {
            println!("[{percent:>3}%] {message}");
        }
        self.last = percent;
    }
}

fn open_log(path: &Option<PathBuf>) -> Option<std::fs::File> {
    let path = path.as_ref()?;
    std::fs::OpenOptions::new().create(true).append(true).open(path).ok()
}

fn current_exe() -> Result<PathBuf, String> {
    std::env::current_exe().map_err(|e| format!("cannot tell where mink.exe is: {e}"))
}

/// The folder holding the install this `mink.exe` belongs to, found by walking up
/// until a `mink-install.json` appears.
fn install_root_of(exe: &Path) -> Option<PathBuf> {
    exe.ancestors()
        .skip(1)
        .take(4)
        .find(|dir| dir.join("mink-install.json").is_file())
        .map(Path::to_path_buf)
}

/// Where the payload is: `--payload`, or the folder above `bin\mink.exe` when it has a manifest.
fn find_payload(args: &InstallerArgs, exe: &Path) -> Result<PathBuf, String> {
    if let Some(payload) = &args.payload {
        return Ok(payload.clone());
    }
    let beside = exe.parent().and_then(Path::parent).map(Path::to_path_buf);
    match beside {
        Some(dir) if dir.join(MANIFEST_NAME).is_file() => Ok(dir),
        _ => Err("this mink.exe is not inside an installer payload (no payload-manifest.txt). \
                  Use the mink-Setup file from the MINK release, or pass --payload <folder>"
            .to_string()),
    }
}

fn say(args: &InstallerArgs, text: &str) {
    if !args.silent || args.log.is_none() {
        println!("{text}");
    }
}

/// Entry point of `mink installer`, called by the command line.
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
    if !cfg!(windows) {
        eprintln!("mink: error: the installer only works on Windows");
        return ExitCode::from(1);
    }
    ExitCode::from(run(&parsed, Arc::new(WindowsHost)))
}

/// Runs the installer against `host`. Returns the exit code.
pub fn run(args: &InstallerArgs, host: Arc<dyn Host + Send + Sync>) -> u8 {
    if let (Some(folder), Some(version)) = (&args.make_manifest, &args.version) {
        return match Manifest::from_directory(folder, version) {
            Ok(manifest) => match std::fs::write(folder.join(MANIFEST_NAME), manifest.to_text()) {
                Ok(()) => {
                    println!("wrote {} ({} files)", folder.join(MANIFEST_NAME).display(), manifest.files.len());
                    0
                }
                Err(error) => {
                    eprintln!("mink: error: {error}");
                    1
                }
            },
            Err(why) => {
                eprintln!("mink: error: {why}");
                1
            }
        };
    }
    if args.make_manifest.is_some() {
        eprintln!("mink: error: --make-manifest needs --version <x.y.z>");
        return 2;
    }
    let exe = match current_exe() {
        Ok(exe) => exe,
        Err(why) => {
            eprintln!("mink: error: {why}");
            return 1;
        }
    };
    if args.uninstall {
        run_uninstall(args, &exe, host)
    } else {
        run_install(args, &exe, host)
    }
}

fn run_install(args: &InstallerArgs, exe: &Path, host: Arc<dyn Host + Send + Sync>) -> u8 {
    let payload = match find_payload(args, exe) {
        Ok(payload) => payload,
        Err(why) => {
            eprintln!("mink: error: {why}");
            return 7;
        }
    };
    let manifest = match flow::read_manifest(&payload) {
        Ok(manifest) => manifest,
        Err(error) => {
            eprintln!("mink: error: {error}");
            return error.exit_code();
        }
    };
    let root = args.dir.clone().unwrap_or_else(flow::default_root);

    if args.silent {
        let request = InstallRequest {
            root: root.clone(),
            choices: choices_from(args),
            allow_elevated: args.allow_elevated,
        };
        let mut printer = Printer {
            quiet: false,
            log: open_log(&args.log),
            last: 0,
        };
        return match flow::perform_install(&payload, &request, host.as_ref(), &mut printer) {
            Ok(outcome) => {
                for warning in &outcome.warnings {
                    eprintln!("mink: warning: {warning}");
                }
                say(args, &format!("MINK {} is installed in {}", outcome.version, root.display()));
                0
            }
            Err(error) => {
                eprintln!("mink: error: {error}");
                if let Some(file) = &mut printer.log {
                    let _ = writeln!(file, "error: {error}");
                }
                error.exit_code()
            }
        };
    }

    // The window.
    let env = |name: &str| std::env::var(name).ok();
    let editors = detect_editors(&env, &|p| p.is_file());
    let existing = engine::check_folder(&root).ok().flatten().map(|r| (root.clone(), r));
    let bin = root.join("bin");
    let on_path = path_contains(&std::env::var("PATH").unwrap_or_default(), &bin);
    let config = server::Config {
        token: server::make_token(),
        version: manifest.version.clone(),
        task: server::Task::Install,
        payload_size: manifest.total_size(),
        payload: Some(payload),
        default_root: root,
        existing,
        uninstall_root: None,
        editors,
        on_path,
        allow_elevated: args.allow_elevated,
        idle_limit: Duration::from_secs(args.idle_seconds.unwrap_or(15)),
    };
    run_window(args, config, host)
}

fn run_uninstall(args: &InstallerArgs, exe: &Path, host: Arc<dyn Host + Send + Sync>) -> u8 {
    let root = match args.root.clone().or_else(|| install_root_of(exe)) {
        Some(root) => root,
        None => {
            eprintln!("mink: error: cannot tell which MINK install to remove (no mink-install.json found); use --root <folder>");
            return 6;
        }
    };
    // The removal deletes the folder this mink.exe lives in, so it runs from a temporary copy.
    if !args.from_temp && exe.starts_with(&root) {
        return relaunch_from_temp(args, exe, &root);
    }
    let layout = Layout::new(&root);
    let Some(record) = Record::read(&layout.record_path()) else {
        eprintln!("mink: error: '{}' does not hold a MINK install", root.display());
        return 6;
    };

    if args.silent {
        let mut printer = Printer {
            quiet: false,
            log: open_log(&args.log),
            last: 0,
        };
        let code = match flow::perform_uninstall(&root, host.as_ref(), &mut printer) {
            Ok(warnings) => {
                for warning in &warnings {
                    eprintln!("mink: warning: {warning}");
                }
                say(args, "MINK was removed");
                0
            }
            Err(error) => {
                eprintln!("mink: error: {error}");
                error.exit_code()
            }
        };
        cleanup_temp_copy(args, exe);
        return code;
    }

    let config = server::Config {
        token: server::make_token(),
        version: record.default.clone(),
        task: server::Task::Uninstall,
        payload_size: 0,
        payload: None,
        default_root: root.clone(),
        existing: Some((root.clone(), record)),
        uninstall_root: Some(root),
        editors: Vec::new(),
        on_path: false,
        allow_elevated: args.allow_elevated,
        idle_limit: Duration::from_secs(args.idle_seconds.unwrap_or(15)),
    };
    let code = run_window(args, config, host);
    cleanup_temp_copy(args, exe);
    code
}

/// Copies this `mink.exe` to a temporary folder and runs the removal from there.
fn relaunch_from_temp(args: &InstallerArgs, exe: &Path, root: &Path) -> u8 {
    let temp = std::env::temp_dir().join(format!("mink-uninstall-{}", std::process::id()));
    if std::fs::create_dir_all(&temp).is_err() {
        eprintln!("mink: error: cannot create a temporary folder for the removal");
        return 1;
    }
    let copy = temp.join("mink.exe");
    if let Err(error) = std::fs::copy(exe, &copy) {
        eprintln!("mink: error: cannot copy mink.exe to '{}': {error}", copy.display());
        return 1;
    }
    let mut command = Command::new(&copy);
    command.arg("installer").arg("--uninstall").arg("--root").arg(root).arg("--from-temp");
    if args.silent {
        command.arg("--silent");
    }
    if args.allow_elevated {
        command.arg("--allow-elevated");
    }
    if let Some(log) = &args.log {
        command.arg("--log").arg(log);
    }
    if let Some(port) = args.port {
        command.arg("--port").arg(port.to_string());
    }
    if args.no_window {
        command.arg("--no-window");
    }
    if let Some(idle) = args.idle_seconds {
        command.arg("--idle-seconds").arg(idle.to_string());
    }
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            eprintln!("mink: error: cannot start the removal: {error}");
            return 1;
        }
    };
    if !args.silent {
        // The window is shown by the copy; this process must exit so its own files can be deleted.
        return 0;
    }
    // A silent removal cannot be waited for to the end: the copy has to delete this very mink.exe,
    // so this process has to be gone first. The copy removes the install record before the launcher,
    // so the record disappearing means the removal went through; the copy finishes the last file
    // as soon as this process exits.
    let record = root.join("mink-install.json");
    let started = std::time::Instant::now();
    while started.elapsed() < Duration::from_secs(60) {
        if let Ok(Some(status)) = child.try_wait() {
            return status.code().unwrap_or(1).clamp(0, 255) as u8;
        }
        if !record.exists() {
            return 0;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    eprintln!("mink: error: the removal did not finish in time; see the log for details");
    1
}

/// Deletes the temporary copy (and its folder) after this process has exited.
fn cleanup_temp_copy(args: &InstallerArgs, exe: &Path) {
    if !args.from_temp {
        return;
    }
    let Some(folder) = exe.parent() else { return };
    if !folder
        .file_name()
        .is_some_and(|n| n.to_string_lossy().starts_with("mink-uninstall-"))
    {
        return;
    }
    // A running program cannot delete itself; a hidden shell waits a moment and does it.
    let _ = Command::new("cmd.exe")
        .args(["/c", "ping", "-n", "4", "127.0.0.1", ">nul", "&", "rmdir", "/s", "/q"])
        .arg(folder)
        .spawn();
}

/// Serves the page and, unless told not to, opens it in an app window.
fn run_window(args: &InstallerArgs, config: server::Config, host: Arc<dyn Host + Send + Sync>) -> u8 {
    let listener = match TcpListener::bind(("127.0.0.1", args.port.unwrap_or(0))) {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("mink: error: cannot start the setup page: {error}");
            return 1;
        }
    };
    let port = listener.local_addr().map(|a| a.port()).unwrap_or(0);
    let token = config.token.clone();
    let address = format!("http://127.0.0.1:{port}/?t={token}");
    if args.no_window {
        println!("{address}");
        let _ = std::io::stdout().flush();
    } else {
        let profile = std::env::temp_dir().join(format!("mink-setup-window-{}", std::process::id()));
        if !server::open_window(port, &token, &profile) {
            drop(listener);
            return run_console(args, config, host);
        }
    }
    server::serve(config, host, listener)
}

/// The no-window fallback: a few questions in the terminal.
fn run_console(args: &InstallerArgs, config: server::Config, host: Arc<dyn Host + Send + Sync>) -> u8 {
    let mut console = StdConsole;
    if config.task == server::Task::Uninstall {
        console.print(&format!("Remove MINK {}? [y/N] ", config.version));
        let answer = console.read_line().unwrap_or_default();
        if parse_yes_no(&answer, false) != Some(true) {
            console.print("Nothing was changed.\n");
            return 8;
        }
        let mut printer = Printer {
            quiet: false,
            log: open_log(&args.log),
            last: 0,
        };
        let root = config.uninstall_root.clone().expect("uninstall root");
        return match flow::perform_uninstall(&root, host.as_ref(), &mut printer) {
            Ok(_) => 0,
            Err(error) => {
                eprintln!("mink: error: {error}");
                error.exit_code()
            }
        };
    }
    console.print("A window could not be opened, so setup continues here in the terminal.\n\n");
    let Some(payload) = config.payload.clone() else {
        return 7;
    };
    let ask = |console: &mut StdConsole, question: &str, default: bool| -> Option<bool> {
        loop {
            console.print(&format!("{question} {} ", if default { "[Y/n]" } else { "[y/N]" }));
            let line = console.read_line()?;
            if let Some(answer) = parse_yes_no(&line, default) {
                return Some(answer);
            }
            console.print("  Please answer y or n.\n");
        }
    };
    let root = config.default_root.clone();
    let Some(go) = ask(&mut console, &format!("Install MINK {} to {}?", config.version, root.display()), true) else {
        return 8;
    };
    if !go {
        return 8;
    }
    let mut choices = Choices::defaults();
    let Some(file_type) = ask(&mut console, "Give .mink files the MINK icon and name?", true) else { return 8 };
    choices.file_type = file_type;
    if file_type {
        let Some(v) = ask(&mut console, "Add right-click entries (Run, Check, Build, Edit)?", true) else { return 8 };
        choices.context_menu = v;
        let Some(v) = ask(&mut console, "Add New > MINK Source File?", true) else { return 8 };
        choices.new_menu = v;
        let Some(v) = ask(&mut console, "Add preview pane and search support?", true) else { return 8 };
        choices.preview = v;
        let Some(v) = ask(&mut console, "List MINK under Settings > Default apps?", true) else { return 8 };
        choices.default_apps = v;
        if let Some(editor) = config.editors.first() {
            let Some(v) = ask(&mut console, &format!("Open .mink files in {} on double-click?", editor.name), true) else {
                return 8;
            };
            if v {
                choices.editor = Some(editor.path.clone());
            }
        }
    }
    if !config.on_path {
        let Some(v) = ask(&mut console, "Add MINK to your PATH?", true) else { return 8 };
        choices.add_to_path = v;
    } else {
        choices.add_to_path = false;
    }
    let request = InstallRequest {
        root,
        choices,
        allow_elevated: config.allow_elevated,
    };
    let mut printer = Printer {
        quiet: false,
        log: open_log(&args.log),
        last: 0,
    };
    match flow::perform_install(&payload, &request, host.as_ref(), &mut printer) {
        Ok(outcome) => {
            for warning in &outcome.warnings {
                eprintln!("mink: warning: {warning}");
            }
            println!("\nMINK {} is installed. Open a new terminal and type: mink --version", outcome.version);
            0
        }
        Err(error) => {
            eprintln!("mink: error: {error}");
            error.exit_code()
        }
    }
}

#[cfg(test)]
mod tests;

/// Used by the install flow's tests and by the CLI: `true` when `InstallError` is a refusal that
/// leaves the machine unchanged.
pub fn is_refusal(error: &InstallError) -> bool {
    matches!(error, InstallError::Folder(_) | InstallError::Elevated | InstallError::Payload(_))
}
