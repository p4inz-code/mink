//! The package manager: manifests, resolution, installation, lockfile, and
//! virtual environments.
//!
//! This is the capability set behind the parity rows *installed package /
//! site-packages* (P04), *dependency declaration + install* (P05), *dependency
//! resolver* (P06), *lockfile* (P07), *project metadata* (P08) and *virtual
//! environment* (P09). The architecture is
//! `docs/ecosystem/PACKAGE_ARCHITECTURE.md`; the pieces are:
//!
//! | Piece | What it does |
//! |---|---|
//! | [`manifest`] | reads and edits `mink.toml` (a dependency-free TOML subset) |
//! | [`version`] | versions and the documented requirement syntax |
//! | [`resolver`] | deterministic closure resolution with conflict/cycle reporting |
//! | [`install`] | copies packages into a packages directory and hashes the trees |
//! | [`lock`] | `mink.lock`: exact versions, sources and content hashes |
//! | [`env`] | `mink env`: per-environment package isolation |
//! | [`sha256`] | content hashing for integrity |
//!
//! A **packages directory** is MINK's site-packages: `<project>/.mink/packages`
//! by default, or `<project>/.mink/envs/<name>/packages` when an environment is
//! active. The compiler resolves `mod name;` against the active packages
//! directory as well as the declaring file's own directory and the stdlib.

pub mod env;
pub mod install;
pub mod lock;
pub mod manifest;
pub mod resolver;
pub mod sha256;
pub mod version;

use std::fmt;
use std::path::{Path, PathBuf};

pub use env::EnvError;
pub use install::{InstallError, InstallOutcome, Installed, VerifyOutcome};
pub use lock::{LOCK_NAME, Lock, LockError, LockPackage};
pub use manifest::{Dependency, MANIFEST_NAME, Manifest, ManifestError, Source};
pub use resolver::{Resolution, ResolveError, ResolvedPackage};
pub use version::{Requirement, Version, VersionError};

/// What an install run should do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InstallOptions {
    /// Ignore the lockfile and re-resolve from the manifest (`mink update`).
    pub update: bool,
    /// Report the install state without writing anything (`mink install
    /// --check`).
    pub check_only: bool,
}

impl Default for InstallOptions {
    fn default() -> Self {
        Self {
            update: false,
            check_only: false,
        }
    }
}

/// What an install run did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallReport {
    /// The project directory.
    pub project: PathBuf,
    /// The manifest that was used.
    pub manifest: PathBuf,
    /// The packages directory that was written.
    pub packages: PathBuf,
    /// The packages now installed, sorted by name.
    pub installed: Vec<Installed>,
    /// The packages removed because they are no longer resolved.
    pub removed: Vec<String>,
    /// The lockfile that was written, when one was.
    pub lock: Option<PathBuf>,
    /// Whether anything on disk changed.
    pub changed: bool,
    /// Whether the run only checked.
    pub check_only: bool,
}

impl InstallReport {
    /// A one-line summary.
    pub fn summary(&self) -> String {
        let verb = if self.check_only {
            "verified"
        } else {
            "installed"
        };
        let mut text = format!("{verb} {} package(s)", self.installed.len());
        if !self.removed.is_empty() {
            text.push_str(&format!(", removed {}", self.removed.len()));
        }
        text
    }
}

/// Every package-manager failure, with its stable code.
#[derive(Debug)]
pub enum PackageError {
    /// The project has no `mink.toml`.
    NotAProject {
        /// The directory that was searched.
        root: PathBuf,
    },
    /// A manifest problem.
    Manifest(Box<ManifestError>),
    /// A resolution problem.
    Resolve(Box<ResolveError>),
    /// An install problem.
    Install(Box<InstallError>),
    /// A lockfile problem.
    Lock(Box<LockError>),
    /// An environment problem.
    Env(Box<EnvError>),
    /// The lockfile disagrees with the manifest.
    StaleLock {
        /// The project directory.
        project: PathBuf,
        /// One line per disagreement.
        problems: Vec<String>,
    },
    /// `--check` found the installed tree out of date.
    OutOfDate {
        /// One line per problem.
        problems: Vec<String>,
    },
}

impl fmt::Display for PackageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotAProject { root } => write!(
                f,
                "no {} in '{}' (run `mink init` to create a project, or run this command in one)",
                MANIFEST_NAME,
                root.display()
            ),
            Self::Manifest(error) => write!(f, "{error}"),
            Self::Resolve(error) => write!(f, "{error}"),
            Self::Install(error) => write!(f, "{error}"),
            Self::Lock(error) => write!(f, "{error}"),
            Self::Env(error) => write!(f, "{error}"),
            Self::StaleLock { project, problems } => {
                write!(f, "{}/{} is out of date:", project.display(), LOCK_NAME)?;
                for problem in problems {
                    write!(f, "\n  {problem}")?;
                }
                write!(f, "\nrun `mink update` to re-resolve")
            }
            Self::OutOfDate { problems } => {
                write!(f, "the installed packages do not match the resolution:")?;
                for problem in problems {
                    write!(f, "\n  {problem}")?;
                }
                write!(f, "\nrun `mink install` to install them")
            }
        }
    }
}

impl std::error::Error for PackageError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Manifest(error) => Some(error.as_ref()),
            Self::Resolve(error) => Some(error.as_ref()),
            Self::Install(error) => Some(error.as_ref()),
            Self::Lock(error) => Some(error.as_ref()),
            Self::Env(error) => Some(error.as_ref()),
            _ => None,
        }
    }
}

impl PackageError {
    /// The stable diagnostic code (`E-PKG01`…`E-PKG13`).
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotAProject { .. } => "E-PKG01",
            Self::Manifest(error) => error.code(),
            Self::Resolve(error) => error.code(),
            Self::Install(error) => error.code(),
            Self::Lock(error) => error.code(),
            Self::Env(error) => error.code(),
            Self::StaleLock { .. } => "E-PKG14",
            Self::OutOfDate { .. } => "E-PKG15",
        }
    }
}

impl From<ManifestError> for PackageError {
    fn from(error: ManifestError) -> Self {
        Self::Manifest(Box::new(error))
    }
}

impl From<ResolveError> for PackageError {
    fn from(error: ResolveError) -> Self {
        Self::Resolve(Box::new(error))
    }
}

impl From<InstallError> for PackageError {
    fn from(error: InstallError) -> Self {
        Self::Install(Box::new(error))
    }
}

impl From<LockError> for PackageError {
    fn from(error: LockError) -> Self {
        Self::Lock(Box::new(error))
    }
}

impl From<EnvError> for PackageError {
    fn from(error: EnvError) -> Self {
        Self::Env(Box::new(error))
    }
}

/// The nearest ancestor of `start` that contains a `mink.toml`.
///
/// `start` may be a directory or a source file.
pub fn find_project_root(start: &Path) -> Option<PathBuf> {
    let mut current = if start.is_dir() {
        start.to_path_buf()
    } else {
        start.parent()?.to_path_buf()
    };
    // A bare file name (`mink build main.mink`) has an empty parent; treat it
    // as the current directory so the joins below stay meaningful.
    if current.as_os_str().is_empty() {
        current = PathBuf::from(".");
    }
    loop {
        if current.join(MANIFEST_NAME).is_file() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

/// Loads the manifest of the project containing `start`.
pub fn load_project(start: &Path) -> Result<Manifest, PackageError> {
    let root = find_project_root(start).ok_or_else(|| PackageError::NotAProject {
        root: start.to_path_buf(),
    })?;
    Ok(Manifest::load(&root)?)
}

/// Resolves a project's dependencies, preferring the lockfile's versions
/// unless `update` is set.
pub fn resolve_project(
    manifest: &Manifest,
    update: bool,
) -> Result<(Resolution, Option<Lock>), PackageError> {
    let lock = Lock::load(&manifest.root)?;
    let pins = match (&lock, update) {
        (Some(lock), false) => lock
            .packages
            .iter()
            .filter(|package| package.name != manifest.name)
            .map(|package| (package.name.clone(), package.version.clone()))
            .collect(),
        _ => Default::default(),
    };
    let resolution = resolver::resolve_with_pins(manifest, &pins)?;
    Ok((resolution, lock))
}

/// Installs a project's dependencies into its active packages directory.
///
/// Without `update`, the lockfile's versions are preferred and a lock that
/// disagrees with the manifest is reported as [`PackageError::StaleLock`]
/// rather than silently changed. With `update`, the manifest is re-resolved
/// from scratch and the lockfile is rewritten.
pub fn install_project(
    project: &Path,
    options: InstallOptions,
) -> Result<InstallReport, PackageError> {
    let manifest = Manifest::load(project)?;
    let (resolution, lock) = resolve_project(&manifest, options.update)?;
    if let Some(lock) = &lock {
        if !options.update {
            if let Err(problems) = lock.reusable_versions(&manifest.name, &resolution) {
                return Err(PackageError::StaleLock {
                    project: project.to_path_buf(),
                    problems,
                });
            }
        }
    }
    let packages = env::packages_directory(project)?;
    if options.check_only {
        let outcome = install::verify(&resolution, &packages)?;
        return match outcome {
            VerifyOutcome::Clean { .. } => Ok(InstallReport {
                project: project.to_path_buf(),
                manifest: manifest.path.clone(),
                packages,
                installed: install::read_installed(&env::packages_directory(project)?)?,
                removed: Vec::new(),
                lock: lock.map(|lock| lock.path),
                changed: false,
                check_only: true,
            }),
            VerifyOutcome::Mismatch { problems } => Err(PackageError::OutOfDate { problems }),
        };
    }
    let outcome = install::install(&resolution, &packages)?;
    let locked: Vec<LockPackage> = outcome
        .installed
        .iter()
        .map(|entry| LockPackage {
            name: entry.name.clone(),
            version: Version::parse(&entry.version).unwrap_or_else(|_| Version::new(0, 0, 0)),
            source: entry.origin.clone(),
            checksum: Some(entry.checksum.clone()),
            dependencies: resolution
                .package(&entry.name)
                .map(|package| package.dependencies.clone())
                .unwrap_or_default(),
        })
        .collect();
    let text = lock::render(&manifest.name, &manifest.version.to_string(), &locked);
    let lock_path = lock::write(project, &text)?;
    Ok(InstallReport {
        project: project.to_path_buf(),
        manifest: manifest.path.clone(),
        packages: outcome.packages,
        installed: outcome.installed,
        removed: outcome.removed,
        lock: Some(lock_path),
        changed: outcome.changed,
        check_only: false,
    })
}

/// Adds a dependency to `mink.toml` and installs it.
///
/// `version` is a requirement (`^1.0.0`); `path` is a local package
/// directory. Exactly one of them is required.
pub fn add_dependency(
    project: &Path,
    name: &str,
    requirement: Option<&str>,
    path: Option<&Path>,
) -> Result<InstallReport, PackageError> {
    if !manifest::is_valid_package_name(name) {
        return Err(ManifestError::BadPackageName {
            name: name.to_string(),
            path: project.join(MANIFEST_NAME),
        }
        .into());
    }
    if requirement.is_some() == path.is_some() {
        return Err(ManifestError::BadField {
            table: "dependencies".to_string(),
            field: name.to_string(),
            detail: "give either a version requirement or a path".to_string(),
            path: project.join(MANIFEST_NAME),
        }
        .into());
    }
    let dependency = Dependency {
        name: name.to_string(),
        requirement: match requirement {
            Some(text) => {
                Requirement::parse(text).map_err(|error| ManifestError::BadRequirement {
                    name: name.to_string(),
                    requirement: text.to_string(),
                    detail: error.to_string(),
                    path: project.join(MANIFEST_NAME),
                })?
            }
            None => Requirement::any(),
        },
        path: path.map(Path::to_path_buf),
    };
    // Validate the declared dependency before writing anything: a bad name,
    // a missing path or a version mismatch must not leave a broken manifest
    // behind.
    let existing = std::fs::read_to_string(project.join(MANIFEST_NAME)).map_err(|source| {
        ManifestError::Io {
            path: project.join(MANIFEST_NAME),
            source,
        }
    })?;
    let edited = Manifest::add_dependency_text(&existing, &dependency)?;
    let probe = Manifest::parse(&edited, project, project.join(MANIFEST_NAME))?;
    // Resolving the edited manifest proves the dependency is real before the
    // file is touched.
    let resolution = resolver::resolve(&probe)?;
    std::fs::write(project.join(MANIFEST_NAME), &edited).map_err(|source| ManifestError::Io {
        path: project.join(MANIFEST_NAME),
        source,
    })?;
    let packages = env::packages_directory(project)?;
    let outcome = install::install(&resolution, &packages)?;
    let locked: Vec<LockPackage> = outcome
        .installed
        .iter()
        .map(|entry| LockPackage {
            name: entry.name.clone(),
            version: Version::parse(&entry.version).unwrap_or_else(|_| Version::new(0, 0, 0)),
            source: entry.origin.clone(),
            checksum: Some(entry.checksum.clone()),
            dependencies: resolution
                .package(&entry.name)
                .map(|package| package.dependencies.clone())
                .unwrap_or_default(),
        })
        .collect();
    let text = lock::render(&probe.name, &probe.version.to_string(), &locked);
    let lock_path = lock::write(project, &text)?;
    Ok(InstallReport {
        project: project.to_path_buf(),
        manifest: probe.path.clone(),
        packages: outcome.packages,
        installed: outcome.installed,
        removed: outcome.removed,
        lock: Some(lock_path),
        changed: true,
        check_only: false,
    })
}

/// Removes a dependency from `mink.toml` and uninstalls whatever is no longer
/// required.
pub fn remove_dependency(project: &Path, name: &str) -> Result<InstallReport, PackageError> {
    let existing = std::fs::read_to_string(project.join(MANIFEST_NAME)).map_err(|source| {
        ManifestError::Io {
            path: project.join(MANIFEST_NAME),
            source,
        }
    })?;
    let (edited, removed) = Manifest::remove_dependency_text(&existing, name)?;
    if !removed {
        return Err(ManifestError::BadField {
            table: "dependencies".to_string(),
            field: name.to_string(),
            detail: "no such dependency in the manifest".to_string(),
            path: project.join(MANIFEST_NAME),
        }
        .into());
    }
    let probe = Manifest::parse(&edited, project, project.join(MANIFEST_NAME))?;
    let resolution = resolver::resolve(&probe)?;
    std::fs::write(project.join(MANIFEST_NAME), &edited).map_err(|source| ManifestError::Io {
        path: project.join(MANIFEST_NAME),
        source,
    })?;
    let packages = env::packages_directory(project)?;
    let outcome = install::install(&resolution, &packages)?;
    let locked: Vec<LockPackage> = outcome
        .installed
        .iter()
        .map(|entry| LockPackage {
            name: entry.name.clone(),
            version: Version::parse(&entry.version).unwrap_or_else(|_| Version::new(0, 0, 0)),
            source: entry.origin.clone(),
            checksum: Some(entry.checksum.clone()),
            dependencies: resolution
                .package(&entry.name)
                .map(|package| package.dependencies.clone())
                .unwrap_or_default(),
        })
        .collect();
    let text = lock::render(&probe.name, &probe.version.to_string(), &locked);
    let lock_path = lock::write(project, &text)?;
    Ok(InstallReport {
        project: project.to_path_buf(),
        manifest: probe.path.clone(),
        packages: outcome.packages,
        installed: outcome.installed,
        removed: outcome.removed,
        lock: Some(lock_path),
        changed: true,
        check_only: false,
    })
}

/// Writes a starter `mink.toml` for `project`.
pub fn initialise_project(
    project: &Path,
    name: &str,
    version: &str,
) -> Result<PathBuf, PackageError> {
    if !manifest::is_valid_package_name(name) {
        return Err(ManifestError::BadPackageName {
            name: name.to_string(),
            path: project.join(MANIFEST_NAME),
        }
        .into());
    }
    let version = Version::parse(version).map_err(|error| ManifestError::BadVersion {
        version: version.to_string(),
        detail: error.to_string(),
        path: project.join(MANIFEST_NAME),
    })?;
    std::fs::create_dir_all(project).map_err(|source| ManifestError::Io {
        path: project.to_path_buf(),
        source,
    })?;
    let path = project.join(MANIFEST_NAME);
    if path.exists() {
        return Err(ManifestError::BadField {
            table: "package".to_string(),
            field: "name".to_string(),
            detail: format!("'{}' already exists", path.display()),
            path,
        }
        .into());
    }
    let text = format!("[package]\nname = \"{name}\"\nversion = \"{version}\"\n\n[dependencies]\n");
    std::fs::write(&path, text).map_err(|source| ManifestError::Io {
        path: path.clone(),
        source,
    })?;
    Ok(path)
}

/// The module search roots a build should add for a project: the active
/// environment's packages directory (or the project's own) and every
/// resolved package directory.
///
/// `start` is the file or directory being compiled. A project that has no
/// manifest contributes no roots, so plain single-file builds are unaffected.
pub fn module_roots(start: &Path) -> Vec<PathBuf> {
    let Some(root) = find_project_root(start) else {
        return Vec::new();
    };
    let mut roots = Vec::new();
    if let Ok(packages) = env::packages_directory(&root) {
        roots.push(packages);
    }
    // Path dependencies are live source trees, so they are searched directly
    // in addition to whatever `install` copied into the packages directory.
    if let Ok(manifest) = Manifest::load(&root) {
        for dependency in &manifest.dependencies {
            if let Some(directory) = manifest.dependency_directory(dependency) {
                roots.push(directory);
            }
        }
    }
    roots
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// A throwaway project under the system temp dir.
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
                "mink-package-{tag}-{}-{unique}",
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
    fn project_root_discovery_walks_upwards() {
        let scratch = Scratch::new("root");
        scratch.write(
            "mink.toml",
            "[package]\nname = \"demo\"\nversion = \"1.0.0\"\n",
        );
        scratch.write("src/deep/app.mink", "fn main() {}\n");
        assert_eq!(
            find_project_root(&scratch.root.join("src").join("deep").join("app.mink")),
            Some(scratch.root.clone())
        );
        assert_eq!(
            find_project_root(&scratch.root.join("src")),
            Some(scratch.root.clone())
        );
        assert_eq!(
            find_project_root(&scratch.root.join("mink.toml")),
            Some(scratch.root.clone())
        );
        let elsewhere = std::env::temp_dir();
        // A directory without a manifest anywhere above reports nothing
        // (the system temp dir has no mink.toml in practice; when it does,
        // the search still returns something consistent).
        let found = find_project_root(&elsewhere);
        assert!(found.is_none_or(|path| path.join(MANIFEST_NAME).is_file()));
    }

    #[test]
    fn initialise_writes_a_manifest_and_refuses_to_overwrite() {
        let scratch = Scratch::new("init");
        let path = initialise_project(&scratch.root, "demo", "0.2.0").expect("initialises");
        assert!(path.is_file());
        let manifest = Manifest::load(&scratch.root).expect("loads");
        assert_eq!(manifest.name, "demo");
        assert_eq!(manifest.version, Version::new(0, 2, 0));
        assert_eq!(
            initialise_project(&scratch.root, "demo", "0.2.0")
                .expect_err("refuses")
                .code(),
            "E-PKG03"
        );
        assert_eq!(
            initialise_project(&scratch.root, "Bad Name", "0.2.0")
                .expect_err("refuses")
                .code(),
            "E-PKG03"
        );
    }

    #[test]
    fn module_roots_include_the_packages_directory() {
        let scratch = Scratch::new("roots");
        scratch.write(
            "mink.toml",
            "[package]\nname = \"demo\"\nversion = \"1.0.0\"\n",
        );
        let roots = module_roots(&scratch.root.join("mink.toml"));
        assert_eq!(roots.len(), 1);
        assert!(roots[0].ends_with(".mink\\packages") || roots[0].ends_with(".mink/packages"));
    }

    #[test]
    fn a_directory_without_a_manifest_has_no_roots() {
        let scratch = Scratch::new("noroots");
        assert!(module_roots(&scratch.root).is_empty());
    }
}
