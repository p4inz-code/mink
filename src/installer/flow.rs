//! The steps of an install or a removal, in order, from the first check to the
//! last registry entry. The file work is in `engine`; the machine-level work is
//! behind `Host`.

use std::fs;
use std::path::{Path, PathBuf};

use super::engine::{self, Choices, InstallError, Layout, Progress, Record};
use super::host::{Host, UninstallEntry};
use super::manifest::{Manifest, MANIFEST_NAME};
use crate::setup::Plan;

const SCRIPT_NAME: &str = "register-mink-filetype.ps1";
const PATH_SCRIPT_NAME: &str = "mink-path.ps1";

/// What to install, and where.
#[derive(Debug, Clone)]
pub struct InstallRequest {
    /// The install folder.
    pub root: PathBuf,
    /// What to set up besides the files.
    pub choices: Choices,
    /// Allow running as an administrator (the install then goes to that account).
    pub allow_elevated: bool,
}

/// The result of an install.
#[derive(Debug, Clone)]
pub struct Outcome {
    /// The version that was installed.
    pub version: String,
    /// The record that was written.
    pub record: Record,
    /// Things that did not work but did not stop the install.
    pub warnings: Vec<String>,
    /// Whether an earlier install was upgraded.
    pub upgraded_from: Option<String>,
}

/// The default install folder: `%LOCALAPPDATA%\Programs\MINK`.
pub fn default_root() -> PathBuf {
    let base = std::env::var("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("C:\\Users\\Public\\AppData\\Local"));
    base.join("Programs").join("MINK")
}

/// Reads the manifest of a payload folder.
pub fn read_manifest(payload: &Path) -> Result<Manifest, InstallError> {
    let path = payload.join(MANIFEST_NAME);
    let text = fs::read_to_string(&path)
        .map_err(|e| InstallError::Payload(format!("{}: cannot read ({e})", path.display())))?;
    Manifest::parse(&text).map_err(InstallError::Payload)
}

fn plan_for(choices: &Choices) -> Plan {
    let mut plan = Plan::defaults();
    plan.file_type = choices.file_type;
    plan.context_menu = choices.context_menu;
    plan.new_menu = choices.new_menu;
    plan.preview = choices.preview;
    plan.default_apps = choices.default_apps;
    plan.editor = choices.editor.clone();
    plan
}

/// Installs from `payload` into the folder named by `request`.
pub fn perform_install(
    payload: &Path,
    request: &InstallRequest,
    host: &dyn Host,
    progress: &mut dyn Progress,
) -> Result<Outcome, InstallError> {
    if host.is_elevated() && !request.allow_elevated {
        return Err(InstallError::Elevated);
    }
    let previous = engine::check_folder(&request.root)?;
    let manifest = read_manifest(payload)?;
    let layout = Layout::new(&request.root);
    let upgraded_from = previous.as_ref().map(|r| r.default.clone());

    let mut record = engine::install(payload, &manifest, &layout, &request.choices, progress)?;
    let mut warnings = Vec::new();
    let shim = layout.shim();

    if request.choices.file_type {
        progress.update(93, "Setting up the .mink file type");
        let script = layout.kit_dir().join(SCRIPT_NAME);
        let args = plan_for(&request.choices).script_arguments(&shim, true);
        match host.run_script(&script, &args) {
            Ok((0, _)) => {}
            Ok((_, text)) => warnings.push(format!("The .mink file type was not set up: {}", text.trim())),
            Err(why) => warnings.push(format!("The .mink file type was not set up: {why}")),
        }
    }

    if request.choices.add_to_path {
        progress.update(96, "Adding MINK to your PATH");
        let script = layout.kit_dir().join(PATH_SCRIPT_NAME);
        let args = vec![
            "-Add".to_string(),
            "-Directory".to_string(),
            layout.bin_dir().display().to_string(),
        ];
        match host.run_script(&script, &args) {
            Ok((0, text)) => {
                // An entry that was already there is not ours to remove later, unless an
                // earlier install of ours is what put it there.
                let already = text.contains("already on your PATH");
                record.path_added = !already || previous.as_ref().is_some_and(|r| r.path_added);
            }
            Ok((_, text)) => warnings.push(format!("PATH was not changed: {}", text.trim())),
            Err(why) => warnings.push(format!("PATH was not changed: {why}")),
        }
    } else if let Some(previous) = &previous {
        record.path_added = previous.path_added;
    }

    progress.update(98, "Adding MINK to Apps & features");
    let entry = UninstallEntry {
        version: manifest.version.clone(),
        root: request.root.clone(),
        uninstall_command: format!("\"{}\" installer --uninstall", shim.display()),
        icon: layout.kit_dir().join("mink-app.ico"),
        size_kb: manifest.total_size() / 1024,
    };
    if let Err(why) = host.write_uninstall_entry(&entry) {
        warnings.push(format!("MINK was not listed in Apps & features: {why}"));
    }

    record.write(&layout.record_path())?;
    progress.update(100, "Done");
    Ok(Outcome {
        version: manifest.version,
        record,
        warnings,
        upgraded_from,
    })
}

/// Removes an installed MINK and everything the installer set up.
pub fn perform_uninstall(
    root: &Path,
    host: &dyn Host,
    progress: &mut dyn Progress,
) -> Result<Vec<String>, InstallError> {
    let layout = Layout::new(root);
    let record = Record::read(&layout.record_path())
        .ok_or_else(|| InstallError::Folder(format!("'{}' does not hold a MINK install", root.display())))?;
    let mut warnings = Vec::new();

    progress.update(5, "Removing the .mink file type");
    let script = layout.kit_dir().join(SCRIPT_NAME);
    if script.is_file() {
        let args = vec!["-Uninstall".to_string(), "-Quiet".to_string()];
        match host.run_script(&script, &args) {
            Ok((0, _)) => {}
            Ok((_, text)) => warnings.push(format!("The .mink file type was not removed: {}", text.trim())),
            Err(why) => warnings.push(format!("The .mink file type was not removed: {why}")),
        }
    }
    if record.path_added {
        progress.update(10, "Removing MINK from your PATH");
        let path_script = layout.kit_dir().join(PATH_SCRIPT_NAME);
        if path_script.is_file() {
            let args = vec![
                "-Remove".to_string(),
                "-Directory".to_string(),
                layout.bin_dir().display().to_string(),
            ];
            if let Err(why) = host.run_script(&path_script, &args) {
                warnings.push(format!("PATH was not changed: {why}"));
            }
        }
    }
    progress.update(15, "Removing MINK from Apps & features");
    if let Err(why) = host.remove_uninstall_entry() {
        warnings.push(format!("The Apps & features entry was not removed: {why}"));
    }

    struct Scaled<'a>(&'a mut dyn Progress);
    impl Progress for Scaled<'_> {
        fn update(&mut self, percent: u8, message: &str) {
            self.0.update(15 + (percent as u32 * 85 / 100) as u8, message);
        }
    }
    engine::uninstall(&layout, &mut Scaled(progress))?;
    Ok(warnings)
}
