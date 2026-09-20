//! Installing resolved packages into a project (the site-packages step).
//!
//! An install target is a **packages directory** — MINK's `site-packages`.
//! The default target is `<project>/.mink/packages`; a project with an active
//! virtual environment installs into `<project>/.mink/envs/<name>/packages`
//! instead (see [`super::env`]).
//!
//! Installing is a deterministic copy plus an integrity record:
//!
//! 1. every resolved package's directory tree is copied to
//!    `<packages>/<name>/`, replacing whatever was there,
//! 2. each installed tree is hashed ([`tree_hash`]) and recorded in
//!    `<packages>/.installed` together with its version and origin,
//! 3. packages that are no longer part of the resolution are removed, so an
//!    install is a faithful mirror of the resolution rather than an
//!    accumulation.
//!
//! The metadata file lives at the packages root and is written in a stable,
//! line-oriented format, so two installs of the same resolution produce
//! byte-identical metadata.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use super::manifest::normalise;
use super::resolver::Resolution;
use super::sha256::Sha256;

/// The metadata file at the root of a packages directory.
pub const INSTALLED_FILE: &str = ".installed";

/// One installed package's metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installed {
    /// The package name.
    pub name: String,
    /// The installed version.
    pub version: String,
    /// The origin spelling (`local:<source>` or `path:<path>`).
    pub origin: String,
    /// The `sha256:` content hash of the installed tree.
    pub checksum: String,
}

impl Installed {
    /// One metadata line: `name<TAB>version<TAB>origin<TAB>checksum`.
    fn to_line(&self) -> String {
        format!(
            "{}\t{}\t{}\t{}",
            self.name, self.version, self.origin, self.checksum
        )
    }

    /// Parses a metadata line, or `None` when it is malformed.
    fn from_line(line: &str) -> Option<Self> {
        let mut fields = line.split('\t');
        let name = fields.next()?.trim();
        let version = fields.next()?.trim();
        let origin = fields.next()?.trim();
        let checksum = fields.next()?.trim();
        if name.is_empty() || version.is_empty() || checksum.is_empty() {
            return None;
        }
        Some(Self {
            name: name.to_string(),
            version: version.to_string(),
            origin: origin.to_string(),
            checksum: checksum.to_string(),
        })
    }
}

/// A completed install.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallOutcome {
    /// The packages directory that was written.
    pub packages: PathBuf,
    /// The installed packages, sorted by name.
    pub installed: Vec<Installed>,
    /// The names of packages removed because they are no longer resolved.
    pub removed: Vec<String>,
    /// Whether anything on disk changed.
    pub changed: bool,
}

/// The result of checking an installed tree against a resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerifyOutcome {
    /// Every resolved package is installed, at the right version, with a
    /// matching content hash.
    Clean {
        /// How many packages were checked.
        packages: usize,
    },
    /// The installed tree does not match the resolution.
    Mismatch {
        /// One line per problem.
        problems: Vec<String>,
    },
}

impl VerifyOutcome {
    /// Whether the check passed.
    pub fn is_clean(&self) -> bool {
        matches!(self, Self::Clean { .. })
    }
}

impl fmt::Display for VerifyOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Clean { packages } => write!(f, "{packages} package(s) verified"),
            Self::Mismatch { problems } => {
                write!(f, "{} problem(s)", problems.len())?;
                for problem in problems {
                    write!(f, "\n  {problem}")?;
                }
                Ok(())
            }
        }
    }
}

/// An install failure.
#[derive(Debug)]
pub enum InstallError {
    /// A directory could not be read, created or written.
    Io {
        /// The path that failed.
        path: PathBuf,
        /// The underlying error.
        source: std::io::Error,
    },
    /// A resolved package's directory does not exist.
    MissingSource {
        /// The package.
        name: String,
        /// The directory that was expected.
        directory: PathBuf,
    },
}

impl fmt::Display for InstallError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => write!(f, "cannot use '{}': {source}", path.display()),
            Self::MissingSource { name, directory } => write!(
                f,
                "package '{name}' has no sources at '{}'",
                directory.display()
            ),
        }
    }
}

impl std::error::Error for InstallError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::MissingSource { .. } => None,
        }
    }
}

impl InstallError {
    /// The stable code.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Io { .. } => "E-PKG09",
            Self::MissingSource { .. } => "E-PKG09",
        }
    }
}

/// The `sha256:` content hash of a directory tree.
///
/// Files are visited in sorted relative-path order and hashed as
/// `path \0 length \0 bytes`, with `/` separators, so the digest is stable
/// across platforms and independent of directory enumeration order.
pub fn tree_hash(directory: &Path) -> Result<String, InstallError> {
    let mut files = Vec::new();
    collect_files(directory, directory, &mut files)?;
    files.sort();
    let mut hasher = Sha256::new();
    for relative in files {
        let path = directory.join(&relative);
        let bytes = std::fs::read(&path).map_err(|source| InstallError::Io {
            path: path.clone(),
            source,
        })?;
        hasher.update(relative.as_bytes());
        hasher.update(&[0]);
        hasher.update(&(bytes.len() as u64).to_le_bytes());
        hasher.update(&bytes);
    }
    Ok(format!("sha256:{}", hex_bytes(&hasher.finish())))
}

/// Renders a digest as lowercase hex.
fn hex_bytes(digest: &[u8; 32]) -> String {
    let mut out = String::with_capacity(64);
    for byte in digest {
        out.push(char::from_digit((byte >> 4) as u32, 16).unwrap_or('0'));
        out.push(char::from_digit((byte & 0x0f) as u32, 16).unwrap_or('0'));
    }
    out
}

/// Collects the files under `directory`, relative to `root`, as `/`-joined
/// strings. Directory symlinks are followed no deeper than one level by the
/// recursion itself; only regular files are hashed.
fn collect_files(root: &Path, directory: &Path, out: &mut Vec<String>) -> Result<(), InstallError> {
    let entries = std::fs::read_dir(directory).map_err(|source| InstallError::Io {
        path: directory.to_path_buf(),
        source,
    })?;
    for entry in entries.flatten() {
        let path = entry.path();
        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(_) => continue,
        };
        if file_type.is_dir() {
            collect_files(root, &path, out)?;
        } else if file_type.is_file() {
            let relative = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            out.push(relative);
        }
    }
    Ok(())
}

/// Reads the metadata file of a packages directory.
pub fn read_installed(packages: &Path) -> Result<Vec<Installed>, InstallError> {
    let path = packages.join(INSTALLED_FILE);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => {
            return Err(InstallError::Io {
                path: path.clone(),
                source,
            });
        }
    };
    let mut installed: Vec<Installed> = text.lines().filter_map(Installed::from_line).collect();
    installed.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(installed)
}

/// Writes the metadata file of a packages directory.
fn write_installed(packages: &Path, installed: &[Installed]) -> Result<(), InstallError> {
    let path = packages.join(INSTALLED_FILE);
    let mut text = String::new();
    for entry in installed {
        text.push_str(&entry.to_line());
        text.push('\n');
    }
    std::fs::write(&path, text).map_err(|source| InstallError::Io { path, source })
}

/// Installs `resolution` into `packages`, removing anything not in it.
pub fn install(resolution: &Resolution, packages: &Path) -> Result<InstallOutcome, InstallError> {
    std::fs::create_dir_all(packages).map_err(|source| InstallError::Io {
        path: packages.to_path_buf(),
        source,
    })?;
    let previous = read_installed(packages)?;
    let mut removed: Vec<String> = Vec::new();
    let wanted: BTreeMap<&str, &super::resolver::ResolvedPackage> = resolution
        .packages
        .iter()
        .map(|pkg| (pkg.name.as_str(), pkg))
        .collect();
    for old in &previous {
        if !wanted.contains_key(old.name.as_str()) {
            remove_tree(&packages.join(&old.name))?;
            removed.push(old.name.clone());
        }
    }
    // A directory from an older resolution without metadata is still stale.
    if let Ok(entries) = std::fs::read_dir(packages) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || wanted.contains_key(name.as_str()) {
                continue;
            }
            if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                remove_tree(&entry.path())?;
                if !removed.contains(&name) {
                    removed.push(name);
                }
            }
        }
    }
    removed.sort();

    let mut installed = Vec::new();
    let mut changed = !removed.is_empty();
    for package in &resolution.packages {
        if !package.directory.exists() {
            return Err(InstallError::MissingSource {
                name: package.name.clone(),
                directory: package.directory.clone(),
            });
        }
        let destination = packages.join(&package.name);
        let checksum = tree_hash(&package.directory)?;
        let entry = Installed {
            name: package.name.clone(),
            version: package.version.to_string(),
            origin: package.origin.lock_text(),
            checksum,
        };
        // Metadata alone does not prove the tree is intact: a modified or
        // partially deleted install must be repaired, not trusted, so the
        // destination's own content hash is compared as well.
        let intact = previous.iter().any(|old| *old == entry)
            && destination.exists()
            && tree_hash(&destination).is_ok_and(|actual| actual == entry.checksum);
        if !intact {
            remove_tree(&destination)?;
            copy_tree(&package.directory, &destination)?;
            changed = true;
        }
        installed.push(entry);
    }
    installed.sort_by(|a, b| a.name.cmp(&b.name));
    if previous != installed {
        write_installed(packages, &installed)?;
        changed = true;
    }
    Ok(InstallOutcome {
        packages: packages.to_path_buf(),
        installed,
        removed,
        changed,
    })
}

/// Checks an installed tree against a resolution without changing anything.
pub fn verify(resolution: &Resolution, packages: &Path) -> Result<VerifyOutcome, InstallError> {
    let installed = read_installed(packages)?;
    let mut problems = Vec::new();
    for package in &resolution.packages {
        let directory = packages.join(&package.name);
        let Some(entry) = installed.iter().find(|old| old.name == package.name) else {
            problems.push(format!("'{}' is not installed", package.name));
            continue;
        };
        if entry.version != package.version.to_string() {
            problems.push(format!(
                "'{}' is installed at {} but the resolution wants {}",
                package.name, entry.version, package.version
            ));
            continue;
        }
        if !directory.exists() {
            problems.push(format!(
                "'{}' is missing from the packages directory",
                package.name
            ));
            continue;
        }
        let actual = tree_hash(&directory)?;
        if actual != entry.checksum {
            problems.push(format!(
                "'{}' content hash is {} but the metadata records {}",
                package.name, actual, entry.checksum
            ));
        }
    }
    for old in &installed {
        if resolution.package(&old.name).is_none() {
            problems.push(format!(
                "'{}' is installed but not in the resolution",
                old.name
            ));
        }
    }
    if problems.is_empty() {
        Ok(VerifyOutcome::Clean {
            packages: resolution.packages.len(),
        })
    } else {
        Ok(VerifyOutcome::Mismatch { problems })
    }
}

/// Removes a directory tree, tolerating its absence.
pub fn remove_tree(path: &Path) -> Result<(), InstallError> {
    match std::fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(InstallError::Io {
            path: path.to_path_buf(),
            source,
        }),
    }
}

/// Copies a directory tree, creating missing parents.
pub fn copy_tree(source: &Path, destination: &Path) -> Result<(), InstallError> {
    std::fs::create_dir_all(destination).map_err(|error| InstallError::Io {
        path: destination.to_path_buf(),
        source: error,
    })?;
    let entries = std::fs::read_dir(source).map_err(|error| InstallError::Io {
        path: source.to_path_buf(),
        source: error,
    })?;
    for entry in entries.flatten() {
        let from = entry.path();
        let to = destination.join(entry.file_name());
        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(_) => continue,
        };
        if file_type.is_dir() {
            copy_tree(&from, &to)?;
        } else if file_type.is_file() {
            std::fs::copy(&from, &to).map_err(|error| InstallError::Io {
                path: to.clone(),
                source: error,
            })?;
        }
        // Symlinks are skipped: a package is a plain source tree.
    }
    Ok(())
}

/// The default packages directory for a project without an active
/// environment.
pub fn default_packages_directory(project: &Path) -> PathBuf {
    normalise(&project.join(".mink").join("packages"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::package::sha256::hex_digest;
    use crate::package::version::Version;
    use std::fs;

    /// A throwaway directory under the system temp dir.
    struct Scratch {
        root: PathBuf,
    }

    impl Scratch {
        fn new(tag: &str) -> Self {
            let unique = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            let root = std::env::temp_dir().join(format!(
                "mink-install-{tag}-{}-{unique}",
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&root);
            fs::create_dir_all(&root).expect("scratch");
            Self { root }
        }

        fn write(&self, relative: &str, body: &str) {
            let path = self.root.join(relative);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).expect("parent");
            }
            fs::write(path, body).expect("write");
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn tree_hash_is_stable_and_content_sensitive() {
        let scratch = Scratch::new("hash");
        scratch.write("pkg/a.mink", "fn a() {}\n");
        scratch.write("pkg/sub/b.mink", "fn b() {}\n");
        let first = tree_hash(&scratch.root.join("pkg")).expect("hashes");
        assert!(first.starts_with("sha256:"));
        assert_eq!(first.len(), "sha256:".len() + 64);
        assert_eq!(first, tree_hash(&scratch.root.join("pkg")).expect("hashes"));

        scratch.write("pkg/a.mink", "fn a() { } \n");
        assert_ne!(first, tree_hash(&scratch.root.join("pkg")).expect("hashes"));

        // A file rename changes the digest even when the contents match.
        scratch.write("pkg/a.mink", "fn a() {}\n");
        assert_eq!(first, tree_hash(&scratch.root.join("pkg")).expect("hashes"));
        fs::rename(
            scratch.root.join("pkg/sub/b.mink"),
            scratch.root.join("pkg/sub/c.mink"),
        )
        .expect("rename");
        assert_ne!(first, tree_hash(&scratch.root.join("pkg")).expect("hashes"));

        // An empty directory hashes to the empty digest.
        fs::create_dir_all(scratch.root.join("empty")).expect("dir");
        assert_eq!(
            tree_hash(&scratch.root.join("empty")).expect("hashes"),
            format!("sha256:{}", hex_digest(b""))
        );
    }

    #[test]
    fn installed_metadata_round_trips_and_ignores_junk() {
        let scratch = Scratch::new("meta");
        let packages = scratch.root.join("packages");
        fs::create_dir_all(&packages).expect("dir");
        let entries = vec![
            Installed {
                name: "alpha".to_string(),
                version: "1.0.0".to_string(),
                origin: "local:vendor".to_string(),
                checksum: "sha256:00".to_string(),
            },
            Installed {
                name: "beta".to_string(),
                version: "0.1.0".to_string(),
                origin: "path:../beta".to_string(),
                checksum: "sha256:11".to_string(),
            },
        ];
        write_installed(&packages, &entries).expect("writes");
        assert_eq!(read_installed(&packages).expect("reads"), entries);

        fs::write(
            packages.join(INSTALLED_FILE),
            "alpha\t1.0.0\tlocal:vendor\tsha256:00\njunk\n\n",
        )
        .expect("write");
        assert_eq!(read_installed(&packages).expect("reads").len(), 1);
    }

    #[test]
    fn a_missing_metadata_file_reads_as_empty() {
        let scratch = Scratch::new("nometa");
        fs::create_dir_all(scratch.root.join("packages")).expect("dir");
        assert!(
            read_installed(&scratch.root.join("packages"))
                .expect("reads")
                .is_empty()
        );
    }

    #[test]
    fn installing_repairs_a_modified_tree() {
        use crate::package::resolver::{Origin, ResolvedPackage};
        let scratch = Scratch::new("repair");
        scratch.write(
            "vendor/util/mink.toml",
            "[package]\nname = \"util\"\nversion = \"1.0.0\"\n",
        );
        scratch.write("vendor/util/util.mink", "pub fn v() -> Int { return 1; }\n");
        let resolution = Resolution {
            packages: vec![ResolvedPackage {
                name: "util".to_string(),
                version: Version::new(1, 0, 0),
                directory: scratch.root.join("vendor").join("util"),
                origin: Origin::Source("vendor".to_string()),
                dependencies: Vec::new(),
            }],
        };
        let packages = scratch.root.join("packages");
        let first = install(&resolution, &packages).expect("installs");
        assert!(first.changed);
        assert!(verify(&resolution, &packages).expect("verifies").is_clean());

        // A second install of an intact tree changes nothing…
        let second = install(&resolution, &packages).expect("installs");
        assert!(!second.changed);
        // …but a modified tree is repaired.
        scratch.write("packages/util/extra.mink", "fn extra() {}\n");
        assert!(!verify(&resolution, &packages).expect("verifies").is_clean());
        let third = install(&resolution, &packages).expect("installs");
        assert!(third.changed);
        assert!(!packages.join("util").join("extra.mink").exists());
        assert!(verify(&resolution, &packages).expect("verifies").is_clean());
    }

    #[test]
    fn installing_removes_packages_outside_the_resolution() {
        use crate::package::resolver::{Origin, ResolvedPackage};
        let scratch = Scratch::new("prune");
        for name in ["keep", "drop"] {
            scratch.write(
                &format!("vendor/{name}/mink.toml"),
                &format!("[package]\nname = \"{name}\"\nversion = \"1.0.0\"\n"),
            );
            scratch.write(&format!("vendor/{name}/{name}.mink"), "fn v() {}\n");
        }
        let both = Resolution {
            packages: ["keep", "drop"]
                .iter()
                .map(|name| ResolvedPackage {
                    name: name.to_string(),
                    version: Version::new(1, 0, 0),
                    directory: scratch.root.join("vendor").join(name),
                    origin: Origin::Source("vendor".to_string()),
                    dependencies: Vec::new(),
                })
                .collect(),
        };
        let packages = scratch.root.join("packages");
        install(&both, &packages).expect("installs");
        assert!(packages.join("drop").is_dir());

        let only_keep = Resolution {
            packages: vec![both.packages[0].clone()],
        };
        let outcome = install(&only_keep, &packages).expect("installs");
        assert_eq!(outcome.removed, vec!["drop".to_string()]);
        assert!(!packages.join("drop").exists());
        assert!(packages.join("keep").is_dir());
        // A directory with no metadata is stale too.
        scratch.write("packages/orphan/mink.toml", "[package]\n");
        let outcome = install(&only_keep, &packages).expect("installs");
        assert_eq!(outcome.removed, vec!["orphan".to_string()]);
        assert!(!packages.join("orphan").exists());
    }

    #[test]
    fn copy_tree_reproduces_the_source() {
        let scratch = Scratch::new("copy");
        scratch.write("from/a.mink", "fn a() {}\n");
        scratch.write("from/nested/b.mink", "fn b() {}\n");
        copy_tree(&scratch.root.join("from"), &scratch.root.join("to")).expect("copies");
        assert_eq!(
            tree_hash(&scratch.root.join("from")).expect("hashes"),
            tree_hash(&scratch.root.join("to")).expect("hashes")
        );
        remove_tree(&scratch.root.join("to")).expect("removes");
        assert!(!scratch.root.join("to").exists());
        // Removing something that is already gone is fine.
        remove_tree(&scratch.root.join("to")).expect("removes");
    }
}
