//! `mink upgrade`: find out whether a newer MINK exists and, for an installer
//! install, fetch and run it.
//!
//! The network is touched only when the user runs this command; MINK never
//! checks for updates on its own. GitHub is the only host contacted
//! (`api.github.com` for the release list, `github.com` for the downloads), over
//! HTTPS, using the `curl.exe` that ships with Windows 10 and later, because the
//! compiler is dependency-free and the standard library has no TLS.
//!
//! What happens depends on how MINK was installed, which is told from where
//! `mink.exe` sits:
//!
//! * **npm**: npm owns the files, and Windows will not let `npm install` replace
//!   the program that is running it. So `mink upgrade` only says which command to
//!   run in a fresh terminal.
//! * **installer**: the release's `mink-Setup-<version>.exe` is downloaded, its
//!   SHA-256 is compared with the checksum published beside it, and only a match
//!   is started. A mismatch deletes the download and starts nothing.
//! * **anything else** (a portable unzip, a build from source): the download page
//!   is named and nothing is changed.

use std::io::{BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use crate::package::sha256::hex_digest;
use crate::package::version::Version;

const LATEST_URL: &str = "https://api.github.com/repos/p4inz-code/mink/releases/latest";
const TAG_URL: &str = "https://api.github.com/repos/p4inz-code/mink/releases/tags/v";
const RELEASES_PAGE: &str = "https://github.com/p4inz-code/mink/releases";
const NPM_COMMAND: &str = "npm install -g @p4inz-code/mink@latest";

/// The usage text of `mink upgrade`.
pub const USAGE: &str = "\
usage: mink upgrade [--check] [--yes] [--to <version>]

  --check          only say whether a newer version exists
  --yes, -y        do not ask before downloading and starting the installer
  --to <version>   install this version instead of the latest (for example 1.0.5)

MINK contacts the network only when you run this command, and only
api.github.com and github.com.";

/// The parsed command line of `mink upgrade`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UpgradeArgs {
    /// Only report.
    pub check: bool,
    /// Do not ask.
    pub yes: bool,
    /// A specific version instead of the latest.
    pub to: Option<String>,
}

/// Parses the arguments after `mink upgrade`.
pub fn parse_args(args: &[String]) -> Result<UpgradeArgs, String> {
    let mut parsed = UpgradeArgs::default();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--check" => parsed.check = true,
            "--yes" | "-y" => parsed.yes = true,
            "--to" => {
                index += 1;
                let value = args.get(index).ok_or("missing version after '--to'")?;
                let cleaned = value.trim_start_matches('v').to_string();
                Version::parse(&cleaned).map_err(|_| format!("'{value}' is not a version like 1.0.5"))?;
                parsed.to = Some(cleaned);
            }
            other => return Err(format!("unknown option '{other}' for 'upgrade'")),
        }
        index += 1;
    }
    Ok(parsed)
}

// ---------------------------------------------------------------------------
// The release
// ---------------------------------------------------------------------------

/// One downloadable file of a release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asset {
    /// The file name.
    pub name: String,
    /// The download address.
    pub url: String,
}

/// A GitHub release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    /// The version, without the leading `v`.
    pub version: String,
    /// The files attached to it.
    pub assets: Vec<Asset>,
}

/// Every string value that follows `"key":` in `json`, in order.
///
/// GitHub's release JSON is regular enough that this is all the parsing the
/// command needs: it reads `tag_name`, and the `name` / `browser_download_url`
/// pairs of the assets. Escapes inside the strings are decoded.
pub fn json_strings(json: &str, key: &str) -> Vec<String> {
    let needle = format!("\"{key}\"");
    let bytes = json.as_bytes();
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(found) = json[from..].find(&needle) {
        let mut at = from + found + needle.len();
        from = at;
        while at < bytes.len() && (bytes[at] as char).is_whitespace() {
            at += 1;
        }
        if bytes.get(at) != Some(&b':') {
            continue;
        }
        at += 1;
        while at < bytes.len() && (bytes[at] as char).is_whitespace() {
            at += 1;
        }
        if bytes.get(at) != Some(&b'"') {
            continue;
        }
        at += 1;
        let mut text = String::new();
        let mut chars = json[at..].chars();
        while let Some(c) = chars.next() {
            match c {
                '"' => break,
                '\\' => match chars.next() {
                    Some('n') => text.push('\n'),
                    Some('t') => text.push('\t'),
                    Some('u') => {
                        let hex: String = chars.by_ref().take(4).collect();
                        if let Some(c) = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                            text.push(c);
                        }
                    }
                    Some(other) => text.push(other),
                    None => break,
                },
                other => text.push(other),
            }
        }
        out.push(text);
    }
    out
}

/// Reads a release out of the API's JSON.
pub fn parse_release(json: &str) -> Result<Release, String> {
    let tag = json_strings(json, "tag_name")
        .into_iter()
        .next()
        .ok_or("the release answer has no tag_name")?;
    let version = tag.trim_start_matches('v').to_string();
    Version::parse(&version).map_err(|_| format!("the release tag '{tag}' is not a version"))?;
    let names = json_strings(json, "name");
    let urls = json_strings(json, "browser_download_url");
    // Asset objects list `name` before `browser_download_url`; the release's own
    // `name` comes first and has no URL of its own, so pair from the end.
    let skip = names.len().saturating_sub(urls.len());
    let assets = names
        .into_iter()
        .skip(skip)
        .zip(urls)
        .map(|(name, url)| Asset { name, url })
        .collect();
    Ok(Release { version, assets })
}

/// The installer among a release's files.
pub fn find_installer(release: &Release) -> Option<&Asset> {
    release.assets.iter().find(|asset| {
        let lower = asset.name.to_ascii_lowercase();
        lower.ends_with(".exe") && lower.contains("setup")
    })
}

/// The checksum file that belongs to `asset`.
pub fn find_checksum<'a>(release: &'a Release, asset: &Asset) -> Option<&'a Asset> {
    let wanted = format!("{}.sha256", asset.name.to_ascii_lowercase());
    release
        .assets
        .iter()
        .find(|candidate| candidate.name.to_ascii_lowercase() == wanted)
}

/// The expected digest in a checksum file: either the bare hex digest or
/// `<digest>  <file name>`.
pub fn expected_digest(text: &str) -> Option<String> {
    let first = text.split_whitespace().next()?.to_ascii_lowercase();
    (first.len() == 64 && first.chars().all(|c| c.is_ascii_hexdigit())).then_some(first)
}

// ---------------------------------------------------------------------------
// How MINK was installed
// ---------------------------------------------------------------------------

/// How the running `mink.exe` got onto the machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallKind {
    /// Through `npm install -g`.
    Npm,
    /// Through the MINK installer.
    Installer,
    /// Anything else: a portable folder, a source build.
    Other,
}

/// Tells how MINK was installed from where `mink.exe` is.
pub fn install_kind(mink_exe: &Path) -> InstallKind {
    let text = mink_exe.display().to_string().replace('/', "\\").to_ascii_lowercase();
    if text.contains("\\node_modules\\@p4inz-code\\mink\\") {
        return InstallKind::Npm;
    }
    // The installer leaves a record next to the version folder it creates.
    let has_record = mink_exe
        .ancestors()
        .skip(1)
        .take(3)
        .any(|dir| dir.join("mink-install.json").is_file());
    if has_record {
        InstallKind::Installer
    } else {
        InstallKind::Other
    }
}

// ---------------------------------------------------------------------------
// The outside world
// ---------------------------------------------------------------------------

/// Fetching text and files over HTTPS.
pub trait Fetcher {
    /// Reads a small text document.
    fn get_text(&self, url: &str) -> Result<String, String>;
    /// Saves a file.
    fn download(&self, url: &str, to: &Path) -> Result<(), String>;
}

/// Starts the installer once it has been checked.
pub trait Launcher {
    /// Starts `installer` with `args` and returns without waiting.
    fn launch(&self, installer: &Path, args: &[&str]) -> Result<(), String>;
}

/// Reading answers and printing text.
pub trait Console {
    /// Prints text without a trailing newline.
    fn print(&mut self, text: &str);
    /// Reads one line, or `None` at end of input.
    fn read_line(&mut self) -> Option<String>;
    /// Whether a person is at a terminal.
    fn is_interactive(&self) -> bool;
}

/// Where things are for one run.
#[derive(Debug, Clone)]
pub struct Context {
    /// The running version.
    pub current: String,
    /// The running executable.
    pub mink_exe: PathBuf,
    /// A folder for the download.
    pub work_dir: PathBuf,
    /// Overrides the release address (tests and mirrors); `None` uses GitHub.
    pub api_override: Option<String>,
}

/// Exit codes of `mink upgrade`.
pub mod exit {
    /// Done, or already up to date.
    pub const OK: u8 = 0;
    /// Something went wrong.
    pub const FAILED: u8 = 1;
    /// The arguments were not understood.
    pub const USAGE: u8 = 2;
}

fn say(console: &mut dyn Console, text: &str) {
    console.print(text);
}

/// Runs `mink upgrade`.
pub fn run_with(
    args: &UpgradeArgs,
    context: &Context,
    console: &mut dyn Console,
    fetcher: &dyn Fetcher,
    launcher: &dyn Launcher,
) -> u8 {
    let current = match Version::parse(&context.current) {
        Ok(version) => version,
        Err(_) => {
            say(console, &format!("mink: error: this MINK reports '{}', which is not a version\n", context.current));
            return exit::FAILED;
        }
    };
    let url = match (&args.to, &context.api_override) {
        (_, Some(url)) => url.clone(),
        (Some(version), None) => format!("{TAG_URL}{version}"),
        (None, None) => LATEST_URL.to_string(),
    };
    let release = match fetcher.get_text(&url).and_then(|json| parse_release(&json)) {
        Ok(release) => release,
        Err(why) => {
            say(console, &format!("mink: error: could not read the release list: {why}\n"));
            return exit::FAILED;
        }
    };
    let latest = Version::parse(&release.version).expect("parse_release checked it");

    if args.to.is_none() && latest <= current {
        say(console, &format!("MINK {current} is the newest version.\n"));
        return exit::OK;
    }
    if args.to.is_some() && latest == current {
        say(console, &format!("MINK {current} is already installed.\n"));
        return exit::OK;
    }
    say(
        console,
        &format!(
            "MINK {} is available (you have {current}).\n",
            release.version
        ),
    );
    if args.check {
        return exit::OK;
    }

    match install_kind(&context.mink_exe) {
        InstallKind::Npm => {
            say(
                console,
                &format!(
                    "You installed MINK with npm, and Windows will not let npm replace a program\n\
                     while it runs. Close this window, open a new terminal and run:\n\n    {NPM_COMMAND}\n"
                ),
            );
            exit::OK
        }
        InstallKind::Other => {
            say(
                console,
                &format!(
                    "This copy of MINK was not put here by the installer, so it is not changed.\n\
                     Download {} from {RELEASES_PAGE}\n",
                    release.version
                ),
            );
            exit::OK
        }
        InstallKind::Installer => install(args, context, console, fetcher, launcher, &release),
    }
}

fn install(
    args: &UpgradeArgs,
    context: &Context,
    console: &mut dyn Console,
    fetcher: &dyn Fetcher,
    launcher: &dyn Launcher,
    release: &Release,
) -> u8 {
    let Some(installer) = find_installer(release) else {
        say(
            console,
            &format!("mink: error: release {} has no installer to download. See {RELEASES_PAGE}\n", release.version),
        );
        return exit::FAILED;
    };
    let Some(checksum) = find_checksum(release, installer) else {
        // An installer without a published checksum is never started automatically.
        say(
            console,
            &format!(
                "mink: error: release {} has no checksum for {}, so it will not be started.\n\
                 Download it yourself from {RELEASES_PAGE}\n",
                release.version, installer.name
            ),
        );
        return exit::FAILED;
    };
    if !args.yes {
        if !console.is_interactive() {
            say(console, "Run `mink upgrade --yes` to download and start the installer.\n");
            return exit::USAGE;
        }
        say(console, &format!("Download {} and start it? [Y/n] ", installer.name));
        let answer = console.read_line().unwrap_or_default();
        if matches!(answer.trim().to_ascii_lowercase().as_str(), "n" | "no") {
            say(console, "Nothing was changed.\n");
            return exit::OK;
        }
    }

    let expected = match fetcher.get_text(&checksum.url).ok().and_then(|text| expected_digest(&text)) {
        Some(digest) => digest,
        None => {
            say(console, "mink: error: the published checksum could not be read\n");
            return exit::FAILED;
        }
    };
    if let Err(error) = std::fs::create_dir_all(&context.work_dir) {
        say(console, &format!("mink: error: cannot create '{}': {error}\n", context.work_dir.display()));
        return exit::FAILED;
    }
    let target = context.work_dir.join(&installer.name);
    say(console, &format!("Downloading {} ...\n", installer.name));
    if let Err(why) = fetcher.download(&installer.url, &target) {
        let _ = std::fs::remove_file(&target);
        say(console, &format!("mink: error: the download failed: {why}\n"));
        return exit::FAILED;
    }
    let bytes = match std::fs::read(&target) {
        Ok(bytes) => bytes,
        Err(error) => {
            say(console, &format!("mink: error: cannot read the download: {error}\n"));
            return exit::FAILED;
        }
    };
    let actual = hex_digest(&bytes);
    if actual != expected {
        let _ = std::fs::remove_file(&target);
        say(
            console,
            &format!(
                "mink: error: the download does not match its checksum, so it was deleted and not started.\n  \
                 expected {expected}\n  got      {actual}\n"
            ),
        );
        return exit::FAILED;
    }
    say(console, "The checksum matches. Starting the installer; this window can close.\n");
    match launcher.launch(&target, &["--silent"]) {
        Ok(()) => exit::OK,
        Err(why) => {
            say(console, &format!("mink: error: could not start the installer: {why}\n"));
            exit::FAILED
        }
    }
}

// ---------------------------------------------------------------------------
// The real thing
// ---------------------------------------------------------------------------

struct StdConsole;

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

/// Fetches with the `curl.exe` that comes with Windows.
struct CurlFetcher {
    /// Whether `file://` addresses are allowed (only for an overridden release address).
    allow_files: bool,
}

impl CurlFetcher {
    fn curl() -> PathBuf {
        let root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_string());
        PathBuf::from(root).join("System32").join("curl.exe")
    }

    fn command(&self) -> Command {
        let mut command = Command::new(Self::curl());
        command.args(["-fsSL", "--max-time", "120", "-A", "mink-upgrade"]);
        if self.allow_files {
            command.args(["--proto", "=https,file"]);
        } else {
            command.args(["--proto", "=https", "--proto-redir", "=https"]);
        }
        command
    }
}

impl Fetcher for CurlFetcher {
    fn get_text(&self, url: &str) -> Result<String, String> {
        let output = self
            .command()
            .args(["-H", "Accept: application/vnd.github+json"])
            .arg(url)
            .output()
            .map_err(|error| format!("could not start curl.exe: {error}"))?;
        if !output.status.success() {
            return Err(format!(
                "curl.exe reported {}: {}",
                output.status.code().unwrap_or(-1),
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    fn download(&self, url: &str, to: &Path) -> Result<(), String> {
        let output = self
            .command()
            .arg("-o")
            .arg(to)
            .arg(url)
            .output()
            .map_err(|error| format!("could not start curl.exe: {error}"))?;
        if output.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
        }
    }
}

struct DetachedLauncher;

impl Launcher for DetachedLauncher {
    fn launch(&self, installer: &Path, args: &[&str]) -> Result<(), String> {
        Command::new(installer)
            .args(args)
            .spawn()
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
}

/// Entry point of `mink upgrade`, called by the command line.
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
        eprintln!("mink: error: `mink upgrade` only works on Windows");
        return ExitCode::from(exit::FAILED);
    }
    let Ok(mink_exe) = std::env::current_exe() else {
        eprintln!("mink: error: cannot tell where mink.exe is");
        return ExitCode::from(exit::FAILED);
    };
    let api_override = std::env::var("MINK_UPGRADE_API").ok().filter(|v| !v.is_empty());
    let context = Context {
        current: env!("CARGO_PKG_VERSION").to_string(),
        mink_exe,
        work_dir: std::env::temp_dir().join(format!("mink-upgrade-{}", std::process::id())),
        api_override: api_override.clone(),
    };
    let fetcher = CurlFetcher {
        allow_files: api_override.is_some(),
    };
    let mut console = StdConsole;
    ExitCode::from(run_with(&parsed, &context, &mut console, &fetcher, &DetachedLauncher))
}

#[cfg(test)]
mod tests;
