//! Several versions of MINK side by side.
//!
//! The installer keeps every installed version in its own folder and puts a
//! launcher, a copy of the newest `mink.exe`, at `<install>\bin\mink.exe`, which
//! is the folder on the PATH. When the launcher starts it works out which
//! version is wanted and runs that version's `mink.exe` with the same arguments,
//! so `mink` always means the version you chose for the project, not simply the
//! last one installed.
//!
//! Which version is wanted, in order:
//!
//! 1. the `MINK_VERSION` environment variable;
//! 2. a `.mink-version` file in the current folder or any folder above it
//!    (its first line, such as `1.0.4`);
//! 3. the default version, set by the installer and changed with `mink use`.
//!
//! A `mink.exe` that is not the launcher (an npm install, a portable folder, a
//! version's own `mink.exe`) never dispatches: it is the version it is.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use crate::installer::engine::{self, Layout};

/// The environment variable that names the wanted version.
pub const ENV_VERSION: &str = "MINK_VERSION";
/// The file that names a project's version.
pub const VERSION_FILE: &str = ".mink-version";
/// Set on the version that was started by the launcher, so it never dispatches again.
const ENV_DISPATCHED: &str = "MINK_LAUNCHED";

/// The install this `mink.exe` belongs to: its layout, found by walking up from the
/// executable until a `mink-install.json` appears.
pub fn find_layout(exe: &Path) -> Option<Layout> {
    exe.ancestors()
        .skip(1)
        .take(4)
        .find(|dir| dir.join("mink-install.json").is_file())
        .map(Layout::new)
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

/// Reads a version file's first line, tolerating a leading `v` and a byte-order mark.
pub fn read_version_file(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let line = text.trim_start_matches('\u{feff}').lines().next()?.trim().trim_start_matches('v');
    (!line.is_empty()).then(|| line.to_string())
}

/// The version wanted from `cwd`, with where the choice came from.
pub fn wanted_version(layout: &Layout, cwd: &Path, env_value: Option<&str>) -> Option<(String, String)> {
    if let Some(value) = env_value.map(str::trim).filter(|v| !v.is_empty()) {
        return Some((value.trim_start_matches('v').to_string(), format!("the {ENV_VERSION} variable")));
    }
    for dir in cwd.ancestors() {
        let file = dir.join(VERSION_FILE);
        if let Some(version) = read_version_file(&file) {
            return Some((version, file.display().to_string()));
        }
    }
    engine::default_version(layout).map(|v| (v, "the default version".to_string()))
}

/// When this is the launcher, runs the wanted version and returns its exit code.
///
/// Returns `None` when this `mink.exe` should just run as itself.
pub fn dispatch(args: &[String]) -> Option<ExitCode> {
    if std::env::var_os(ENV_DISPATCHED).is_some() {
        return None;
    }
    // `installer`, `versions` and `use` are about the install itself, so the launcher answers them.
    if matches!(args.first().map(String::as_str), Some("installer" | "versions" | "use")) {
        return None;
    }
    let exe = std::env::current_exe().ok()?;
    let layout = find_layout(&exe)?;
    if !same_file(&exe, &layout.shim()) {
        return None;
    }
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let env_value = std::env::var(ENV_VERSION).ok();
    let Some((version, source)) = wanted_version(&layout, &cwd, env_value.as_deref()) else {
        eprintln!(
            "mink: error: no MINK version is set as the default. Run `mink versions`, then `mink use <version>`."
        );
        return Some(ExitCode::from(1));
    };
    let target = layout.version_dir(&version).join("bin").join("mink.exe");
    if !target.is_file() {
        let installed = engine::list_versions(&layout);
        eprintln!(
            "mink: error: MINK {version} (from {source}) is not installed.\n  installed: {}\n  \
             Install it, or change the choice with `mink use <version>`.",
            if installed.is_empty() { "none".to_string() } else { installed.join(", ") }
        );
        return Some(ExitCode::from(1));
    }
    let status = Command::new(&target)
        .args(args)
        .env(ENV_DISPATCHED, "1")
        .status();
    Some(match status {
        Ok(status) => ExitCode::from(status.code().unwrap_or(1).clamp(0, 255) as u8),
        Err(error) => {
            eprintln!("mink: error: cannot start '{}': {error}", target.display());
            ExitCode::from(1)
        }
    })
}

/// Entry point of `mink versions` and `mink use <version>`.
pub fn main(args: &[String], choose: bool) -> ExitCode {
    let Ok(exe) = std::env::current_exe() else {
        eprintln!("mink: error: cannot tell where mink.exe is");
        return ExitCode::from(1);
    };
    let Some(layout) = find_layout(&exe) else {
        println!(
            "This copy of MINK was not put here by the installer, so there is only one version: {}.",
            env!("CARGO_PKG_VERSION")
        );
        return ExitCode::SUCCESS;
    };
    if choose {
        let [version] = args else {
            eprintln!("mink: error: usage: mink use <version>");
            return ExitCode::from(2);
        };
        let version = version.trim_start_matches('v');
        return match engine::set_default(&layout, version) {
            Ok(()) => {
                println!("MINK {version} is now the default.");
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("mink: error: {error}");
                ExitCode::from(1)
            }
        };
    }
    if let Some(extra) = args.first() {
        eprintln!("mink: error: unexpected argument '{extra}' for 'versions'");
        return ExitCode::from(2);
    }
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    print!("{}", describe(&layout, &cwd, std::env::var(ENV_VERSION).ok().as_deref()));
    ExitCode::SUCCESS
}

/// The text of `mink versions`.
pub fn describe(layout: &Layout, cwd: &Path, env_value: Option<&str>) -> String {
    let installed = engine::list_versions(layout);
    let default = engine::default_version(layout);
    let wanted = wanted_version(layout, cwd, env_value);
    let mut out = String::new();
    if installed.is_empty() {
        out.push_str("No versions are installed.\n");
        return out;
    }
    out.push_str("Installed versions:\n");
    for version in &installed {
        let mut notes = Vec::new();
        if default.as_deref() == Some(version.as_str()) {
            notes.push("default");
        }
        if wanted.as_ref().is_some_and(|(w, _)| w == version) {
            notes.push("runs here");
        }
        let mark = if wanted.as_ref().is_some_and(|(w, _)| w == version) { "*" } else { " " };
        let note = if notes.is_empty() { String::new() } else { format!("  ({})", notes.join(", ")) };
        out.push_str(&format!("  {mark} {version}{note}\n"));
    }
    if let Some((version, source)) = &wanted {
        if !installed.contains(version) {
            out.push_str(&format!("\n{version} is asked for by {source} but is not installed.\n"));
        } else {
            out.push_str(&format!("\nIn this folder, {version} runs (chosen by {source}).\n"));
        }
    }
    out.push_str("Change the default with `mink use <version>`, or pin a project with a .mink-version file.\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn scratch(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mink_versions_{label}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn install(root: &Path, versions: &[&str], default: &str) -> Layout {
        let layout = Layout::new(root);
        for v in versions {
            let bin = layout.version_dir(v).join("bin");
            fs::create_dir_all(&bin).unwrap();
            fs::write(bin.join("mink.exe"), format!("exe {v}")).unwrap();
        }
        fs::write(layout.default_path(), format!("{default}\n")).unwrap();
        fs::write(
            layout.record_path(),
            r#"{"product":"MINK","default":"1.0.5","versions":[],"choices":{},"pathAdded":false}"#,
        )
        .unwrap();
        layout
    }

    #[test]
    fn the_layout_is_found_from_the_launcher_and_from_a_versions_own_exe() {
        let root = scratch("find");
        let layout = install(&root, &["1.0.5"], "1.0.5");
        assert!(find_layout(&layout.shim()).is_some(), "bin\\mink.exe is one level below the root");
        let own = layout.version_dir("1.0.5").join("bin").join("mink.exe");
        assert_eq!(find_layout(&own).unwrap().root, root);
        assert!(find_layout(Path::new("C:\\elsewhere\\mink.exe")).is_none());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn version_files_are_read_leniently() {
        let dir = scratch("file");
        let file = dir.join(VERSION_FILE);
        fs::write(&file, "\u{feff}v1.0.4\r\nignored\r\n").unwrap();
        assert_eq!(read_version_file(&file).as_deref(), Some("1.0.4"));
        fs::write(&file, "   \n").unwrap();
        assert_eq!(read_version_file(&file), None);
        assert_eq!(read_version_file(&dir.join("missing")), None);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn the_environment_beats_a_project_file_which_beats_the_default() {
        let root = scratch("order");
        let layout = install(&root, &["1.0.4", "1.0.5"], "1.0.5");
        let project = root.join("work").join("project").join("src");
        fs::create_dir_all(&project).unwrap();
        // Default only.
        let (v, why) = wanted_version(&layout, &project, None).unwrap();
        assert_eq!((v.as_str(), why.as_str()), ("1.0.5", "the default version"));
        // A file two folders up.
        fs::write(root.join("work").join("project").join(VERSION_FILE), "1.0.4\n").unwrap();
        let (v, why) = wanted_version(&layout, &project, None).unwrap();
        assert_eq!(v, "1.0.4");
        assert!(why.ends_with(VERSION_FILE), "{why}");
        // The environment wins over the file.
        let (v, why) = wanted_version(&layout, &project, Some(" v9.9.9 ")).unwrap();
        assert_eq!(v, "9.9.9");
        assert!(why.contains(ENV_VERSION));
        // An empty variable is ignored.
        assert_eq!(wanted_version(&layout, &project, Some("  ")).unwrap().0, "1.0.4");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn the_listing_marks_the_default_and_the_one_that_runs_here() {
        let root = scratch("list");
        let layout = install(&root, &["1.0.3", "1.0.5", "1.0.4"], "1.0.5");
        let text = describe(&layout, &root, None);
        assert!(text.contains("* 1.0.5  (default, runs here)"), "{text}");
        assert!(text.contains("  1.0.4\n"), "{text}");
        let first = text.find("1.0.5").unwrap();
        let last = text.find("1.0.3").unwrap();
        assert!(first < last, "newest first");
        fs::write(root.join(VERSION_FILE), "1.0.3").unwrap();
        let text = describe(&layout, &root, None);
        assert!(text.contains("* 1.0.3  (runs here)"), "{text}");
        assert!(text.contains("1.0.5  (default)"), "{text}");
        let text = describe(&layout, &root, Some("7.0.0"));
        assert!(text.contains("7.0.0 is asked for by the MINK_VERSION variable but is not installed"), "{text}");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn nothing_installed_is_said_plainly() {
        let root = scratch("none");
        let layout = Layout::new(&root);
        assert_eq!(describe(&layout, &root, None), "No versions are installed.\n");
        let _ = fs::remove_dir_all(root);
    }
}
