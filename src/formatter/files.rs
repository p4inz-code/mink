//! Finding `.mink` files and formatting them on disk.

use std::fs;
use std::path::{Path, PathBuf};

use super::format_source;

/// Folder names that are never searched.
const SKIPPED: &[&str] = &["target", "node_modules", "dist"];

/// What happened to one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileOutcome {
    /// Already in the canonical layout.
    Unchanged,
    /// The canonical layout differs; both texts are returned.
    Changed {
        /// The file as it is.
        old: String,
        /// The file as it should be.
        new: String,
    },
    /// The file could not be read or formatted.
    Failed(String),
}

/// The `.mink` files named by `paths`: a file as given, and every `.mink` file below a folder.
///
/// Hidden folders (those starting with a dot, such as `.git` and `.mink`) and the usual build
/// folders are skipped. The result is sorted and has no duplicates.
pub fn collect_files(paths: &[PathBuf]) -> Result<Vec<PathBuf>, String> {
    let mut found = Vec::new();
    for path in paths {
        let meta = fs::metadata(path).map_err(|e| format!("'{}': {e}", path.display()))?;
        if meta.is_dir() {
            walk(path, &mut found).map_err(|e| format!("'{}': {e}", path.display()))?;
        } else {
            found.push(path.clone());
        }
    }
    found.sort();
    found.dedup();
    Ok(found)
}

fn walk(dir: &Path, found: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if entry.file_type()?.is_dir() {
            if name.starts_with('.') || SKIPPED.contains(&name.as_str()) {
                continue;
            }
            walk(&path, found)?;
        } else if path.extension().is_some_and(|e| e == "mink") {
            found.push(path);
        }
    }
    Ok(())
}

/// Formats the file at `path` and says whether it differs. The file is not written.
pub fn format_file(path: &Path) -> FileOutcome {
    let old = match fs::read(path) {
        Ok(bytes) => match String::from_utf8(bytes) {
            Ok(text) => text,
            Err(_) => return FileOutcome::Failed("the file is not valid UTF-8".to_string()),
        },
        Err(error) => return FileOutcome::Failed(error.to_string()),
    };
    match format_source(&old) {
        Ok(new) if new == old => FileOutcome::Unchanged,
        Ok(new) => FileOutcome::Changed { old, new },
        Err(error) => FileOutcome::Failed(error.to_string()),
    }
}

/// Writes `text` to `path` through a temporary file in the same folder, so an interruption
/// never leaves a half-written source file.
pub fn write_atomically(path: &Path, text: &str) -> std::io::Result<()> {
    let mut temporary = path.as_os_str().to_owned();
    temporary.push(".mink-fmt.tmp");
    let temporary = PathBuf::from(temporary);
    fs::write(&temporary, text)?;
    fs::rename(&temporary, path).inspect_err(|_| {
        let _ = fs::remove_file(&temporary);
    })
}
