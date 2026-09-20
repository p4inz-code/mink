//! The `mink.toml` manifest: parsing, inspection, and editing.
//!
//! The architecture doc (`docs/ecosystem/PACKAGE_ARCHITECTURE.md` §3) defines
//! the manifest shape; this module reads the parts the package manager acts
//! on — `[package]`, `[dependencies]` and `[sources]` — and leaves every
//! other section (`[features]`, `[target.*]`, `[dev-dependencies]`, …) in
//! place untouched.
//!
//! The compiler is dependency-free by policy, so this is a small TOML-subset
//! reader rather than a full TOML implementation. It supports exactly what
//! the documented manifest needs:
//!
//! - `[table]` headers and `key = value` pairs,
//! - basic (`"…"`) string values,
//! - inline tables (`{ version = "1.0.0", path = "../x" }`),
//! - arrays of strings (`["a", "b"]`),
//! - `#` comments and blank lines.
//!
//! Anything else is a reported error rather than a silent misread.

use std::fmt;
use std::path::{Path, PathBuf};

use super::version::{Requirement, Version};

/// The manifest file name.
pub const MANIFEST_NAME: &str = "mink.toml";

/// A parsed `mink.toml`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    /// The `[package]` name.
    pub name: String,
    /// The `[package]` version.
    pub version: Version,
    /// The declared dependencies, sorted by name.
    pub dependencies: Vec<Dependency>,
    /// The configured package sources, sorted by name.
    pub sources: Vec<Source>,
    /// The directory holding the manifest.
    pub root: PathBuf,
    /// The manifest file itself.
    pub path: PathBuf,
}

/// One `[dependencies]` entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dependency {
    /// The dependency's package name (the table key).
    pub name: String,
    /// The version requirement; `*` for a path dependency.
    pub requirement: Requirement,
    /// The `path` of a local dependency, relative to the manifest directory.
    pub path: Option<PathBuf>,
}

/// One `[sources]` entry: a named directory holding `<name>/<version>/`
/// package directories.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    /// The source's name (the table key).
    pub name: String,
    /// The directory, relative to the manifest directory when relative.
    pub directory: PathBuf,
}

impl Dependency {
    /// Whether this dependency is a local path dependency.
    pub fn is_path(&self) -> bool {
        self.path.is_some()
    }

    /// The manifest line that declares this dependency.
    pub fn to_manifest_line(&self) -> String {
        match (&self.path, self.requirement.is_any()) {
            (Some(path), _) => format!(
                "{} = {{ path = \"{}\" }}",
                self.name,
                path.to_string_lossy().replace('\\', "/")
            ),
            (None, false) => format!("{} = \"{}\"", self.name, self.requirement.text),
            (None, true) => format!("{} = \"*\"", self.name),
        }
    }
}

impl Manifest {
    /// Reads and parses `<root>/mink.toml`.
    pub fn load(root: &Path) -> Result<Self, ManifestError> {
        let path = root.join(MANIFEST_NAME);
        let text = std::fs::read_to_string(&path).map_err(|source| ManifestError::Io {
            path: path.clone(),
            source,
        })?;
        Self::parse(&text, root, path)
    }

    /// Parses manifest `text` that lives in `root`.
    pub fn parse(text: &str, root: &Path, path: PathBuf) -> Result<Self, ManifestError> {
        let document = Document::parse(text, &path)?;
        let package = document
            .table("package")
            .ok_or_else(|| ManifestError::MissingTable {
                table: "package".to_string(),
                path: path.clone(),
            })?;
        let name = package
            .get("name")
            .and_then(Entry::as_string)
            .ok_or_else(|| ManifestError::MissingField {
                table: "package".to_string(),
                field: "name".to_string(),
                path: path.clone(),
            })?
            .to_string();
        validate_package_name(&name, &path)?;
        let version_text = package
            .get("version")
            .and_then(Entry::as_string)
            .ok_or_else(|| ManifestError::MissingField {
                table: "package".to_string(),
                field: "version".to_string(),
                path: path.clone(),
            })?;
        let version = Version::parse(version_text).map_err(|error| ManifestError::BadVersion {
            version: version_text.to_string(),
            detail: error.to_string(),
            path: path.clone(),
        })?;

        let mut dependencies = Vec::new();
        if let Some(table) = document.table("dependencies") {
            for (key, entry) in table.entries() {
                let dependency = parse_dependency(key, entry, root, &path)?;
                if dependencies
                    .iter()
                    .any(|existing: &Dependency| existing.name == dependency.name)
                {
                    return Err(ManifestError::DuplicateDependency {
                        name: dependency.name,
                        path: path.clone(),
                    });
                }
                dependencies.push(dependency);
            }
        }
        dependencies.sort_by(|a, b| a.name.cmp(&b.name));

        let mut sources = Vec::new();
        if let Some(table) = document.table("sources") {
            for (key, entry) in table.entries() {
                let directory = entry.as_string().map(PathBuf::from).ok_or_else(|| {
                    ManifestError::BadField {
                        table: "sources".to_string(),
                        field: key.to_string(),
                        detail: "expected a directory string".to_string(),
                        path: path.clone(),
                    }
                })?;
                sources.push(Source {
                    name: key.to_string(),
                    directory,
                });
            }
        }
        sources.sort_by(|a, b| a.name.cmp(&b.name));

        Ok(Self {
            name,
            version,
            dependencies,
            sources,
            root: root.to_path_buf(),
            path,
        })
    }

    /// The dependency with `name`, if declared.
    pub fn dependency(&self, name: &str) -> Option<&Dependency> {
        self.dependencies.iter().find(|dep| dep.name == name)
    }

    /// The absolute path of a path dependency.
    pub fn dependency_directory(&self, dependency: &Dependency) -> Option<PathBuf> {
        let path = dependency.path.as_ref()?;
        let joined = if path.is_absolute() {
            path.clone()
        } else {
            self.root.join(path)
        };
        Some(normalise(&joined))
    }

    /// Reads the manifest of a path dependency.
    pub fn load_dependency(&self, dependency: &Dependency) -> Result<Self, ManifestError> {
        let directory = self.dependency_directory(dependency).ok_or_else(|| {
            ManifestError::NotAPathDependency {
                name: dependency.name.clone(),
                path: self.path.clone(),
            }
        })?;
        Self::load(&directory)
    }

    /// The absolute directory of every configured source.
    pub fn source_directories(&self) -> Vec<PathBuf> {
        self.sources
            .iter()
            .map(|source| {
                if source.directory.is_absolute() {
                    source.directory.clone()
                } else {
                    normalise(&self.root.join(&source.directory))
                }
            })
            .collect()
    }

    /// Adds `dependency` to the manifest text of `path`, preserving every
    /// other line, and returns the new text.
    ///
    /// The dependency is inserted at the top of the `[dependencies]` table
    /// (creating the table at the end of the file when it is absent). A
    /// dependency with the same name is replaced in place.
    pub fn add_dependency_text(
        text: &str,
        dependency: &Dependency,
    ) -> Result<String, ManifestError> {
        let line = dependency.to_manifest_line();
        let key = format!("{} ", dependency.name);
        let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
        let header = lines.iter().position(|l| l.trim() == "[dependencies]");
        if let Some(header) = header {
            let end = next_table_header(&lines, header);
            if let Some(existing) = lines[header + 1..end]
                .iter()
                .position(|l| key_matches(l, &key))
            {
                lines[header + 1 + existing] = line;
                return Ok(join_lines(&lines));
            }
            lines.insert(header + 1, line);
            return Ok(join_lines(&lines));
        }
        if !lines.is_empty() && !lines.last().is_some_and(|l| l.trim().is_empty()) {
            lines.push(String::new());
        }
        lines.push("[dependencies]".to_string());
        lines.push(line);
        Ok(join_lines(&lines))
    }

    /// Removes the dependency named `name` from manifest `text`.
    ///
    /// Returns the new text and whether anything was removed; the
    /// `[dependencies]` header is left in place even when it becomes empty,
    /// so a later edit does not have to recreate it.
    pub fn remove_dependency_text(text: &str, name: &str) -> Result<(String, bool), ManifestError> {
        let key = format!("{name} ");
        let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
        let Some(header) = lines.iter().position(|l| l.trim() == "[dependencies]") else {
            return Ok((join_lines(&lines), false));
        };
        let end = next_table_header(&lines, header);
        let Some(existing) = lines[header + 1..end]
            .iter()
            .position(|l| key_matches(l, &key))
        else {
            return Ok((join_lines(&lines), false));
        };
        lines.remove(header + 1 + existing);
        Ok((join_lines(&lines), true))
    }
}

/// Whether `line` declares the key whose prefix is `key` (a dependency name
/// followed by a space).
fn key_matches(line: &str, key: &str) -> bool {
    let trimmed = line.trim_start();
    if !trimmed.starts_with(key.trim_end()) {
        return false;
    }
    let rest = &trimmed[key.trim_end().len()..];
    rest.starts_with(' ') || rest.starts_with('=')
}

/// The index of the next table header after `from`, or the line count.
fn next_table_header(lines: &[String], from: usize) -> usize {
    for (offset, line) in lines.iter().enumerate().skip(from + 1) {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            return offset;
        }
    }
    lines.len()
}

/// Joins lines with newlines, ending with a trailing newline.
fn join_lines(lines: &[String]) -> String {
    let mut out = lines.join("\n");
    while out.ends_with("\n\n") {
        out.pop();
    }
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

/// Whether `name` is a legal package name: lowercase letters, digits, `-`
/// and `_`, starting with a letter or digit.
pub fn is_valid_package_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
        && name
            .bytes()
            .next()
            .is_some_and(|b| b.is_ascii_alphanumeric())
}

/// Rejects an illegal package name.
fn validate_package_name(name: &str, path: &Path) -> Result<(), ManifestError> {
    if is_valid_package_name(name) {
        Ok(())
    } else {
        Err(ManifestError::BadPackageName {
            name: name.to_string(),
            path: path.to_path_buf(),
        })
    }
}

/// Normalises a path lexically (no filesystem access, no canonicalisation).
pub fn normalise(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Parses one `[dependencies]` entry.
fn parse_dependency(
    name: &str,
    entry: &Entry,
    root: &Path,
    path: &Path,
) -> Result<Dependency, ManifestError> {
    validate_package_name(name, path)?;
    match entry {
        Entry::String(text) => Ok(Dependency {
            name: name.to_string(),
            requirement: Requirement::parse(text).map_err(|error| {
                ManifestError::BadRequirement {
                    name: name.to_string(),
                    requirement: text.clone(),
                    detail: error.to_string(),
                    path: path.to_path_buf(),
                }
            })?,
            path: None,
        }),
        Entry::InlineTable(fields) => {
            let path_field = fields
                .iter()
                .find(|(key, _)| key == "path")
                .and_then(|(_, value)| value.as_string())
                .map(PathBuf::from);
            let version_field = fields
                .iter()
                .find(|(key, _)| key == "version")
                .and_then(|(_, value)| value.as_string());
            let requirement = match version_field {
                Some(text) => {
                    Requirement::parse(text).map_err(|error| ManifestError::BadRequirement {
                        name: name.to_string(),
                        requirement: text.to_string(),
                        detail: error.to_string(),
                        path: path.to_path_buf(),
                    })?
                }
                None => Requirement::any(),
            };
            if path_field.is_none() && version_field.is_none() {
                return Err(ManifestError::BadField {
                    table: "dependencies".to_string(),
                    field: name.to_string(),
                    detail: "expected a version or a path".to_string(),
                    path: path.to_path_buf(),
                });
            }
            // A path dependency's declared version, when present, must match
            // the manifest sitting at that path.
            if let (Some(relative), Some(want)) = (&path_field, version_field) {
                let directory = normalise(&root.join(relative));
                let nested = Manifest::load(&directory)?;
                if !Requirement::parse(want)
                    .map_err(|error| ManifestError::BadRequirement {
                        name: name.to_string(),
                        requirement: want.to_string(),
                        detail: error.to_string(),
                        path: path.to_path_buf(),
                    })?
                    .matches(&nested.version)
                {
                    return Err(ManifestError::PathVersionMismatch {
                        name: name.to_string(),
                        requirement: want.to_string(),
                        found: nested.version,
                        path: path.to_path_buf(),
                    });
                }
            }
            Ok(Dependency {
                name: name.to_string(),
                requirement,
                path: path_field,
            })
        }
        Entry::Array(_) => Err(ManifestError::BadField {
            table: "dependencies".to_string(),
            field: name.to_string(),
            detail: "expected a string or an inline table".to_string(),
            path: path.to_path_buf(),
        }),
    }
}

/// A parsed TOML subset document: tables in source order.
#[derive(Debug, Clone, Default)]
pub(crate) struct Document {
    tables: Vec<(String, Table)>,
}

/// One table: key/value entries in source order.
#[derive(Debug, Clone, Default)]
pub(crate) struct Table {
    entries: Vec<(String, Entry)>,
}

impl Table {
    /// The value for `key`.
    pub(crate) fn get(&self, key: &str) -> Option<&Entry> {
        self.entries
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, entry)| entry)
    }

    /// Every entry, in source order.
    pub(crate) fn entries(&self) -> impl Iterator<Item = (&str, &Entry)> {
        self.entries
            .iter()
            .map(|(key, entry)| (key.as_str(), entry))
    }
}

impl Document {
    /// The table named `name`, if present.
    pub(crate) fn table(&self, name: &str) -> Option<&Table> {
        self.tables
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, table)| table)
    }

    /// Every `[[name]]` array-of-tables element, in source order.
    pub(crate) fn array_tables(&self, name: &str) -> Vec<&Table> {
        let prefix = format!("{name}#");
        self.tables
            .iter()
            .filter(|(key, _)| key.starts_with(&prefix))
            .map(|(_, table)| table)
            .collect()
    }

    /// Parses the supported TOML subset.
    pub(crate) fn parse(text: &str, path: &Path) -> Result<Self, ManifestError> {
        let mut document = Document::default();
        let mut current: Option<usize> = None;
        for (number, raw) in text.lines().enumerate() {
            let line_number = number + 1;
            let line = strip_comment(raw).trim();
            if line.is_empty() {
                continue;
            }
            if let Some(header) = line.strip_prefix('[') {
                // `[[name]]` is an array of tables; each element is stored
                // under a distinct `name#index` key so `name` stays a plain
                // lookup and every element keeps its source order.
                if let Some(element) = header.strip_prefix('[') {
                    let Some(name) = element.strip_suffix("]]") else {
                        return Err(ManifestError::Syntax {
                            line: line_number,
                            detail: "unterminated table-array header".to_string(),
                            path: path.to_path_buf(),
                        });
                    };
                    let name = name.trim().to_string();
                    let count = document
                        .tables
                        .iter()
                        .filter(|(key, _)| key.starts_with(&format!("{name}#")))
                        .count();
                    document
                        .tables
                        .push((format!("{name}#{count}"), Table::default()));
                    current = Some(document.tables.len() - 1);
                    continue;
                }
                let Some(name) = header.strip_suffix(']') else {
                    return Err(ManifestError::Syntax {
                        line: line_number,
                        detail: "unterminated table header".to_string(),
                        path: path.to_path_buf(),
                    });
                };
                let name = name.trim().to_string();
                // A table may be defined once; repeating it would silently
                // discard the earlier definition (TOML rejects it too).
                if document.tables.iter().any(|(key, _)| *key == name) {
                    return Err(ManifestError::DuplicateKey {
                        key: format!("[{name}]"),
                        line: line_number,
                        path: path.to_path_buf(),
                    });
                }
                document.tables.push((name, Table::default()));
                current = Some(document.tables.len() - 1);
                continue;
            }
            let Some((key, value)) = split_key_value(line, line_number, path)? else {
                return Err(ManifestError::Syntax {
                    line: line_number,
                    detail: format!("expected `key = value`, found '{line}'"),
                    path: path.to_path_buf(),
                });
            };
            let Some(index) = current else {
                return Err(ManifestError::Syntax {
                    line: line_number,
                    detail: "value outside any table".to_string(),
                    path: path.to_path_buf(),
                });
            };
            let entry = parse_value(value, line_number, path)?;
            if document.tables[index].1.get(&key).is_some() {
                return Err(ManifestError::DuplicateKey {
                    key,
                    line: line_number,
                    path: path.to_path_buf(),
                });
            }
            document.tables[index].1.entries.push((key, entry));
        }
        Ok(document)
    }
}

/// One value in a table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Entry {
    /// A quoted string.
    String(String),
    /// An inline table: `{ a = "b", c = "d" }`.
    InlineTable(Vec<(String, Entry)>),
    /// An array of values.
    Array(Vec<Entry>),
}

impl Entry {
    /// The string value, if this entry is a string.
    pub(crate) fn as_string(&self) -> Option<&str> {
        match self {
            Self::String(text) => Some(text),
            Self::InlineTable(_) | Self::Array(_) => None,
        }
    }

    /// The string items, if this entry is an array of strings.
    pub(crate) fn as_string_array(&self) -> Option<Vec<&str>> {
        match self {
            Self::Array(items) => items.iter().map(Self::as_string).collect(),
            Self::String(_) | Self::InlineTable(_) => None,
        }
    }
}

/// Removes a `#` comment that is not inside a string.
fn strip_comment(line: &str) -> &str {
    let mut in_string = false;
    let mut escaped = false;
    for (index, byte) in line.bytes().enumerate() {
        if escaped {
            escaped = false;
            continue;
        }
        match byte {
            b'\\' if in_string => escaped = true,
            b'"' => in_string = !in_string,
            b'#' if !in_string => return &line[..index],
            _ => {}
        }
    }
    line
}

/// Splits `key = value` at the first top-level `=`.
fn split_key_value<'a>(
    line: &'a str,
    line_number: usize,
    path: &Path,
) -> Result<Option<(String, &'a str)>, ManifestError> {
    let mut in_string = false;
    for (index, byte) in line.bytes().enumerate() {
        match byte {
            b'"' => in_string = !in_string,
            b'=' if !in_string => {
                let key = line[..index].trim();
                let value = line[index + 1..].trim();
                if key.is_empty() {
                    return Err(ManifestError::Syntax {
                        line: line_number,
                        detail: "empty key".to_string(),
                        path: path.to_path_buf(),
                    });
                }
                if value.is_empty() {
                    return Err(ManifestError::Syntax {
                        line: line_number,
                        detail: format!("key '{key}' has no value"),
                        path: path.to_path_buf(),
                    });
                }
                return Ok(Some((unquote_key(key), value)));
            }
            _ => {}
        }
    }
    Ok(None)
}

/// Strips optional quotes from a key.
fn unquote_key(key: &str) -> String {
    key.trim_matches('"').to_string()
}

/// Parses a value expression.
fn parse_value(text: &str, line_number: usize, path: &Path) -> Result<Entry, ManifestError> {
    if let Some(body) = text.strip_prefix('{') {
        let Some(body) = body.strip_suffix('}') else {
            return Err(ManifestError::Syntax {
                line: line_number,
                detail: "unterminated inline table".to_string(),
                path: path.to_path_buf(),
            });
        };
        let mut fields = Vec::new();
        for field in split_top_level(body) {
            let field = field.trim();
            if field.is_empty() {
                continue;
            }
            let Some((key, value)) = split_key_value(field, line_number, path)? else {
                return Err(ManifestError::Syntax {
                    line: line_number,
                    detail: format!("expected `key = value` in inline table, found '{field}'"),
                    path: path.to_path_buf(),
                });
            };
            fields.push((unquote_key(&key), parse_value(value, line_number, path)?));
        }
        return Ok(Entry::InlineTable(fields));
    }
    if let Some(body) = text.strip_prefix('[') {
        let Some(body) = body.strip_suffix(']') else {
            return Err(ManifestError::Syntax {
                line: line_number,
                detail: "unterminated array".to_string(),
                path: path.to_path_buf(),
            });
        };
        let mut items = Vec::new();
        for item in split_top_level(body) {
            let item = item.trim();
            if item.is_empty() {
                continue;
            }
            items.push(parse_value(item, line_number, path)?);
        }
        return Ok(Entry::Array(items));
    }
    if let Some(body) = text.strip_prefix('"') {
        let Some(body) = body.strip_suffix('"') else {
            return Err(ManifestError::Syntax {
                line: line_number,
                detail: "unterminated string".to_string(),
                path: path.to_path_buf(),
            });
        };
        return Ok(Entry::String(unescape(body)));
    }
    Err(ManifestError::Syntax {
        line: line_number,
        detail: format!(
            "unsupported value '{text}' (only quoted strings, inline tables and arrays are read)"
        ),
        path: path.to_path_buf(),
    })
}

/// Splits on commas that are not inside a string, array or inline table.
fn split_top_level(text: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut in_string = false;
    let mut start = 0usize;
    for (index, byte) in text.bytes().enumerate() {
        match byte {
            b'"' => in_string = !in_string,
            b'{' | b'[' if !in_string => depth += 1,
            b'}' | b']' if !in_string => depth -= 1,
            b',' if !in_string && depth == 0 => {
                parts.push(&text[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }
    parts.push(&text[start..]);
    parts
}

/// Resolves the escape sequences the manifest subset supports.
fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('\\') => out.push('\\'),
            Some('"') => out.push('"'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// A malformed or unusable manifest.
#[derive(Debug)]
pub enum ManifestError {
    /// The manifest could not be read.
    Io {
        /// The path that failed.
        path: PathBuf,
        /// The underlying I/O error.
        source: std::io::Error,
    },
    /// `mink.toml` is not beside the project.
    NotFound {
        /// The directory that was searched.
        root: PathBuf,
    },
    /// A syntax problem in the supported subset.
    Syntax {
        /// 1-based line number.
        line: usize,
        /// What was wrong.
        detail: String,
        /// The manifest.
        path: PathBuf,
    },
    /// A duplicate key in one table.
    DuplicateKey {
        /// The repeated key.
        key: String,
        /// 1-based line number.
        line: usize,
        /// The manifest.
        path: PathBuf,
    },
    /// The `[package]` table is absent.
    MissingTable {
        /// The missing table.
        table: String,
        /// The manifest.
        path: PathBuf,
    },
    /// A required field is absent.
    MissingField {
        /// The table.
        table: String,
        /// The missing field.
        field: String,
        /// The manifest.
        path: PathBuf,
    },
    /// A field has the wrong shape.
    BadField {
        /// The table.
        table: String,
        /// The field.
        field: String,
        /// What was expected.
        detail: String,
        /// The manifest.
        path: PathBuf,
    },
    /// `[package] name` is not a legal package name.
    BadPackageName {
        /// The rejected name.
        name: String,
        /// The manifest.
        path: PathBuf,
    },
    /// `[package] version` is not a version.
    BadVersion {
        /// The rejected version.
        version: String,
        /// Why it was rejected.
        detail: String,
        /// The manifest.
        path: PathBuf,
    },
    /// A dependency's requirement is malformed.
    BadRequirement {
        /// The dependency.
        name: String,
        /// The rejected requirement.
        requirement: String,
        /// Why it was rejected.
        detail: String,
        /// The manifest.
        path: PathBuf,
    },
    /// A dependency is declared twice.
    DuplicateDependency {
        /// The repeated name.
        name: String,
        /// The manifest.
        path: PathBuf,
    },
    /// A path dependency's version does not satisfy the declaration.
    PathVersionMismatch {
        /// The dependency.
        name: String,
        /// The declared requirement.
        requirement: String,
        /// The version found at the path.
        found: Version,
        /// The manifest.
        path: PathBuf,
    },
    /// A path was asked for on a dependency that has none.
    NotAPathDependency {
        /// The dependency.
        name: String,
        /// The manifest.
        path: PathBuf,
    },
}

impl fmt::Display for ManifestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => write!(f, "cannot read '{}': {source}", path.display()),
            Self::NotFound { root } => write!(
                f,
                "no {} in '{}' (run this command in a MINK project)",
                MANIFEST_NAME,
                root.display()
            ),
            Self::Syntax { line, detail, path } => write!(f, "{}:{line}: {detail}", path.display()),
            Self::DuplicateKey { key, line, path } => {
                write!(f, "{}:{line}: key '{key}' appears twice", path.display())
            }
            Self::MissingTable { table, path } => {
                write!(f, "{}: missing [{table}] table", path.display())
            }
            Self::MissingField { table, field, path } => {
                write!(f, "{}: missing {table}.{field}", path.display())
            }
            Self::BadField {
                table,
                field,
                detail,
                path,
            } => write!(f, "{}: invalid {table}.{field}: {detail}", path.display()),
            Self::BadPackageName { name, path } => write!(
                f,
                "{}: invalid package name '{name}' (lowercase letters, digits, '-' and '_' only)",
                path.display()
            ),
            Self::BadVersion {
                version,
                detail,
                path,
            } => write!(
                f,
                "{}: invalid version '{version}': {detail}",
                path.display()
            ),
            Self::BadRequirement {
                name,
                requirement,
                detail,
                path,
            } => write!(
                f,
                "{}: invalid requirement for '{name}' ('{requirement}'): {detail}",
                path.display()
            ),
            Self::DuplicateDependency { name, path } => write!(
                f,
                "{}: dependency '{name}' is declared twice",
                path.display()
            ),
            Self::PathVersionMismatch {
                name,
                requirement,
                found,
                path,
            } => write!(
                f,
                "{}: dependency '{name}' requires '{requirement}' but the package at that path is {found}",
                path.display()
            ),
            Self::NotAPathDependency { name, path } => write!(
                f,
                "{}: dependency '{name}' is not a path dependency",
                path.display()
            ),
        }
    }
}

impl std::error::Error for ManifestError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

/// The diagnostic code for a manifest error.
impl ManifestError {
    /// The stable code (the `E-PKG01`… family).
    pub fn code(&self) -> &'static str {
        match self {
            Self::Io { .. } | Self::NotFound { .. } => "E-PKG01",
            Self::Syntax { .. } | Self::DuplicateKey { .. } => "E-PKG02",
            Self::MissingTable { .. }
            | Self::MissingField { .. }
            | Self::BadField { .. }
            | Self::BadPackageName { .. }
            | Self::BadVersion { .. }
            | Self::BadRequirement { .. }
            | Self::DuplicateDependency { .. } => "E-PKG03",
            Self::PathVersionMismatch { .. } | Self::NotAPathDependency { .. } => "E-PKG04",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Result<Manifest, ManifestError> {
        Manifest::parse(
            text,
            Path::new("/project"),
            PathBuf::from("/project/mink.toml"),
        )
    }

    const BASIC: &str = "\
[package]
name = \"demo\"
version = \"0.1.0\"

[dependencies]
util = \"^1.0.0\"
helper = { path = \"../helper\" }
";

    #[test]
    fn parses_the_documented_manifest_shape() {
        let manifest = parse(BASIC).expect("parses");
        assert_eq!(manifest.name, "demo");
        assert_eq!(manifest.version, Version::new(0, 1, 0));
        assert_eq!(manifest.dependencies.len(), 2);
        assert_eq!(manifest.dependencies[0].name, "helper");
        assert!(manifest.dependencies[0].is_path());
        assert_eq!(
            manifest.dependencies[0].path.as_deref(),
            Some(Path::new("../helper"))
        );
        assert_eq!(manifest.dependencies[1].name, "util");
        assert_eq!(manifest.dependencies[1].requirement.text, "^1.0.0");
    }

    #[test]
    fn comments_and_unknown_sections_are_ignored() {
        let text = "\
[package]
name = \"demo\" # the name
version = \"1.2.3\"

[features]
default = [\"std\"]
std = []

[target.'cfg(target_os = \"windows\")'.dependencies]
win = \"1.0.0\"
";
        let manifest = parse(text).expect("parses");
        assert_eq!(manifest.version, Version::new(1, 2, 3));
        assert!(manifest.dependencies.is_empty());
    }

    #[test]
    fn sources_are_read_and_resolved() {
        let text = "\
[package]
name = \"demo\"
version = \"1.0.0\"

[sources]
vendor = \"vendor\"
absolute = \"/pkg\"
";
        let manifest = parse(text).expect("parses");
        assert_eq!(manifest.sources.len(), 2);
        let directories = manifest.source_directories();
        assert_eq!(directories.len(), 2);
        assert!(directories[0].ends_with("pkg"));
    }

    #[test]
    fn malformed_inputs_are_rejected_with_a_line() {
        let cases = [
            ("[package]\nname = \"demo\"\n", "E-PKG03"),
            ("[package]\nname = \"demo\"\nversion = \"1\"\n", "E-PKG03"),
            (
                "[package]\nname = \"Demo\"\nversion = \"1.0.0\"\n",
                "E-PKG03",
            ),
            (
                "[package]\nname = \"demo\"\nversion = \"1.0.0\"\n\n[dependencies]\nutil = \"^\"\n",
                "E-PKG03",
            ),
            (
                "[package]\nname = \"demo\"\nversion = \"1.0.0\"\nunterminated\n",
                "E-PKG02",
            ),
            (
                "[package]\nname = \"demo\"\nversion = \"1.0.0\"\n\n[dependencies]\nutil = 1\n",
                "E-PKG02",
            ),
        ];
        for (text, code) in cases {
            let error = parse(text).expect_err("rejected");
            assert_eq!(error.code(), code, "{text:?}");
            assert!(!error.to_string().is_empty());
        }
    }

    #[test]
    fn a_duplicate_table_is_rejected() {
        let text = "\
[package]
name = \"demo\"
version = \"1.0.0\"

[dependencies]
util = \"1.0.0\"

[dependencies]
other = \"1.0.0\"
";
        let error = parse(text).expect_err("rejected");
        assert_eq!(error.code(), "E-PKG02");
        assert!(error.to_string().contains("[dependencies]"), "{error}");
    }

    #[test]
    fn a_duplicate_dependency_is_rejected() {
        let text = "\
[package]
name = \"demo\"
version = \"1.0.0\"

[dependencies]
util = \"1.0.0\"
util = \"2.0.0\"
";
        assert_eq!(parse(text).expect_err("rejected").code(), "E-PKG02");
    }

    #[test]
    fn add_and_remove_edit_the_dependency_table_in_place() {
        let dependency = Dependency {
            name: "zebra".to_string(),
            requirement: Requirement::parse("^2.0.0").expect("parses"),
            path: None,
        };
        let edited = Manifest::add_dependency_text(BASIC, &dependency).expect("edits");
        assert!(edited.contains("[dependencies]\nzebra = \"^2.0.0\"\n"));
        assert!(edited.contains("util = \"^1.0.0\"\n"));
        let manifest = parse(&edited).expect("parses");
        assert_eq!(manifest.dependencies.len(), 3);

        // Replacing an existing dependency keeps one entry.
        let replaced = Manifest::add_dependency_text(
            &edited,
            &Dependency {
                name: "zebra".to_string(),
                requirement: Requirement::parse("^3.0.0").expect("parses"),
                path: None,
            },
        )
        .expect("edits");
        assert_eq!(parse(&replaced).expect("parses").dependencies.len(), 3);
        assert!(replaced.contains("zebra = \"^3.0.0\""));

        let (removed, changed) = Manifest::remove_dependency_text(&edited, "zebra").expect("edits");
        assert!(changed);
        assert_eq!(parse(&removed).expect("parses").dependencies.len(), 2);
        let (again, changed) = Manifest::remove_dependency_text(&removed, "zebra").expect("edits");
        assert!(!changed);
        assert_eq!(again, removed);
    }

    #[test]
    fn add_creates_the_dependency_table_when_absent() {
        let text = "[package]\nname = \"demo\"\nversion = \"1.0.0\"\n";
        let edited = Manifest::add_dependency_text(
            text,
            &Dependency {
                name: "util".to_string(),
                requirement: Requirement::any(),
                path: Some(PathBuf::from("../util")),
            },
        )
        .expect("edits");
        let manifest = parse(&edited).expect("parses");
        assert_eq!(manifest.dependencies.len(), 1);
        assert!(edited.contains("util = { path = \"../util\" }"));
    }

    #[test]
    fn path_dependencies_resolve_relative_to_the_manifest() {
        let manifest = parse(BASIC).expect("parses");
        let helper = manifest.dependency("helper").expect("declared");
        assert_eq!(
            manifest.dependency_directory(helper).expect("path"),
            PathBuf::from("/helper")
        );
    }

    #[test]
    fn paths_normalise_without_touching_the_filesystem() {
        assert_eq!(normalise(Path::new("/a/b/../c")), PathBuf::from("/a/c"));
        assert_eq!(normalise(Path::new("a/./b")), PathBuf::from("a/b"));
        assert_eq!(normalise(Path::new("../x")), PathBuf::from("../x"));
    }

    #[test]
    fn package_names_are_validated() {
        assert!(is_valid_package_name("mink-json"));
        assert!(is_valid_package_name("a"));
        assert!(is_valid_package_name("x_1"));
        assert!(!is_valid_package_name(""));
        assert!(!is_valid_package_name("-x"));
        assert!(!is_valid_package_name("Upper"));
        assert!(!is_valid_package_name("has space"));
        assert!(!is_valid_package_name("has/slash"));
    }
}
