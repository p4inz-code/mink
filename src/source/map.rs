use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::{SourceFile, SourceId};

/// Registry of all source files known to a compilation session.
///
/// Owns every [`SourceFile`] and assigns stable [`SourceId`]s sequentially.
/// Files are never removed, so an id remains valid for the lifetime of the
/// map.
#[derive(Debug, Default)]
pub struct SourceMap {
    files: Vec<SourceFile>,
}

impl SourceMap {
    /// Creates an empty source map.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers an in-memory source file and returns its id.
    pub fn add(&mut self, name: impl Into<PathBuf>, text: impl Into<String>) -> SourceId {
        let id = SourceId::new(self.files.len() as u32);
        self.files.push(SourceFile::new(id, name, text));
        id
    }

    /// Reads the file at `path` from disk and registers it.
    ///
    /// If the path was already loaded, returns its existing id instead of
    /// creating a duplicate entry.
    pub fn load(&mut self, path: &Path) -> io::Result<SourceId> {
        let canon = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        for (i, file) in self.files.iter().enumerate() {
            let existing_canon = file
                .name()
                .canonicalize()
                .unwrap_or_else(|_| file.name().to_path_buf());
            if existing_canon == canon {
                return Ok(SourceId::new(i as u32));
            }
        }
        let text = fs::read_to_string(path)?;
        Ok(self.add(path.to_path_buf(), text))
    }

    /// Returns the file registered under `id`, if any.
    pub fn get(&self, id: SourceId) -> Option<&SourceFile> {
        self.files.get(id.raw() as usize)
    }

    /// Number of registered source files.
    pub fn len(&self) -> usize {
        self.files.len()
    }

    /// Whether no source files are registered.
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }
}
