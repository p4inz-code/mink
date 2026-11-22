//! The file work of installing and removing MINK.
//!
//! Layout under the install folder (by default `%LOCALAPPDATA%\Programs\MINK`):
//!
//! ```text
//! bin\mink.exe             the launcher, a copy of the newest mink.exe
//! windows\                 the file-type kit (script, icons, template), refreshed on each install
//! versions\<version>\      one folder per installed version: bin\, stdlib\, LICENSE
//! default-version.txt      which version the launcher runs
//! mink-install.json        what the installer did, so removal can follow it
//! ```
//!
//! Every version sits in its own folder, so several can be installed side by
//! side and an upgrade never overwrites the files of one that is running. A
//! version is staged in `versions\<version>.new` and renamed into place, so an
//! interrupted install leaves the previous one intact.

use std::fs;
use std::path::{Path, PathBuf};

use super::json::{self, Value};
use super::manifest::{Manifest, MANIFEST_NAME};

/// The folder names and files of an install.
#[derive(Debug, Clone)]
pub struct Layout {
    /// The install folder.
    pub root: PathBuf,
}

impl Layout {
    /// A layout rooted at `root`.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// `versions\`.
    pub fn versions_dir(&self) -> PathBuf {
        self.root.join("versions")
    }

    /// `versions\<version>`. The name must pass [`is_version_name`], or the path stays inside
    /// `versions\` only by luck; callers that take a name from outside check it first.
    pub fn version_dir(&self, version: &str) -> PathBuf {
        self.versions_dir().join(version)
    }

    /// `bin\`.
    pub fn bin_dir(&self) -> PathBuf {
        self.root.join("bin")
    }

    /// `bin\mink.exe`, the launcher.
    pub fn shim(&self) -> PathBuf {
        self.bin_dir().join("mink.exe")
    }

    /// `windows\`, the file-type kit.
    pub fn kit_dir(&self) -> PathBuf {
        self.root.join("windows")
    }

    /// `mink-install.json`.
    pub fn record_path(&self) -> PathBuf {
        self.root.join("mink-install.json")
    }

    /// `default-version.txt`.
    pub fn default_path(&self) -> PathBuf {
        self.root.join("default-version.txt")
    }
}

/// Whether `name` is safe to use as a folder name under `versions\`: letters, digits and `.`, `-`,
/// `_`, `+`, no separators, not starting with a dot. A version named by a project file or an
/// environment variable is checked with this before it is turned into a path, so a hostile
/// `.mink-version` cannot point the launcher at a program outside the install.
pub fn is_version_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && !name.starts_with('.')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '+'))
}

/// What the user chose to set up besides the files.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Choices {
    /// Register the `.mink` file type.
    pub file_type: bool,
    /// Add the right-click entries.
    pub context_menu: bool,
    /// Add New > MINK Source File.
    pub new_menu: bool,
    /// Add the preview and search registration.
    pub preview: bool,
    /// Add the Default apps entry.
    pub default_apps: bool,
    /// Add the install folder's `bin` to the user PATH.
    pub add_to_path: bool,
    /// The editor to open `.mink` files in, if any.
    pub editor: Option<PathBuf>,
}

impl Choices {
    /// Everything on, no editor named (double-click opens Notepad).
    pub fn defaults() -> Self {
        Self {
            file_type: true,
            context_menu: true,
            new_menu: true,
            preview: true,
            default_apps: true,
            add_to_path: true,
            editor: None,
        }
    }

    fn to_value(&self) -> Value {
        Value::object(vec![
            ("fileType", Value::Bool(self.file_type)),
            ("contextMenu", Value::Bool(self.context_menu)),
            ("newMenu", Value::Bool(self.new_menu)),
            ("preview", Value::Bool(self.preview)),
            ("defaultApps", Value::Bool(self.default_apps)),
            ("addToPath", Value::Bool(self.add_to_path)),
            (
                "editor",
                self.editor
                    .as_ref()
                    .map(|p| Value::text(p.display().to_string()))
                    .unwrap_or(Value::Null),
            ),
        ])
    }

    fn from_value(value: &Value) -> Self {
        let flag = |key: &str| value.get(key).and_then(Value::as_bool).unwrap_or(false);
        Self {
            file_type: flag("fileType"),
            context_menu: flag("contextMenu"),
            new_menu: flag("newMenu"),
            preview: flag("preview"),
            default_apps: flag("defaultApps"),
            add_to_path: flag("addToPath"),
            editor: value.get("editor").and_then(Value::as_str).map(PathBuf::from),
        }
    }
}

/// What an install did, written to `mink-install.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    /// The default version, the one the launcher runs.
    pub default: String,
    /// Every installed version.
    pub versions: Vec<String>,
    /// The choices made at the last install.
    pub choices: Choices,
    /// Whether the installer added `bin` to the PATH, so removal takes it out again.
    pub path_added: bool,
}

impl Record {
    /// Reads a record, or `None` when the file is missing or damaged.
    pub fn read(path: &Path) -> Option<Self> {
        let value = json::parse(&fs::read_to_string(path).ok()?)?;
        if value.get("product").and_then(Value::as_str) != Some("MINK") {
            return None;
        }
        let versions = match value.get("versions") {
            Some(Value::List(items)) => items.iter().filter_map(|v| v.as_str().map(str::to_string)).collect(),
            _ => Vec::new(),
        };
        Some(Self {
            default: value.get("default")?.as_str()?.to_string(),
            versions,
            choices: value.get("choices").map(Choices::from_value).unwrap_or_default(),
            path_added: value.get("pathAdded").and_then(Value::as_bool).unwrap_or(false),
        })
    }

    /// Writes the record.
    pub fn write(&self, path: &Path) -> Result<(), InstallError> {
        let value = Value::object(vec![
            ("product", Value::text("MINK")),
            ("default", Value::text(&self.default)),
            ("versions", Value::List(self.versions.iter().map(Value::text).collect())),
            ("choices", self.choices.to_value()),
            ("pathAdded", Value::Bool(self.path_added)),
        ]);
        fs::write(path, value.to_json() + "\n").map_err(|e| InstallError::io(path, &e))
    }
}

/// Why an install or removal stopped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallError {
    /// The payload is damaged or incomplete.
    Payload(String),
    /// The chosen folder cannot be used.
    Folder(String),
    /// A file is in use, usually because MINK is running.
    Locked(PathBuf),
    /// Any other file error.
    Io(String),
    /// The installer runs as an administrator, which would install into that account.
    Elevated,
}

impl InstallError {
    fn io(path: &Path, error: &std::io::Error) -> Self {
        InstallError::Io(format!("{}: {error}", path.display()))
    }

    /// The process exit code for this error.
    pub fn exit_code(&self) -> u8 {
        match self {
            InstallError::Io(_) => 1,
            InstallError::Elevated => 3,
            InstallError::Locked(_) => 4,
            InstallError::Folder(_) => 6,
            InstallError::Payload(_) => 7,
        }
    }
}

impl std::fmt::Display for InstallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InstallError::Payload(why) => write!(f, "the installer files are damaged: {why}"),
            InstallError::Folder(why) => write!(f, "{why}"),
            InstallError::Locked(path) => write!(
                f,
                "'{}' is in use. Close every terminal or program that is running MINK and try again",
                path.display()
            ),
            InstallError::Io(why) => write!(f, "{why}"),
            InstallError::Elevated => write!(
                f,
                "this installer is running as an administrator, so MINK would be installed for that account only. \
                 Start it from a normal (non-administrator) window instead"
            ),
        }
    }
}

impl std::error::Error for InstallError {}

/// Receives progress while an install or removal runs.
pub trait Progress {
    /// `percent` is 0 to 100; `message` is one short line for the log.
    fn update(&mut self, percent: u8, message: &str);
}

/// A progress receiver that ignores everything.
pub struct Quiet;

impl Progress for Quiet {
    fn update(&mut self, _percent: u8, _message: &str) {}
}

/// Checks that `dir` can hold an install and returns the record of one that is already there.
///
/// A folder that does not exist, or is empty, is fine. A folder that holds a MINK
/// install is fine (it is upgraded). A folder with anything else in it is
/// refused, so the installer never mixes into, or later deletes, somebody else's files.
pub fn check_folder(dir: &Path) -> Result<Option<Record>, InstallError> {
    if dir.parent().is_none() || dir.as_os_str().is_empty() {
        return Err(InstallError::Folder("choose a folder, not the root of a drive".to_string()));
    }
    if !dir.exists() {
        return Ok(None);
    }
    if !dir.is_dir() {
        return Err(InstallError::Folder(format!("'{}' is a file, not a folder", dir.display())));
    }
    let layout = Layout::new(dir);
    if layout.record_path().is_file() {
        return Ok(Record::read(&layout.record_path()));
    }
    let empty = fs::read_dir(dir)
        .map_err(|e| InstallError::io(dir, &e))?
        .next()
        .is_none();
    if empty {
        Ok(None)
    } else {
        Err(InstallError::Folder(format!(
            "'{}' already holds other files. Choose an empty folder or a new one so nothing of yours is mixed in or removed later",
            dir.display()
        )))
    }
}

/// Where a payload file goes: the shared kit goes to `windows\`, everything else
/// to the version's own folder.
fn destination(layout: &Layout, version: &str, path: &str) -> PathBuf {
    let native = path.replace('/', "\\");
    match native.strip_prefix("windows\\") {
        Some(rest) => layout.kit_dir().join(rest),
        None => layout.version_dir(version).join(native),
    }
}

/// Installs the payload's files and the launcher.
///
/// The payload is verified first. Files are written to staging folders and moved
/// into place, so a failure leaves the previous install as it was.
pub fn install(
    payload: &Path,
    manifest: &Manifest,
    layout: &Layout,
    choices: &Choices,
    progress: &mut dyn Progress,
) -> Result<Record, InstallError> {
    let total = manifest.files.len().max(1);
    progress.update(0, "Checking the installer files");
    manifest
        .verify(payload, &mut |index, path| {
            progress.update((index * 40 / total) as u8, &format!("Checking {path}"))
        })
        .map_err(InstallError::Payload)?;

    let version = &manifest.version;
    let existing = Record::read(&layout.record_path());
    fs::create_dir_all(&layout.root).map_err(|e| InstallError::io(&layout.root, &e))?;
    fs::create_dir_all(layout.versions_dir()).map_err(|e| InstallError::io(&layout.versions_dir(), &e))?;

    let stage = layout.versions_dir().join(format!("{version}.new"));
    let kit_stage = layout.root.join("windows.new");
    for leftover in [&stage, &kit_stage] {
        if leftover.exists() {
            fs::remove_dir_all(leftover).map_err(|e| InstallError::io(leftover, &e))?;
        }
    }

    let result = (|| -> Result<(), InstallError> {
        for (index, file) in manifest.files.iter().enumerate() {
            progress.update(40 + (index * 40 / total) as u8, &format!("Copying {}", file.path));
            let native = file.path.replace('/', "\\");
            let target = match native.strip_prefix("windows\\") {
                Some(rest) => kit_stage.join(rest),
                None => stage.join(&native),
            };
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(|e| InstallError::io(parent, &e))?;
            }
            fs::copy(payload.join(&native), &target).map_err(|e| InstallError::io(&target, &e))?;
            let written = fs::metadata(&target).map_err(|e| InstallError::io(&target, &e))?.len();
            if written != file.size {
                return Err(InstallError::Io(format!(
                    "{}: wrote {written} bytes, expected {}",
                    file.path, file.size
                )));
            }
        }
        // The manifest travels with the version so the files can be checked later.
        fs::write(stage.join(MANIFEST_NAME), manifest.to_text()).map_err(|e| InstallError::io(&stage, &e))?;
        Ok(())
    })();
    if let Err(error) = result {
        let _ = fs::remove_dir_all(&stage);
        let _ = fs::remove_dir_all(&kit_stage);
        return Err(error);
    }

    progress.update(82, "Putting the version in place");
    let final_dir = layout.version_dir(version);
    let backup = layout.versions_dir().join(format!("{version}.old"));
    if backup.exists() {
        let _ = fs::remove_dir_all(&backup);
    }
    let had_previous = final_dir.exists();
    let swap = (|| -> Result<(), InstallError> {
        if had_previous {
            fs::rename(&final_dir, &backup).map_err(|_| InstallError::Locked(final_dir.clone()))?;
        }
        fs::rename(&stage, &final_dir).map_err(|e| InstallError::io(&final_dir, &e))?;
        Ok(())
    })();
    if let Err(error) = swap {
        if had_previous && !final_dir.exists() {
            let _ = fs::rename(&backup, &final_dir);
        }
        let _ = fs::remove_dir_all(&stage);
        let _ = fs::remove_dir_all(&kit_stage);
        return Err(error);
    }

    progress.update(86, "Updating the shared Windows kit");
    let kit = layout.kit_dir();
    let kit_backup = layout.root.join("windows.old");
    if kit_backup.exists() {
        let _ = fs::remove_dir_all(&kit_backup);
    }
    if kit_stage.exists() {
        let kit_moved = !kit.exists() || fs::rename(&kit, &kit_backup).is_ok();
        if !kit_moved || fs::rename(&kit_stage, &kit).is_err() {
            // The kit is shared and small; if it cannot be swapped, put the old one back and
            // fail the whole install so the version is not left half-wired.
            if kit_backup.exists() && !kit.exists() {
                let _ = fs::rename(&kit_backup, &kit);
            }
            let _ = fs::remove_dir_all(&final_dir);
            if had_previous {
                let _ = fs::rename(&backup, &final_dir);
            }
            let _ = fs::remove_dir_all(&kit_stage);
            return Err(InstallError::Locked(kit));
        }
    }

    progress.update(90, "Installing the launcher");
    if let Err(error) = replace_shim(layout, &final_dir.join("bin").join("mink.exe")) {
        let _ = fs::remove_dir_all(&final_dir);
        if had_previous {
            let _ = fs::rename(&backup, &final_dir);
        }
        return Err(error);
    }

    let mut versions = existing.map(|r| r.versions).unwrap_or_default();
    if !versions.contains(version) {
        versions.push(version.clone());
    }
    let record = Record {
        default: version.clone(),
        versions,
        choices: choices.clone(),
        path_added: false,
    };
    fs::write(layout.default_path(), format!("{version}\n")).map_err(|e| InstallError::io(&layout.default_path(), &e))?;
    record.write(&layout.record_path())?;
    let _ = fs::remove_dir_all(&backup);
    let _ = fs::remove_dir_all(layout.root.join("windows.old"));
    progress.update(92, "Files are in place");
    Ok(record)
}

/// Replaces `bin\mink.exe` with a copy of `new_exe`.
///
/// Windows will not overwrite or delete a program that is running, but it will
/// rename it, so the old launcher is moved aside first and the new one renamed
/// into its place. A running terminal keeps working from the moved file.
fn replace_shim(layout: &Layout, new_exe: &Path) -> Result<(), InstallError> {
    let bin = layout.bin_dir();
    fs::create_dir_all(&bin).map_err(|e| InstallError::io(&bin, &e))?;
    let target = layout.shim();
    let staged = bin.join("mink.exe.new");
    let old = bin.join("mink.exe.old");
    fs::copy(new_exe, &staged).map_err(|e| InstallError::io(&staged, &e))?;
    if target.exists() {
        let _ = fs::remove_file(&old);
        fs::rename(&target, &old).map_err(|_| InstallError::Locked(target.clone()))?;
    }
    if let Err(error) = fs::rename(&staged, &target) {
        if old.exists() {
            let _ = fs::rename(&old, &target);
        }
        return Err(InstallError::io(&target, &error));
    }
    // The moved-aside launcher can only be deleted once nothing runs it; leaving it is harmless.
    let _ = fs::remove_file(&old);
    Ok(())
}

/// The installed versions, newest first.
pub fn list_versions(layout: &Layout) -> Vec<String> {
    use crate::package::version::Version;
    let Ok(entries) = fs::read_dir(layout.versions_dir()) else {
        return Vec::new();
    };
    let mut versions: Vec<String> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.path().join("bin").join("mink.exe").is_file())
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    versions.sort_by(|a, b| match (Version::parse(a), Version::parse(b)) {
        (Ok(a), Ok(b)) => b.cmp(&a),
        _ => b.cmp(a),
    });
    versions
}

/// The version the launcher runs: the one in `default-version.txt` if that version is installed.
pub fn default_version(layout: &Layout) -> Option<String> {
    let text = fs::read_to_string(layout.default_path()).ok()?;
    let version = text.trim().to_string();
    if !is_version_name(&version) {
        return None;
    }
    layout
        .version_dir(&version)
        .join("bin")
        .join("mink.exe")
        .is_file()
        .then_some(version)
}

/// Makes `version` the one the launcher runs.
pub fn set_default(layout: &Layout, version: &str) -> Result<(), InstallError> {
    if !is_version_name(version) {
        return Err(InstallError::Folder(format!("'{version}' is not a version name")));
    }
    if !layout.version_dir(version).join("bin").join("mink.exe").is_file() {
        return Err(InstallError::Folder(format!(
            "MINK {version} is not installed (installed: {})",
            list_versions(layout).join(", ")
        )));
    }
    fs::write(layout.default_path(), format!("{version}\n")).map_err(|e| InstallError::io(&layout.default_path(), &e))?;
    if let Some(mut record) = Record::read(&layout.record_path()) {
        record.default = version.to_string();
        record.write(&layout.record_path())?;
    }
    Ok(())
}

/// Removes everything the installer put in the folder, then the folder if it is empty.
///
/// Only the paths of the layout are removed. Anything else that happens to be in
/// the folder is left alone, and so is the folder itself in that case.
pub fn uninstall(layout: &Layout, progress: &mut dyn Progress) -> Result<(), InstallError> {
    progress.update(0, "Removing MINK");
    let versions = list_all_version_dirs(layout);
    let total = versions.len().max(1);
    for (index, dir) in versions.iter().enumerate() {
        progress.update((index * 70 / total) as u8, &format!("Removing {}", dir.display()));
        if dir.exists() {
            fs::remove_dir_all(dir).map_err(|_| InstallError::Locked(dir.clone()))?;
        }
    }
    progress.update(75, "Removing the Windows kit");
    for dir in [layout.kit_dir(), layout.root.join("windows.new"), layout.root.join("windows.old")] {
        if dir.exists() {
            fs::remove_dir_all(&dir).map_err(|_| InstallError::Locked(dir.clone()))?;
        }
    }
    // The record goes before the launcher: it is the sign that an install exists, and the launcher
    // can stay locked for a moment by the very program that started this removal.
    for file in [layout.default_path(), layout.record_path()] {
        if file.exists() {
            fs::remove_file(&file).map_err(|e| InstallError::io(&file, &e))?;
        }
    }
    progress.update(85, "Removing the launcher");
    let bin = layout.bin_dir();
    for name in ["mink.exe", "mink.exe.new", "mink.exe.old"] {
        let file = bin.join(name);
        remove_file_waiting(&file, 60, std::time::Duration::from_millis(250))?;
    }
    for dir in [bin, layout.versions_dir(), layout.root.clone()] {
        // Only an empty folder is removed; anything else in it is not ours.
        let _ = fs::remove_dir(&dir);
    }
    progress.update(100, "MINK was removed");
    Ok(())
}

/// Deletes a file, retrying for a while if Windows says it is in use.
///
/// The launcher is the program that started the removal, and it is still exiting when the removal
/// reaches it, so a short wait is expected rather than an error.
fn remove_file_waiting(file: &Path, attempts: u32, pause: std::time::Duration) -> Result<(), InstallError> {
    for attempt in 0..attempts {
        if !file.exists() {
            return Ok(());
        }
        if fs::remove_file(file).is_ok() {
            return Ok(());
        }
        if attempt + 1 < attempts {
            std::thread::sleep(pause);
        }
    }
    if file.exists() {
        Err(InstallError::Locked(file.to_path_buf()))
    } else {
        Ok(())
    }
}

fn list_all_version_dirs(layout: &Layout) -> Vec<PathBuf> {
    fs::read_dir(layout.versions_dir())
        .map(|entries| entries.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.is_dir()).collect())
        .unwrap_or_default()
}

/// The folder a payload's files for `version` would land in (for tests and messages).
pub fn version_destination(layout: &Layout, version: &str, payload_path: &str) -> PathBuf {
    destination(layout, version, payload_path)
}
