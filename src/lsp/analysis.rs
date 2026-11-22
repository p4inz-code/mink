//! Checking a document with the real compiler and indexing every file it touches.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::scan::FileIndex;
use crate::driver::{self, BuildError, CheckError};
use crate::source::{SourceId, SourceMap};

/// A file the compiler loaded while checking a document, with its index.
#[derive(Debug, Clone)]
pub struct AnalyzedFile {
    /// The document address the client uses (or a `file:` address made from the path).
    pub uri: String,
    /// The path on disk.
    pub path: PathBuf,
    /// The text that was checked (the editor's buffer for an open document).
    pub text: Arc<String>,
    /// The token index.
    pub index: Arc<FileIndex>,
}

/// One problem found by the compiler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diag {
    /// Byte range in the file.
    pub range: (usize, usize),
    /// The diagnostic code, such as `E-S01`.
    pub code: String,
    /// The message.
    pub message: String,
    /// A related location, such as the first declaration of a duplicate: (uri, byte range, note).
    pub related: Option<(String, (usize, usize), String)>,
}

/// The result of checking one document.
#[derive(Debug, Clone, Default)]
pub struct Analysis {
    /// Problems grouped by the file they are in. Every analysed file has an entry, empty when clean.
    pub diagnostics: BTreeMap<String, Vec<Diag>>,
    /// Every file the compiler loaded, the checked document first.
    pub files: Vec<AnalyzedFile>,
}

impl Analysis {
    /// The analysed file with this address.
    pub fn file(&self, uri: &str) -> Option<&AnalyzedFile> {
        self.files.iter().find(|f| f.uri == uri)
    }
}

/// Turns a `file:` address into a path. Other schemes (such as `untitled:`) give `None`.
pub fn uri_to_path(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    let rest = rest.strip_prefix("localhost").unwrap_or(rest);
    let decoded = percent_decode(rest)?;
    let mut path = decoded.as_str();
    // `/C:/dir/file` on Windows.
    let bytes = path.as_bytes();
    if bytes.len() >= 3 && bytes[0] == b'/' && bytes[2] == b':' && bytes[1].is_ascii_alphabetic() {
        path = &path[1..];
    }
    Some(PathBuf::from(path.replace('/', "\\")))
}

/// Turns a path into a `file:` address.
pub fn path_to_uri(path: &Path) -> String {
    let text = path.display().to_string().replace('\\', "/");
    let mut out = String::from("file://");
    if !text.starts_with('/') {
        out.push('/');
    }
    for byte in text.bytes() {
        let keep = byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~' | b'/' | b':');
        if keep {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = text.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// Remembers the index of a file's text so unchanged files are not tokenized on every keystroke.
#[derive(Default)]
pub struct IndexCache {
    entries: HashMap<PathBuf, (Arc<String>, Arc<FileIndex>)>,
}

impl IndexCache {
    fn get(&mut self, path: &Path, text: &str) -> (Arc<String>, Arc<FileIndex>) {
        if let Some((cached_text, index)) = self.entries.get(path) {
            if cached_text.as_str() == text {
                return (Arc::clone(cached_text), Arc::clone(index));
            }
        }
        let text = Arc::new(text.to_string());
        let index = Arc::new(FileIndex::build(&text));
        self.entries.insert(path.to_path_buf(), (Arc::clone(&text), Arc::clone(&index)));
        (text, index)
    }
}

/// Checks `root` with the compiler, using `overlays` (path, buffer text) instead of the files on disk.
///
/// `client_uris` maps a path to the exact address the client used for an open document, so
/// diagnostics go back to the address the client knows.
pub fn analyze(
    root: &Path,
    overlays: &[(PathBuf, String)],
    client_uris: &HashMap<PathBuf, String>,
    cache: &mut IndexCache,
) -> Analysis {
    let mut sources = SourceMap::new();
    // A file registered first is found again by the loader, so the buffer wins over the disk copy.
    for (path, text) in overlays {
        sources.add(path.clone(), text.clone());
    }
    let errors: Vec<CheckError> = match driver::check(&mut sources, root) {
        Ok(report) => report.errors,
        Err(BuildError::FrontEnd(report)) => report.errors,
        Err(other) => {
            // The root could not be read at all: say so on the root document.
            let message = other.to_string();
            let mut analysis = Analysis::default();
            let uri = client_uris.get(root).cloned().unwrap_or_else(|| path_to_uri(root));
            analysis.diagnostics.insert(
                uri,
                vec![Diag {
                    range: (0, 0),
                    code: "E-IO".to_string(),
                    message,
                    related: None,
                }],
            );
            return analysis;
        }
    };

    let uri_of = |path: &Path| -> String {
        client_uris
            .get(path)
            .cloned()
            .or_else(|| {
                // Match a client path that differs only by case or by `\\?\` prefixes.
                let wanted = normalize(path);
                client_uris.iter().find(|(p, _)| normalize(p) == wanted).map(|(_, u)| u.clone())
            })
            .unwrap_or_else(|| path_to_uri(path))
    };

    let mut analysis = Analysis::default();
    // The checked document first, then the rest in load order.
    let mut order: Vec<usize> = (0..sources.len()).collect();
    if let Some(root_position) = order
        .iter()
        .position(|&i| sources.get(SourceId::new(i as u32)).is_some_and(|f| normalize(f.name()) == normalize(root)))
    {
        let first = order.remove(root_position);
        order.insert(0, first);
    }
    for i in order {
        let Some(file) = sources.get(SourceId::new(i as u32)) else { continue };
        let path = file.name().to_path_buf();
        let (text, index) = cache.get(&path, file.text());
        let uri = uri_of(&path);
        analysis.diagnostics.entry(uri.clone()).or_default();
        analysis.files.push(AnalyzedFile { uri, path, text, index });
    }

    for error in &errors {
        let span = error.span();
        let Some(file) = sources.get(span.file()) else { continue };
        let uri = uri_of(file.name());
        let related = error.related_span().and_then(|related| {
            let other = sources.get(related.file())?;
            Some((
                uri_of(other.name()),
                (related.start() as usize, related.end() as usize),
                "related location".to_string(),
            ))
        });
        let diag = Diag {
            range: (span.start() as usize, span.end().max(span.start()) as usize),
            code: error.code().to_string(),
            message: error.to_string(),
            related: related.map(|(u, r, _)| (u, r, "see also".to_string())),
        };
        analysis.diagnostics.entry(uri).or_default().push(diag);
    }
    for list in analysis.diagnostics.values_mut() {
        list.sort_by_key(|d| (d.range.0, d.range.1));
    }
    analysis
}

fn normalize(path: &Path) -> String {
    path.canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .display()
        .to_string()
        .trim_start_matches("\\\\?\\")
        .replace('/', "\\")
        .to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_addresses_round_trip_on_windows_paths() {
        let path = PathBuf::from("C:\\Users\\Ana Maria\\my proj\\main.mink");
        let uri = path_to_uri(&path);
        assert_eq!(uri, "file:///C:/Users/Ana%20Maria/my%20proj/main.mink");
        assert_eq!(uri_to_path(&uri), Some(path));
    }

    #[test]
    fn addresses_from_editors_decode_the_drive_colon_and_unicode() {
        assert_eq!(
            uri_to_path("file:///c%3A/Users/a/%E6%97%A5%E6%9C%AC.mink"),
            Some(PathBuf::from("c:\\Users\\a\\\u{65e5}\u{672c}.mink"))
        );
        assert_eq!(uri_to_path("untitled:Untitled-1"), None);
        assert_eq!(uri_to_path("file:///C:/bad%2"), None);
    }

    #[test]
    fn a_clean_file_has_no_diagnostics_and_an_index() {
        let dir = std::env::temp_dir().join(format!("mink_lsp_an_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("a.mink");
        std::fs::write(&path, "fn main() {}\n").unwrap();
        let mut cache = IndexCache::default();
        let analysis = analyze(&path, &[], &HashMap::new(), &mut cache);
        assert_eq!(analysis.files.len(), 1);
        assert!(analysis.diagnostics.values().all(|d| d.is_empty()));
        assert_eq!(analysis.files[0].index.symbols[0].name, "main");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_buffer_is_checked_instead_of_the_file_on_disk() {
        let dir = std::env::temp_dir().join(format!("mink_lsp_buf_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("a.mink");
        std::fs::write(&path, "fn main() {}\n").unwrap();
        let mut cache = IndexCache::default();
        let mut uris = HashMap::new();
        uris.insert(path.clone(), "file:///client/a.mink".to_string());
        let broken = "fn main() { let x = missing_name; }\n".to_string();
        let analysis = analyze(&path, &[(path.clone(), broken)], &uris, &mut cache);
        let list = &analysis.diagnostics["file:///client/a.mink"];
        assert_eq!(list.len(), 1, "{list:?}");
        assert_eq!(list[0].code, "E-S01");
        assert!(list[0].message.contains("missing_name"));
        let _ = std::fs::remove_dir_all(dir);
    }
}
