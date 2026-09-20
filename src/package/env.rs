//! Virtual environments (`mink env`) — MINK's environment isolation.
//!
//! An environment is a directory that owns its own packages directory:
//!
//! ```text
//! <project>/
//!   .mink/
//!     packages/                 # the project's own packages (no env active)
//!     active-env                # the name of the active environment
//!     envs/
//!       dev/
//!         packages/             # only this environment's packages
//!       release/
//!         packages/
//! ```
//!
//! Two environments never share installed packages: `install` writes into
//! whichever packages directory is active, and module resolution reads only
//! that one (falling back to the project's own packages when no environment
//! is active). That is the isolation property Python's `venv` provides, with
//! MINK's directory shape instead of `bin/`/`Scripts/` symlink farms.

use std::fmt;
use std::path::{Path, PathBuf};

use super::install::default_packages_directory;
use super::manifest::normalise;

/// The project state directory.
pub const STATE_DIR: &str = ".mink";

/// The directory holding every environment.
pub const ENVS_DIR: &str = "envs";

/// The file naming the active environment.
pub const ACTIVE_FILE: &str = "active-env";

/// The directory inside an environment that holds its packages.
pub const ENV_PACKAGES_DIR: &str = "packages";

/// One environment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Environment {
    /// The environment name.
    pub name: String,
    /// The environment directory.
    pub directory: PathBuf,
    /// Whether this environment is the active one.
    pub active: bool,
}

impl Environment {
    /// The environment's packages directory.
    pub fn packages_directory(&self) -> PathBuf {
        self.directory.join(ENV_PACKAGES_DIR)
    }
}

/// The `mink env` subcommands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnvAction {
    /// Create an environment (and activate it).
    New,
    /// Activate an existing environment.
    Use,
    /// List the environments.
    List,
    /// Remove an environment.
    Remove,
}

/// An environment failure.
#[derive(Debug)]
pub enum EnvError {
    /// The directory could not be created or removed.
    Io {
        /// The path that failed.
        path: PathBuf,
        /// The underlying error.
        source: std::io::Error,
    },
    /// The project has no `mink.toml`.
    NotAProject {
        /// The directory that was searched.
        root: PathBuf,
    },
    /// The environment name is not a legal name.
    BadName(String),
    /// The environment does not exist.
    Unknown {
        /// The name that was asked for.
        name: String,
        /// The environments that do exist.
        available: Vec<String>,
    },
    /// The environment already exists.
    Exists {
        /// The name that was asked for.
        name: String,
    },
}

impl fmt::Display for EnvError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => write!(f, "cannot use '{}': {source}", path.display()),
            Self::NotAProject { root } => write!(
                f,
                "no {} in '{}' (run this command in a MINK project)",
                super::manifest::MANIFEST_NAME,
                root.display()
            ),
            Self::BadName(name) => write!(
                f,
                "invalid environment name '{name}' (lowercase letters, digits, '-' and '_' only)"
            ),
            Self::Unknown { name, available } => {
                write!(f, "no environment named '{name}'")?;
                if available.is_empty() {
                    write!(f, " (no environments exist)")
                } else {
                    write!(f, " (available: {})", available.join(", "))
                }
            }
            Self::Exists { name } => write!(f, "environment '{name}' already exists"),
        }
    }
}

impl std::error::Error for EnvError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl EnvError {
    /// The stable code.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Io { .. } => "E-PKG12",
            Self::NotAProject { .. }
            | Self::BadName(_)
            | Self::Unknown { .. }
            | Self::Exists { .. } => "E-PKG13",
        }
    }
}

/// Whether `name` is a legal environment name.
pub fn is_valid_env_name(name: &str) -> bool {
    super::manifest::is_valid_package_name(name)
}

/// The `.mink` state directory of a project.
pub fn state_directory(project: &Path) -> PathBuf {
    normalise(&project.join(STATE_DIR))
}

/// The directory holding every environment of a project.
pub fn environments_directory(project: &Path) -> PathBuf {
    state_directory(project).join(ENVS_DIR)
}

/// The name of the active environment, if any.
pub fn active(project: &Path) -> Result<Option<String>, EnvError> {
    let path = state_directory(project).join(ACTIVE_FILE);
    match std::fs::read_to_string(&path) {
        Ok(text) => {
            let name = text.trim().to_string();
            if name.is_empty() {
                Ok(None)
            } else {
                Ok(Some(name))
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(EnvError::Io {
            path: path.clone(),
            source,
        }),
    }
}

/// The packages directory an install should use, given the active
/// environment.
pub fn packages_directory(project: &Path) -> Result<PathBuf, EnvError> {
    match active(project)? {
        Some(name) => Ok(environment(project, &name)?.packages_directory()),
        None => Ok(default_packages_directory(project)),
    }
}

/// The environment named `name` of `project`.
pub fn environment(project: &Path, name: &str) -> Result<Environment, EnvError> {
    if !is_valid_env_name(name) {
        return Err(EnvError::BadName(name.to_string()));
    }
    let directory = environments_directory(project).join(name);
    if !directory.is_dir() {
        return Err(EnvError::Unknown {
            name: name.to_string(),
            available: list(project)?.into_iter().map(|env| env.name).collect(),
        });
    }
    Ok(Environment {
        name: name.to_string(),
        directory,
        active: active(project)?.as_deref() == Some(name),
    })
}

/// Every environment of `project`, sorted by name.
pub fn list(project: &Path) -> Result<Vec<Environment>, EnvError> {
    let active_name = active(project)?;
    let directory = environments_directory(project);
    let mut names: Vec<String> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&directory) {
        for entry in entries.flatten() {
            if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                names.push(entry.file_name().to_string_lossy().into_owned());
            }
        }
    }
    names.sort();
    Ok(names
        .into_iter()
        .map(|name| Environment {
            directory: directory.join(&name),
            active: active_name.as_deref() == Some(name.as_str()),
            name,
        })
        .collect())
}

/// Creates (or reuses) the environment `name` and activates it.
pub fn create(project: &Path, name: &str) -> Result<Environment, EnvError> {
    if !is_valid_env_name(name) {
        return Err(EnvError::BadName(name.to_string()));
    }
    let directory = environments_directory(project).join(name);
    std::fs::create_dir_all(directory.join(ENV_PACKAGES_DIR)).map_err(|source| EnvError::Io {
        path: directory.clone(),
        source,
    })?;
    activate(project, name)?;
    Ok(Environment {
        name: name.to_string(),
        directory,
        active: true,
    })
}

/// Writes the active-environment pointer.
pub fn activate(project: &Path, name: &str) -> Result<(), EnvError> {
    if !is_valid_env_name(name) {
        return Err(EnvError::BadName(name.to_string()));
    }
    if !environments_directory(project).join(name).is_dir() {
        return Err(EnvError::Unknown {
            name: name.to_string(),
            available: list(project)?.into_iter().map(|env| env.name).collect(),
        });
    }
    let state = state_directory(project);
    std::fs::create_dir_all(&state).map_err(|source| EnvError::Io {
        path: state.clone(),
        source,
    })?;
    let path = state.join(ACTIVE_FILE);
    std::fs::write(&path, format!("{name}\n")).map_err(|source| EnvError::Io { path, source })
}

/// Removes the environment `name`, clearing the pointer when it was active.
pub fn remove(project: &Path, name: &str) -> Result<Environment, EnvError> {
    let environment = environment(project, name)?;
    std::fs::remove_dir_all(&environment.directory).map_err(|source| EnvError::Io {
        path: environment.directory.clone(),
        source,
    })?;
    if environment.active {
        let path = state_directory(project).join(ACTIVE_FILE);
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(source) => return Err(EnvError::Io { path, source }),
        }
    }
    Ok(environment)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// A throwaway project directory under the system temp dir.
    struct Scratch {
        root: PathBuf,
    }

    impl Scratch {
        fn new(tag: &str) -> Self {
            let unique = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            let root = std::env::temp_dir()
                .join(format!("mink-env-{tag}-{}-{unique}", std::process::id()));
            let _ = fs::remove_dir_all(&root);
            fs::create_dir_all(&root).expect("scratch");
            fs::write(
                root.join("mink.toml"),
                "[package]\nname = \"demo\"\nversion = \"1.0.0\"\n",
            )
            .expect("manifest");
            Self { root }
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn without_an_environment_installs_go_to_the_project() {
        let scratch = Scratch::new("default");
        assert_eq!(active(&scratch.root).expect("active"), None);
        assert_eq!(
            packages_directory(&scratch.root).expect("packages"),
            default_packages_directory(&scratch.root)
        );
    }

    #[test]
    fn creating_an_environment_activates_it() {
        let scratch = Scratch::new("create");
        let created = create(&scratch.root, "dev").expect("creates");
        assert!(created.active);
        assert!(created.packages_directory().is_dir());
        assert_eq!(
            active(&scratch.root).expect("active"),
            Some("dev".to_string())
        );
        assert_eq!(
            packages_directory(&scratch.root).expect("packages"),
            created.packages_directory()
        );
    }

    #[test]
    fn environments_are_listed_in_name_order_and_isolated() {
        let scratch = Scratch::new("list");
        create(&scratch.root, "release").expect("creates");
        create(&scratch.root, "dev").expect("creates");
        let environments = list(&scratch.root).expect("lists");
        let names: Vec<&str> = environments.iter().map(|env| env.name.as_str()).collect();
        assert_eq!(names, vec!["dev", "release"]);
        assert!(environments[0].active, "dev was created last");
        assert!(!environments[1].active);
        // Distinct packages directories.
        assert_ne!(
            environments[0].packages_directory(),
            environments[1].packages_directory()
        );
    }

    #[test]
    fn activating_switches_the_install_target() {
        let scratch = Scratch::new("activate");
        create(&scratch.root, "dev").expect("creates");
        create(&scratch.root, "release").expect("creates");
        assert_eq!(
            active(&scratch.root).expect("active"),
            Some("release".to_string())
        );
        activate(&scratch.root, "dev").expect("activates");
        assert_eq!(
            active(&scratch.root).expect("active"),
            Some("dev".to_string())
        );
        assert_eq!(
            packages_directory(&scratch.root).expect("packages"),
            environments_directory(&scratch.root)
                .join("dev")
                .join(ENV_PACKAGES_DIR)
        );
    }

    #[test]
    fn removing_the_active_environment_clears_the_pointer() {
        let scratch = Scratch::new("remove");
        create(&scratch.root, "dev").expect("creates");
        let removed = remove(&scratch.root, "dev").expect("removes");
        assert!(removed.active);
        assert!(!removed.directory.exists());
        assert_eq!(active(&scratch.root).expect("active"), None);
        assert!(list(&scratch.root).expect("lists").is_empty());
        assert_eq!(
            packages_directory(&scratch.root).expect("packages"),
            default_packages_directory(&scratch.root)
        );
    }

    #[test]
    fn unknown_and_invalid_environments_are_reported() {
        let scratch = Scratch::new("errors");
        let error = environment(&scratch.root, "ghost").expect_err("unknown");
        assert_eq!(error.code(), "E-PKG13");
        assert!(error.to_string().contains("no environment named 'ghost'"));
        assert_eq!(
            environment(&scratch.root, "Bad Name")
                .expect_err("invalid")
                .code(),
            "E-PKG13"
        );
        assert_eq!(
            activate(&scratch.root, "ghost")
                .expect_err("unknown")
                .code(),
            "E-PKG13"
        );
        create(&scratch.root, "dev").expect("creates");
        let error = environment(&scratch.root, "ghost").expect_err("still unknown");
        assert!(error.to_string().contains("available: dev"), "{error}");
    }

    #[test]
    fn a_removed_environment_is_independent_of_the_others() {
        let scratch = Scratch::new("independent");
        create(&scratch.root, "a").expect("creates");
        create(&scratch.root, "b").expect("creates");
        remove(&scratch.root, "a").expect("removes");
        let names: Vec<String> = list(&scratch.root)
            .expect("lists")
            .into_iter()
            .map(|env| env.name)
            .collect();
        assert_eq!(names, vec!["b".to_string()]);
    }
}
