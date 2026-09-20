//! The `mink.lock` lockfile.
//!
//! The format is the one in `docs/ecosystem/PACKAGE_ARCHITECTURE.md` §5: one
//! `[[package]]` entry per resolved package with its exact version, source,
//! content hash and dependency names. Two properties matter for
//! reproducibility:
//!
//! - the file is written in resolved-package order (sorted by name), so the
//!   same resolution always produces the same bytes, and
//! - `install` prefers the lock's versions when the lock is present and still
//!   satisfies the manifest, so a checkout installs the versions that were
//!   tested rather than whatever is newest today. `update` ignores the lock
//!   and re-resolves.

use std::fmt;
use std::path::{Path, PathBuf};

use super::manifest::{Document, MANIFEST_NAME, ManifestError};
use super::resolver::Resolution;
use super::version::Version;

/// The lockfile name.
pub const LOCK_NAME: &str = "mink.lock";

/// One `[[package]]` entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockPackage {
    /// The package name.
    pub name: String,
    /// The exact resolved version.
    pub version: Version,
    /// The origin spelling recorded at resolution time.
    pub source: String,
    /// The `sha256:` content hash, when the lock was written after an install.
    pub checksum: Option<String>,
    /// The package's own dependencies, sorted.
    pub dependencies: Vec<String>,
}

/// A parsed lockfile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lock {
    /// The locked packages, sorted by name.
    pub packages: Vec<LockPackage>,
    /// The file the lock came from.
    pub path: PathBuf,
}

impl Lock {
    /// The entry for `name`.
    pub fn package(&self, name: &str) -> Option<&LockPackage> {
        self.packages.iter().find(|pkg| pkg.name == name)
    }

    /// Reads `<project>/mink.lock`; `None` when there is no lockfile.
    pub fn load(project: &Path) -> Result<Option<Self>, LockError> {
        let path = project.join(LOCK_NAME);
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(source) => {
                return Err(LockError::Io { path, source });
            }
        };
        Self::parse(&text, path).map(Some)
    }

    /// Parses lockfile `text`.
    pub fn parse(text: &str, path: PathBuf) -> Result<Self, LockError> {
        let document = Document::parse(text, &path).map_err(|error| LockError::Syntax {
            detail: error.to_string(),
            path: path.clone(),
        })?;
        let mut packages = Vec::new();
        for table in document.array_tables("package") {
            let name = table
                .get("name")
                .and_then(|entry| entry.as_string())
                .ok_or_else(|| LockError::MissingField {
                    field: "name".to_string(),
                    path: path.clone(),
                })?;
            let version_text = table
                .get("version")
                .and_then(|entry| entry.as_string())
                .ok_or_else(|| LockError::MissingField {
                    field: "version".to_string(),
                    path: path.clone(),
                })?;
            let version = Version::parse(version_text).map_err(|error| LockError::Syntax {
                detail: format!("invalid version '{version_text}': {error}"),
                path: path.clone(),
            })?;
            let source = table
                .get("source")
                .and_then(|entry| entry.as_string())
                .unwrap_or("local")
                .to_string();
            let checksum = table
                .get("checksum")
                .and_then(|entry| entry.as_string())
                .map(str::to_string);
            let dependencies = table
                .get("dependencies")
                .and_then(|entry| entry.as_string_array())
                .map(|names| names.into_iter().map(str::to_string).collect())
                .unwrap_or_default();
            packages.push(LockPackage {
                name: name.to_string(),
                version,
                source,
                checksum,
                dependencies,
            });
        }
        packages.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(Self { packages, path })
    }

    /// Whether this lock still pins the resolution.
    ///
    /// `project_name` is the lock's own root entry, which is skipped: it is
    /// the project, not a dependency. Returns one line per disagreement
    /// otherwise, so `install` can report why the lock was not reused.
    pub fn reusable_versions(
        &self,
        project_name: &str,
        resolution: &Resolution,
    ) -> Result<(), Vec<String>> {
        let mut problems = Vec::new();
        for package in &resolution.packages {
            match self.package(&package.name) {
                Some(locked) if locked.version == package.version => {}
                Some(locked) => problems.push(format!(
                    "'{}' is locked at {} but the manifest resolves to {}",
                    package.name, locked.version, package.version
                )),
                None => problems.push(format!("'{}' is not in the lockfile", package.name)),
            }
        }
        for locked in &self.packages {
            if locked.name == project_name {
                continue;
            }
            if resolution.package(&locked.name).is_none() {
                problems.push(format!(
                    "'{}' is in the lockfile but no longer required",
                    locked.name
                ));
            }
        }
        if problems.is_empty() {
            Ok(())
        } else {
            Err(problems)
        }
    }
}

/// Renders a resolution as lockfile text.
///
/// The project entry is written first and every package follows in sorted
/// order, so the same resolution always produces the same bytes.
pub fn render(project_name: &str, root_version: &str, packages: &[LockPackage]) -> String {
    let mut out = String::new();
    out.push_str("# This file is auto-generated. Do not edit manually.\n");
    out.push_str("# Run `mink update` to regenerate.\n");
    out.push('\n');
    out.push_str("[[package]]\n");
    out.push_str(&format!("name = \"{project_name}\"\n"));
    out.push_str(&format!("version = \"{root_version}\"\n"));
    out.push_str("source = \"local\"\n");
    let mut roots: Vec<String> = packages
        .iter()
        .map(|package| package.name.clone())
        .collect();
    roots.sort();
    out.push_str(&format!(
        "dependencies = [{}]\n",
        roots
            .iter()
            .map(|name| format!("\"{name}\""))
            .collect::<Vec<_>>()
            .join(", ")
    ));
    let mut sorted: Vec<&LockPackage> = packages.iter().collect();
    sorted.sort_by(|a, b| a.name.cmp(&b.name));
    for package in sorted {
        out.push('\n');
        out.push_str("[[package]]\n");
        out.push_str(&format!("name = \"{}\"\n", package.name));
        out.push_str(&format!("version = \"{}\"\n", package.version));
        out.push_str(&format!("source = \"{}\"\n", package.source));
        if let Some(checksum) = &package.checksum {
            out.push_str(&format!("checksum = \"{checksum}\"\n"));
        }
        out.push_str(&format!(
            "dependencies = [{}]\n",
            package
                .dependencies
                .iter()
                .map(|name| format!("\"{name}\""))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    out
}

/// Writes `mink.lock` into `project`.
pub fn write(project: &Path, text: &str) -> Result<PathBuf, LockError> {
    let path = project.join(LOCK_NAME);
    std::fs::write(&path, text).map_err(|source| LockError::Io {
        path: path.clone(),
        source,
    })?;
    Ok(path)
}

/// The lockfile could not be read or understood.
#[derive(Debug)]
pub enum LockError {
    /// The file could not be read or written.
    Io {
        /// The path that failed.
        path: PathBuf,
        /// The underlying error.
        source: std::io::Error,
    },
    /// The lockfile is malformed.
    Syntax {
        /// What was wrong.
        detail: String,
        /// The lockfile.
        path: PathBuf,
    },
    /// A `[[package]]` entry is missing a field.
    MissingField {
        /// The missing field.
        field: String,
        /// The lockfile.
        path: PathBuf,
    },
}

impl fmt::Display for LockError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => write!(f, "cannot use '{}': {source}", path.display()),
            Self::Syntax { detail, path } => write!(f, "{}: {detail}", path.display()),
            Self::MissingField { field, path } => {
                write!(f, "{}: a [[package]] entry has no {field}", path.display())
            }
        }
    }
}

impl std::error::Error for LockError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Syntax { .. } | Self::MissingField { .. } => None,
        }
    }
}

impl LockError {
    /// The stable code.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Io { .. } => "E-PKG10",
            Self::Syntax { .. } | Self::MissingField { .. } => "E-PKG11",
        }
    }
}

/// Converts a manifest error into the lockfile's error type, so the CLI can
/// report one family of messages.
impl From<ManifestError> for LockError {
    fn from(error: ManifestError) -> Self {
        Self::Syntax {
            detail: error.to_string(),
            path: PathBuf::from(MANIFEST_NAME),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::package::resolver::{Origin, Resolution, ResolvedPackage};

    fn package(name: &str, version: &str, dependencies: &[&str]) -> LockPackage {
        LockPackage {
            name: name.to_string(),
            version: Version::parse(version).expect("version"),
            source: "local:vendor".to_string(),
            checksum: Some("sha256:abc".to_string()),
            dependencies: dependencies.iter().map(|name| name.to_string()).collect(),
        }
    }

    #[test]
    fn lock_round_trips_through_the_renderer() {
        let packages = vec![
            package("alpha", "1.0.0", &[]),
            LockPackage {
                source: "path:../beta".to_string(),
                ..package("beta", "0.2.0", &["alpha"])
            },
        ];
        let text = render("demo", "0.1.0", &packages);
        let lock = Lock::parse(&text, PathBuf::from("mink.lock")).expect("parses");
        assert_eq!(lock.packages.len(), 3);
        let root = lock.package("demo").expect("root entry");
        assert_eq!(root.dependencies, vec!["alpha", "beta"]);
        let alpha = lock.package("alpha").expect("alpha");
        assert_eq!(alpha.version, Version::new(1, 0, 0));
        assert_eq!(alpha.source, "local:vendor");
        assert_eq!(alpha.checksum.as_deref(), Some("sha256:abc"));
        let beta = lock.package("beta").expect("beta");
        assert_eq!(beta.source, "path:../beta");
        assert_eq!(beta.dependencies, vec!["alpha"]);
    }

    #[test]
    fn rendering_is_byte_stable() {
        let packages = vec![package("alpha", "1.0.0", &[])];
        let first = render("demo", "0.1.0", &packages);
        let second = render("demo", "0.1.0", &packages);
        assert_eq!(first, second);
        assert!(first.ends_with('\n'));
        assert!(first.starts_with("# This file is auto-generated."));
    }

    #[test]
    fn a_missing_lockfile_is_not_an_error() {
        let root = std::env::temp_dir().join(format!("mink-lock-missing-{}", std::process::id()));
        assert!(Lock::load(&root).expect("loads").is_none());
    }

    #[test]
    fn malformed_locks_are_rejected() {
        let cases = [
            ("[[package]]\nname = \"a\"\n", "E-PKG11"),
            ("[[package]]\nname = \"a\"\nversion = \"1\"\n", "E-PKG11"),
            ("[[package]]\nversion = \"1.0.0\"\n", "E-PKG11"),
            ("this is not toml\n", "E-PKG11"),
        ];
        for (text, code) in cases {
            let error = Lock::parse(text, PathBuf::from("mink.lock")).expect_err("rejected");
            assert_eq!(error.code(), code, "{text:?}");
        }
    }

    #[test]
    fn reusability_requires_matching_versions() {
        let resolution = Resolution {
            packages: vec![ResolvedPackage {
                name: "alpha".to_string(),
                version: Version::new(1, 0, 0),
                directory: PathBuf::from("/vendor/alpha/1.0.0"),
                origin: Origin::Source("vendor".to_string()),
                dependencies: Vec::new(),
            }],
        };
        let text = render("demo", "0.1.0", &[package("alpha", "1.0.0", &[])]);
        let lock = Lock::parse(&text, PathBuf::from("mink.lock")).expect("parses");
        assert!(lock.reusable_versions("demo", &resolution).is_ok());

        let newer = Resolution {
            packages: vec![ResolvedPackage {
                name: "alpha".to_string(),
                version: Version::new(1, 1, 0),
                directory: PathBuf::from("/vendor/alpha/1.1.0"),
                origin: Origin::Source("vendor".to_string()),
                dependencies: Vec::new(),
            }],
        };
        let problems = lock
            .reusable_versions("demo", &newer)
            .expect_err("mismatch");
        assert_eq!(problems.len(), 1);
        assert!(problems[0].contains("locked at 1.0.0"));
    }
}
