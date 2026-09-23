//! Deterministic dependency resolution.
//!
//! The resolver answers one question: given a project manifest, which exact
//! versions of which packages make up its dependency closure? It follows the
//! architecture doc's §21/§22 rules:
//!
//! - Candidates come from the project's configured **sources** (directories
//!   laid out `<source>/<name>/<version>/mink.toml`) and from **path
//!   dependencies**, which are pinned to the version in their own manifest.
//! - Resolution is a fixed point: every candidate's own `[dependencies]`
//!   contribute requirements, and the pass repeats until the chosen versions
//!   stop changing. Every step works on a `BTreeMap`, so the result does not
//!   depend on directory enumeration order — the same project resolves to the
//!   same versions on any machine.
//! - Within a set of requirements, the **highest** version that satisfies all
//!   of them wins (the usual conservative-maximum rule).
//! - A package whose requirements cannot all be satisfied is a **conflict**,
//!   reported with every requirement and the package that asked for it
//!   (§22 "conflict reporting").
//! - A dependency cycle is reported (§21 "cycle detection").

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};

use super::manifest::{MANIFEST_NAME, Manifest, ManifestError, normalise};
use super::version::{Requirement, Version};

/// How many fixed-point passes are attempted before giving up.
const MAX_PASSES: usize = 64;

/// Where a resolved package's sources live.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Origin {
    /// A local path dependency, recorded as written in the manifest.
    Path(String),
    /// A package directory inside a named source.
    Source(String),
}

impl Origin {
    /// The lockfile spelling (`path:...` / `local:...`).
    pub fn lock_text(&self) -> String {
        match self {
            Self::Path(text) => format!("path:{text}"),
            Self::Source(name) => format!("local:{name}"),
        }
    }
}

impl fmt::Display for Origin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Path(text) => write!(f, "path:{text}"),
            Self::Source(name) => write!(f, "local:{name}"),
        }
    }
}

/// One selectable version of a package.
#[derive(Debug, Clone)]
pub struct Candidate {
    /// The package name.
    pub name: String,
    /// The candidate version.
    pub version: Version,
    /// The directory holding the package's sources.
    pub directory: PathBuf,
    /// The candidate's own manifest.
    pub manifest: Manifest,
    /// Where the candidate came from.
    pub origin: Origin,
}

/// A package chosen by resolution.
#[derive(Debug, Clone)]
pub struct ResolvedPackage {
    /// The package name.
    pub name: String,
    /// The chosen version.
    pub version: Version,
    /// The directory holding the package's sources.
    pub directory: PathBuf,
    /// Where the package came from.
    pub origin: Origin,
    /// The names of this package's own dependencies, sorted.
    pub dependencies: Vec<String>,
}

/// The full resolution of a project's dependencies.
#[derive(Debug, Clone)]
pub struct Resolution {
    /// Every resolved package, sorted by name.
    pub packages: Vec<ResolvedPackage>,
}

impl Resolution {
    /// The resolved package named `name`.
    pub fn package(&self, name: &str) -> Option<&ResolvedPackage> {
        self.packages.iter().find(|pkg| pkg.name == name)
    }

    /// The resolved packages' directories (the module search roots).
    pub fn directories(&self) -> Vec<PathBuf> {
        self.packages
            .iter()
            .map(|pkg| pkg.directory.clone())
            .collect()
    }
}

/// A requirement together with the package that asked for it.
#[derive(Debug, Clone)]
pub struct Requested {
    /// Who asked: `"<project>"` for the root manifest, else a package name.
    pub from: String,
    /// The requirement they wrote.
    pub requirement: Requirement,
}

/// A resolution failure.
#[derive(Debug)]
pub enum ResolveError {
    /// A manifest could not be read or parsed.
    Manifest(Box<ManifestError>),
    /// A requirement cannot be satisfied by any candidate.
    NoCandidate {
        /// The package.
        name: String,
        /// The requirements that must hold.
        wanted: Vec<Requested>,
        /// The versions that exist, sorted descending.
        available: Vec<Version>,
    },
    /// Different requirements on one package cannot hold together.
    Conflict {
        /// The package.
        name: String,
        /// The requirements, in deterministic order.
        wanted: Vec<Requested>,
        /// The versions that exist, sorted descending.
        available: Vec<Version>,
    },
    /// Nothing changed for `MAX_PASSES`; the graph never settled.
    NotConverged {
        /// The package whose version kept changing.
        name: String,
    },
    /// A package depends on itself, directly or transitively.
    Cycle {
        /// The cycle, starting at its lexicographically smallest member,
        /// with the first name repeated at the end.
        chain: Vec<String>,
    },
}

impl fmt::Display for ResolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Manifest(error) => write!(f, "{error}"),
            Self::NoCandidate {
                name,
                wanted,
                available,
            } => {
                write!(
                    f,
                    "no version of '{name}' satisfies {}",
                    describe_requirements(wanted)
                )?;
                if available.is_empty() {
                    write!(f, " (no versions of '{name}' are available)")
                } else {
                    write!(f, " (available: {})", join_versions(available))
                }
            }
            Self::Conflict {
                name,
                wanted,
                available,
            } => {
                write!(
                    f,
                    "conflicting requirements for '{name}': {}",
                    describe(wanted)
                )?;
                if !available.is_empty() {
                    write!(f, " (available: {})", join_versions(available))?;
                }
                Ok(())
            }
            Self::NotConverged { name } => write!(
                f,
                "dependency resolution for '{name}' did not settle after {MAX_PASSES} passes"
            ),
            Self::Cycle { chain } => {
                write!(f, "dependency cycle: {}", chain.join(" -> "))
            }
        }
    }
}

impl std::error::Error for ResolveError {}

impl ResolveError {
    /// The stable code (`E-PKG05`…`E-PKG08`, or the wrapped manifest
    /// error's own code).
    pub fn code(&self) -> &'static str {
        match self {
            Self::Manifest(error) => error.code(),
            Self::NoCandidate { .. } => "E-PKG05",
            Self::Conflict { .. } => "E-PKG06",
            Self::NotConverged { .. } => "E-PKG07",
            Self::Cycle { .. } => "E-PKG08",
        }
    }
}

/// One requirement line in a diagnostic: `'demo' wants '^1.0.0'`.
fn describe(wanted: &[Requested]) -> String {
    match wanted {
        [] => "no requirement".to_string(),
        [request] => format!("'{}' wants '{}'", request.from, request.requirement),
        _ => wanted
            .iter()
            .map(|request| format!("'{}' wants '{}'", request.from, request.requirement))
            .collect::<Vec<_>>()
            .join(", "),
    }
}

/// One requirement line in a "no candidate" diagnostic:
/// `'^1.0.0' required by 'demo'`.
///
/// [`describe`] is used where a clause like `conflicting requirements for
/// 'x': 'a' wants '^1'` reads as a sentence; a "no version satisfies …"
/// diagnostic needs the requirement itself to lead, which is what this
/// renders.
fn describe_requirements(wanted: &[Requested]) -> String {
    match wanted {
        [] => "no requirement".to_string(),
        _ => wanted
            .iter()
            .map(|request| format!("'{}' required by '{}'", request.requirement, request.from))
            .collect::<Vec<_>>()
            .join(", "),
    }
}

/// Renders versions highest-first.
fn join_versions(versions: &[Version]) -> String {
    versions
        .iter()
        .map(Version::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

/// Resolves the dependency closure of `root`.
pub fn resolve(root: &Manifest) -> Result<Resolution, ResolveError> {
    resolve_with_pins(root, &BTreeMap::new())
}

/// Resolves the dependency closure of `root`, preferring `pins`.
///
/// A pin is the lockfile's exact version for a package. It is honored only
/// when that version exists and still satisfies every requirement; otherwise
/// resolution falls back to the highest matching version, so a stale pin
/// degrades into a normal resolution instead of an error.
pub fn resolve_with_pins(
    root: &Manifest,
    pins: &BTreeMap<String, Version>,
) -> Result<Resolution, ResolveError> {
    // Candidate discovery is cached per (name, pinned directory) so the
    // sources are read once.
    let mut candidates = CandidateSet::new(root);
    let root_pins = path_pins(root);
    let mut selected: BTreeMap<String, Candidate> = BTreeMap::new();
    for pass in 0..MAX_PASSES {
        let requirements = collect_requirements(root, &selected);
        let mut next: BTreeMap<String, Candidate> = BTreeMap::new();
        for (name, wanted) in &requirements {
            let candidates = candidates.for_requirement(name, wanted, root)?;
            let chosen = pick(name, wanted, &candidates, pins.get(name))
                .ok_or_else(|| failure(name, wanted, &candidates))?;
            next.insert(name.clone(), chosen);
        }
        if next.len() == selected.len()
            && next.iter().all(|(name, candidate)| {
                selected
                    .get(name)
                    .is_some_and(|old| old.version == candidate.version)
            })
        {
            return finish(next);
        }
        // A pass that keeps changing one package's version without settling is
        // reported rather than looping forever.
        if pass == MAX_PASSES - 1 {
            let name = next
                .iter()
                .find(|(name, candidate)| {
                    selected
                        .get(*name)
                        .is_none_or(|old| old.version != candidate.version)
                })
                .map(|(name, _)| name.clone())
                .unwrap_or_else(|| "<unknown>".to_string());
            return Err(ResolveError::NotConverged { name });
        }
        selected = next;
    }
    let _ = root_pins;
    finish(selected)
}

/// Turns the final selection into a sorted resolution and checks for cycles.
fn finish(selected: BTreeMap<String, Candidate>) -> Result<Resolution, ResolveError> {
    let mut packages: Vec<ResolvedPackage> = selected
        .into_values()
        .map(|candidate| {
            let mut dependencies: Vec<String> = candidate
                .manifest
                .dependencies
                .iter()
                .map(|dep| dep.name.clone())
                .collect();
            dependencies.sort();
            dependencies.dedup();
            ResolvedPackage {
                name: candidate.name,
                version: candidate.version,
                directory: candidate.directory,
                origin: candidate.origin,
                dependencies,
            }
        })
        .collect();
    packages.sort_by(|a, b| a.name.cmp(&b.name));
    detect_cycle(&packages)?;
    Ok(Resolution { packages })
}

/// Reports the first dependency cycle, if any.
fn detect_cycle(packages: &[ResolvedPackage]) -> Result<(), ResolveError> {
    let graph: BTreeMap<&str, &[String]> = packages
        .iter()
        .map(|pkg| (pkg.name.as_str(), pkg.dependencies.as_slice()))
        .collect();
    let mut done: BTreeSet<&str> = BTreeSet::new();
    // Deterministic roots: packages in name order.
    for package in packages {
        let mut stack: Vec<&str> = vec![package.name.as_str()];
        let mut path: Vec<String> = vec![package.name.clone()];
        let mut visiting: BTreeSet<&str> = BTreeSet::new();
        visiting.insert(package.name.as_str());
        while let Some(current) = stack.pop() {
            if done.contains(current) {
                path.pop();
                continue;
            }
            let Some(neighbours) = graph.get(current) else {
                path.pop();
                continue;
            };
            let mut advanced = false;
            for neighbour in neighbours.iter().rev() {
                if visiting.contains(neighbour.as_str()) {
                    // Found a back edge: report from the repeated node.
                    let start = path.iter().position(|name| name == neighbour).unwrap_or(0);
                    let mut chain = path[start..].to_vec();
                    chain.push(neighbour.clone());
                    return Err(ResolveError::Cycle { chain });
                }
                if done.contains(neighbour.as_str()) {
                    continue;
                }
                visiting.insert(neighbour.as_str());
                stack.push(neighbour.as_str());
                path.push(neighbour.clone());
                advanced = true;
                break;
            }
            if !advanced {
                done.insert(current);
                visiting.remove(current);
                path.pop();
            }
        }
        done.insert(package.name.as_str());
    }
    Ok(())
}

/// The requirement sets implied by the root manifest plus every currently
/// selected package.
fn collect_requirements(
    root: &Manifest,
    selected: &BTreeMap<String, Candidate>,
) -> BTreeMap<String, Vec<Requested>> {
    let mut requirements: BTreeMap<String, Vec<Requested>> = BTreeMap::new();
    for dependency in &root.dependencies {
        requirements
            .entry(dependency.name.clone())
            .or_default()
            .push(Requested {
                from: root.name.clone(),
                requirement: dependency.requirement.clone(),
            });
    }
    for (name, candidate) in selected {
        for dependency in &candidate.manifest.dependencies {
            requirements
                .entry(dependency.name.clone())
                .or_default()
                .push(Requested {
                    from: name.clone(),
                    requirement: dependency.requirement.clone(),
                });
        }
    }
    for wanted in requirements.values_mut() {
        wanted.sort_by(|a, b| {
            (a.from.as_str(), a.requirement.text.as_str())
                .cmp(&(b.from.as_str(), b.requirement.text.as_str()))
        });
        wanted.dedup_by(|a, b| a.from == b.from && a.requirement.text == b.requirement.text);
    }
    requirements
}

/// The candidate satisfying every requirement: the pinned version when the
/// pin still matches, else the highest version.
fn pick(
    name: &str,
    wanted: &[Requested],
    candidates: &[Candidate],
    pin: Option<&Version>,
) -> Option<Candidate> {
    let mut matching: Vec<&Candidate> = candidates
        .iter()
        .filter(|candidate| {
            candidate.name == name
                && wanted
                    .iter()
                    .all(|request| request.requirement.matches(&candidate.version))
        })
        .collect();
    matching.sort_by(|a, b| {
        b.version
            .cmp(&a.version)
            .then_with(|| a.origin.cmp(&b.origin))
            .then_with(|| a.directory.cmp(&b.directory))
    });
    if let Some(pin) = pin {
        if let Some(pinned) = matching.iter().find(|candidate| &candidate.version == pin) {
            return Some((*pinned).clone());
        }
    }
    matching.first().map(|candidate| (*candidate).clone())
}

/// Whether the failure is "nothing exists" or "nothing satisfies".
fn failure(name: &str, wanted: &[Requested], candidates: &[Candidate]) -> ResolveError {
    let mut available: Vec<Version> = candidates
        .iter()
        .filter(|candidate| candidate.name == name)
        .map(|candidate| candidate.version.clone())
        .collect();
    available.sort_by(|a, b| b.cmp(a));
    available.dedup();
    if available.is_empty() {
        ResolveError::NoCandidate {
            name: name.to_string(),
            wanted: wanted.to_vec(),
            available,
        }
    } else if wanted.len() > 1 {
        ResolveError::Conflict {
            name: name.to_string(),
            wanted: wanted.to_vec(),
            available,
        }
    } else {
        ResolveError::NoCandidate {
            name: name.to_string(),
            wanted: wanted.to_vec(),
            available,
        }
    }
}

/// The path-pinned dependency directories in the root manifest.
fn path_pins(root: &Manifest) -> BTreeMap<String, PathBuf> {
    let mut pins = BTreeMap::new();
    for dependency in &root.dependencies {
        if let Some(directory) = root.dependency_directory(dependency) {
            pins.insert(dependency.name.clone(), directory);
        }
    }
    pins
}

/// Candidate discovery over the project's sources.
struct CandidateSet<'a> {
    root: &'a Manifest,
    /// Resolved candidates keyed by directory.
    cache: BTreeMap<PathBuf, Option<Candidate>>,
    /// Directory listings keyed by source directory.
    listings: BTreeMap<PathBuf, Vec<String>>,
}

impl<'a> CandidateSet<'a> {
    /// A new set over `root`'s sources.
    fn new(root: &'a Manifest) -> Self {
        Self {
            root,
            cache: BTreeMap::new(),
            listings: BTreeMap::new(),
        }
    }

    /// Every candidate for `name` that could satisfy any of `wanted`.
    fn for_requirement(
        &mut self,
        name: &str,
        wanted: &[Requested],
        root: &Manifest,
    ) -> Result<Vec<Candidate>, ResolveError> {
        let mut out: Vec<Candidate> = Vec::new();
        // Path dependencies are pinned to their own manifest version.
        if let Some(dependency) = root.dependency(name) {
            if let Some(directory) = root.dependency_directory(dependency) {
                if let Some(candidate) = self.load(
                    name,
                    &directory,
                    Origin::Path(
                        dependency
                            .path
                            .as_ref()
                            .map(|path| path.to_string_lossy().replace('\\', "/"))
                            .unwrap_or_default(),
                    ),
                )? {
                    out.push(candidate);
                }
                // A path dependency is authoritative: sources are not searched
                // for the same name.
                return Ok(out);
            }
        }
        for source in &self.root.sources {
            let directory = if source.directory.is_absolute() {
                source.directory.clone()
            } else {
                normalise(&self.root.root.join(&source.directory))
            };
            let versions = self.list(&directory, name);
            for version in versions {
                let candidate_dir = directory.join(name).join(&version);
                if let Some(candidate) =
                    self.load(name, &candidate_dir, Origin::Source(source.name.clone()))?
                {
                    out.push(candidate);
                }
            }
        }
        // Ambiguity between two sources for the same name+version resolves by
        // source name (deterministic), so the same project always installs the
        // same bytes.
        out.sort_by(|a, b| {
            b.version
                .cmp(&a.version)
                .then_with(|| a.origin.cmp(&b.origin))
        });
        out.dedup_by(|a, b| a.version == b.version && a.origin == b.origin);
        let _ = wanted;
        Ok(out)
    }

    /// The version directory names inside `<source>/<name>/`, sorted.
    fn list(&mut self, source: &Path, name: &str) -> Vec<String> {
        let package_dir = source.join(name);
        if let Some(cached) = self.listings.get(&package_dir) {
            return cached.clone();
        }
        let mut versions: Vec<String> = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&package_dir) {
            for entry in entries.flatten() {
                if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                    let candidate = entry.path().join(MANIFEST_NAME);
                    if candidate.exists() {
                        versions.push(entry.file_name().to_string_lossy().into_owned());
                    }
                }
            }
        }
        versions.sort();
        self.listings.insert(package_dir, versions.clone());
        versions
    }

    /// Reads and caches the candidate in `directory`.
    fn load(
        &mut self,
        name: &str,
        directory: &Path,
        origin: Origin,
    ) -> Result<Option<Candidate>, ResolveError> {
        let key = normalise(directory);
        if let Some(cached) = self.cache.get(&key) {
            return Ok(cached.clone());
        }
        let manifest = match Manifest::load(&key) {
            Ok(manifest) => manifest,
            Err(error) => {
                self.cache.insert(key.clone(), None);
                return Err(ResolveError::Manifest(Box::new(error)));
            }
        };
        let candidate = Candidate {
            name: manifest.name.clone(),
            version: manifest.version.clone(),
            directory: key.clone(),
            manifest,
            origin,
        };
        if candidate.name != name {
            self.cache.insert(key, None);
            return Err(ResolveError::Manifest(Box::new(
                ManifestError::BadPackageName {
                    name: candidate.name,
                    path: candidate.directory.join(MANIFEST_NAME),
                },
            )));
        }
        self.cache.insert(key, Some(candidate.clone()));
        Ok(Some(candidate))
    }
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
            let root = std::env::temp_dir().join(format!(
                "mink-resolver-{tag}-{}-{unique}",
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&root);
            fs::create_dir_all(&root).expect("scratch dir");
            Self { root }
        }

        /// Writes a package into the source directory `vendor`.
        fn package(&self, vendor: &str, name: &str, version: &str, dependencies: &str) {
            let directory = self.root.join(vendor).join(name).join(version);
            fs::create_dir_all(&directory).expect("package dir");
            let body = format!(
                "[package]\nname = \"{name}\"\nversion = \"{version}\"\n\n[dependencies]\n{dependencies}"
            );
            fs::write(directory.join(MANIFEST_NAME), body).expect("manifest");
            fs::write(
                directory.join(format!("{name}.mink")),
                format!("fn {name}_value() -> Int {{ return 1; }}\n"),
            )
            .expect("source");
        }

        /// Writes the project manifest.
        fn project(&self, body: &str) {
            fs::write(self.root.join(MANIFEST_NAME), body).expect("project manifest");
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn resolve_scratch(scratch: &Scratch) -> Result<Resolution, ResolveError> {
        let manifest = Manifest::load(&scratch.root).expect("project manifest");
        resolve(&manifest)
    }

    #[test]
    fn picks_the_highest_matching_version() {
        let scratch = Scratch::new("highest");
        scratch.package("vendor", "util", "1.0.0", "");
        scratch.package("vendor", "util", "1.2.0", "");
        scratch.package("vendor", "util", "2.0.0", "");
        scratch.project(
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n\n[dependencies]\nutil = \"^1.0.0\"\n\n[sources]\nvendor = \"vendor\"\n",
        );
        let resolution = resolve_scratch(&scratch).expect("resolves");
        assert_eq!(resolution.packages.len(), 1);
        assert_eq!(resolution.packages[0].version, Version::new(1, 2, 0));
    }

    #[test]
    fn a_bare_dependency_is_compatible_not_exact() {
        let scratch = Scratch::new("bare");
        scratch.package("vendor", "util", "1.0.0", "");
        scratch.package("vendor", "util", "1.9.0", "");
        scratch.project(
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n\n[dependencies]\nutil = \"1.0.0\"\n\n[sources]\nvendor = \"vendor\"\n",
        );
        let resolution = resolve_scratch(&scratch).expect("resolves");
        assert_eq!(resolution.packages[0].version, Version::new(1, 9, 0));
    }

    #[test]
    fn transitive_dependencies_are_resolved() {
        let scratch = Scratch::new("transitive");
        scratch.package("vendor", "leaf", "1.0.0", "");
        scratch.package("vendor", "mid", "1.0.0", "leaf = \"^1.0.0\"\n");
        scratch.project(
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n\n[dependencies]\nmid = \"^1.0.0\"\n\n[sources]\nvendor = \"vendor\"\n",
        );
        let resolution = resolve_scratch(&scratch).expect("resolves");
        let names: Vec<&str> = resolution
            .packages
            .iter()
            .map(|pkg| pkg.name.as_str())
            .collect();
        assert_eq!(names, vec!["leaf", "mid"]);
        assert_eq!(
            resolution.package("mid").expect("mid").dependencies,
            vec!["leaf".to_string()]
        );
    }

    #[test]
    fn conflicting_requirements_are_reported_with_both_sides() {
        let scratch = Scratch::new("conflict");
        scratch.package("vendor", "leaf", "1.0.0", "");
        scratch.package("vendor", "leaf", "2.0.0", "");
        scratch.package("vendor", "mid", "1.0.0", "leaf = \"^2.0.0\"\n");
        scratch.project(
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n\n[dependencies]\nmid = \"^1.0.0\"\nleaf = \"^1.0.0\"\n\n[sources]\nvendor = \"vendor\"\n",
        );
        let error = resolve_scratch(&scratch).expect_err("conflict");
        assert_eq!(error.code(), "E-PKG06");
        let text = error.to_string();
        assert!(text.contains("'demo' wants '^1.0.0'"), "{text}");
        assert!(text.contains("'mid' wants '^2.0.0'"), "{text}");
        assert!(text.contains("available: 2.0.0, 1.0.0"), "{text}");
    }

    #[test]
    fn a_missing_package_is_reported() {
        let scratch = Scratch::new("missing");
        scratch.project(
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n\n[dependencies]\nghost = \"^1.0.0\"\n\n[sources]\nvendor = \"vendor\"\n",
        );
        let error = resolve_scratch(&scratch).expect_err("missing");
        assert_eq!(error.code(), "E-PKG05");
        let text = error.to_string();
        assert!(text.contains("no versions of 'ghost'"), "{text}");
        // The requirement leads the clause, so the sentence reads
        // "no version of 'ghost' satisfies '^1.0.0' required by 'demo'"
        // rather than "no version of 'ghost' satisfies 'demo' wants '^1.0.0'".
        assert!(
            text.contains("satisfies '^1.0.0' required by 'demo'"),
            "{text}"
        );
        assert!(!text.contains("wants"), "{text}");
    }

    #[test]
    fn a_cycle_is_reported() {
        let scratch = Scratch::new("cycle");
        scratch.package("vendor", "a", "1.0.0", "b = \"^1.0.0\"\n");
        scratch.package("vendor", "b", "1.0.0", "a = \"^1.0.0\"\n");
        scratch.project(
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n\n[dependencies]\na = \"^1.0.0\"\n\n[sources]\nvendor = \"vendor\"\n",
        );
        let error = resolve_scratch(&scratch).expect_err("cycle");
        assert_eq!(error.code(), "E-PKG08");
        assert!(error.to_string().contains("a -> b -> a"), "{error}");
    }

    #[test]
    fn path_dependencies_pin_their_own_version() {
        let scratch = Scratch::new("path");
        let helper = scratch.root.join("helper");
        fs::create_dir_all(&helper).expect("helper dir");
        fs::write(
            helper.join(MANIFEST_NAME),
            "[package]\nname = \"helper\"\nversion = \"0.4.0\"\n",
        )
        .expect("helper manifest");
        // The source also offers a different version; the path wins.
        scratch.package("vendor", "helper", "9.9.9", "");
        scratch.project(
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n\n[dependencies]\nhelper = { path = \"helper\" }\n\n[sources]\nvendor = \"vendor\"\n",
        );
        let resolution = resolve_scratch(&scratch).expect("resolves");
        assert_eq!(resolution.packages[0].version, Version::new(0, 4, 0));
        assert_eq!(
            resolution.packages[0].origin,
            Origin::Path("helper".to_string())
        );
    }

    #[test]
    fn resolution_is_deterministic_across_runs() {
        let scratch = Scratch::new("deterministic");
        for name in ["a", "b", "c"] {
            for version in ["1.0.0", "1.0.1", "1.1.0"] {
                scratch.package("vendor", name, version, "");
            }
        }
        scratch.package(
            "vendor",
            "top",
            "1.0.0",
            "a = \"^1.0.0\"\nb = \"^1.0.0\"\nc = \"^1.0.0\"\n",
        );
        scratch.project(
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n\n[dependencies]\ntop = \"^1.0.0\"\n\n[sources]\nvendor = \"vendor\"\n",
        );
        let first = resolve_scratch(&scratch).expect("resolves");
        for _ in 0..3 {
            let again = resolve_scratch(&scratch).expect("resolves");
            assert_eq!(first.packages.len(), again.packages.len());
            for (left, right) in first.packages.iter().zip(&again.packages) {
                assert_eq!(left.name, right.name);
                assert_eq!(left.version, right.version);
                assert_eq!(left.directory, right.directory);
            }
        }
    }

    #[test]
    fn a_pin_is_honored_when_it_still_satisfies_the_requirements() {
        let scratch = Scratch::new("pin");
        scratch.package("vendor", "util", "1.0.0", "");
        scratch.package("vendor", "util", "1.2.0", "");
        scratch.project(
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n\n[dependencies]\nutil = \"^1.0.0\"\n\n[sources]\nvendor = \"vendor\"\n",
        );
        let manifest = Manifest::load(&scratch.root).expect("manifest");
        let mut pins = BTreeMap::new();
        pins.insert("util".to_string(), Version::new(1, 0, 0));
        let pinned = resolve_with_pins(&manifest, &pins).expect("resolves");
        assert_eq!(pinned.packages[0].version, Version::new(1, 0, 0));

        // A pin the requirement excludes is ignored, not reported.
        pins.insert("util".to_string(), Version::new(9, 9, 9));
        let fallback = resolve_with_pins(&manifest, &pins).expect("resolves");
        assert_eq!(fallback.packages[0].version, Version::new(1, 2, 0));
    }

    #[test]
    fn an_empty_dependency_table_resolves_to_nothing() {
        let scratch = Scratch::new("empty");
        scratch.project("[package]\nname = \"demo\"\nversion = \"0.1.0\"\n");
        let resolution = resolve_scratch(&scratch).expect("resolves");
        assert!(resolution.packages.is_empty());
    }

    #[test]
    fn a_broken_source_manifest_is_reported() {
        let scratch = Scratch::new("broken");
        let directory = scratch.root.join("vendor").join("util").join("1.0.0");
        fs::create_dir_all(&directory).expect("dir");
        fs::write(
            directory.join(MANIFEST_NAME),
            "[package]\nname = \"util\"\n",
        )
        .expect("manifest");
        scratch.project(
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n\n[dependencies]\nutil = \"^1.0.0\"\n\n[sources]\nvendor = \"vendor\"\n",
        );
        let error = resolve_scratch(&scratch).expect_err("broken");
        assert_eq!(error.code(), "E-PKG03");
    }
}
