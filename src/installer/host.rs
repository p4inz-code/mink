//! What the installer asks the machine to do, behind a trait so tests can run
//! the whole install flow without touching the registry.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::setup::{PowerShellRunner, ScriptRunner};

/// The Apps & features entry for the installed MINK.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UninstallEntry {
    /// The version shown in the list.
    pub version: String,
    /// The install folder.
    pub root: PathBuf,
    /// The command that removes MINK.
    pub uninstall_command: String,
    /// The icon shown in the list.
    pub icon: PathBuf,
    /// Approximate size in kilobytes.
    pub size_kb: u64,
}

/// Machine-level actions.
pub trait Host {
    /// Runs a PowerShell script and returns its exit code and output.
    fn run_script(&self, script: &Path, args: &[String]) -> Result<(i32, String), String>;
    /// Adds or updates the Apps & features entry.
    fn write_uninstall_entry(&self, entry: &UninstallEntry) -> Result<(), String>;
    /// Removes the Apps & features entry.
    fn remove_uninstall_entry(&self) -> Result<(), String>;
    /// Whether the process runs as an administrator.
    fn is_elevated(&self) -> bool;
}

/// The per-user key of the Apps & features list.
pub const UNINSTALL_KEY: &str = "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\MINK";

/// The real machine.
pub struct WindowsHost;

fn system32(name: &str) -> PathBuf {
    let root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_string());
    PathBuf::from(root).join("System32").join(name)
}

fn reg(args: &[&str]) -> Result<(), String> {
    let output = Command::new(system32("reg.exe"))
        .args(args)
        .output()
        .map_err(|error| format!("could not start reg.exe: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

impl Host for WindowsHost {
    fn run_script(&self, script: &Path, args: &[String]) -> Result<(i32, String), String> {
        let output = PowerShellRunner.run(script, args)?;
        Ok((output.code, output.text))
    }

    fn write_uninstall_entry(&self, entry: &UninstallEntry) -> Result<(), String> {
        let text = |name: &str, value: &str| reg(&["add", UNINSTALL_KEY, "/v", name, "/t", "REG_SZ", "/d", value, "/f"]);
        let number = |name: &str, value: &str| reg(&["add", UNINSTALL_KEY, "/v", name, "/t", "REG_DWORD", "/d", value, "/f"]);
        text("DisplayName", "MINK")?;
        text("DisplayVersion", &entry.version)?;
        text("Publisher", "P4inz Interactive Labs")?;
        text("InstallLocation", &entry.root.display().to_string())?;
        text("DisplayIcon", &entry.icon.display().to_string())?;
        text("UninstallString", &entry.uninstall_command)?;
        text("URLInfoAbout", "https://github.com/p4inz-code/mink")?;
        number("EstimatedSize", &entry.size_kb.to_string())?;
        number("NoModify", "1")?;
        number("NoRepair", "1")?;
        Ok(())
    }

    fn remove_uninstall_entry(&self) -> Result<(), String> {
        // Deleting a key that is not there is not a failure of the removal.
        match reg(&["delete", UNINSTALL_KEY, "/f"]) {
            Ok(()) => Ok(()),
            Err(why) if why.to_ascii_lowercase().contains("unable to find") => Ok(()),
            Err(why) => Err(why),
        }
    }

    fn is_elevated(&self) -> bool {
        // The High Mandatory Level SID is present in the token of an elevated process.
        Command::new(system32("whoami.exe"))
            .arg("/groups")
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).contains("S-1-16-12288"))
            .unwrap_or(false)
    }
}
