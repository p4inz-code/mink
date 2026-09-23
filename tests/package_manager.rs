//! Package-manager integration tests — the P04/P05/P06/P07/P08/P09 capability.
//!
//! Every test builds a **fresh external project** in the system temp
//! directory (never inside the repository) and drives the real `mink` CLI:
//! `mink init`, `mink add`, `mink install`, `mink update`, `mink remove`,
//! `mink env …`. The end-to-end tests then compile and **execute a genuine
//! Windows PE** that imports an installed package, which is the only proof
//! that the installed site-packages layout actually reaches the compiler's
//! module resolution.
//!
//! The package sources are directories laid out `<source>/<name>/<version>/`
//! (the documented V1 source package layout); a network registry is P10 in the
//! parity matrix and is out of scope here.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// A throwaway project tree in the system temp directory.
struct Project {
    root: PathBuf,
}

impl Project {
    /// Creates an empty tree named after `tag` (unique per process and call).
    fn new(tag: &str) -> Self {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let root = std::env::temp_dir().join(format!(
            "mink-pkgtest-{tag}-{}-{unique}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("project root");
        Self { root }
    }

    /// Writes `body` at `relative`, creating parent directories.
    fn write(&self, relative: &str, body: &str) {
        let path = self.root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("parent directory");
        }
        fs::write(path, body).expect("write file");
    }

    /// Reads a file relative to the project root.
    fn read(&self, relative: &str) -> String {
        fs::read_to_string(self.root.join(relative)).expect("read file")
    }

    /// Whether a path exists inside the project.
    fn exists(&self, relative: &str) -> bool {
        self.root.join(relative).exists()
    }

    /// The full path of a project-relative path.
    fn path(&self, relative: &str) -> PathBuf {
        self.root.join(relative)
    }

    /// Publishes a source package into `vendor/<name>/<version>/`.
    ///
    /// `body` is the package's module source; `dependencies` is an already
    /// formatted `[dependencies]` body (empty for none).
    fn publish(&self, name: &str, version: &str, body: &str, dependencies: &str) {
        let directory = format!("vendor/{name}/{version}");
        self.write(
            &format!("{directory}/mink.toml"),
            &format!("[package]\nname = \"{name}\"\nversion = \"{version}\"\n\n[dependencies]\n{dependencies}"),
        );
        self.write(&format!("{directory}/{name}.mink"), body);
    }

    /// Creates an application project with a `[sources]` table pointing at
    /// this tree's `vendor` directory.
    fn app(&self) -> PathBuf {
        let app = self.root.join("app");
        fs::create_dir_all(&app).expect("app directory");
        app
    }

    /// Runs `mink <args>` with `cwd` as the working directory.
    fn mink_in(&self, cwd: &Path, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_mink"))
            .args(args)
            .current_dir(cwd)
            .output()
            .expect("mink runs")
    }

    /// Runs `mink <args>` in the project root.
    fn mink(&self, args: &[&str]) -> Output {
        self.mink_in(&self.root, args)
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// Asserts a command succeeded, showing both streams when it did not.
fn assert_ok(output: &Output, what: &str) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(
        output.status.success(),
        "{what} failed with {:?}\nstdout:\n{stdout}\nstderr:\n{stderr}",
        output.status.code()
    );
    stdout
}

/// Asserts a command failed with a specific diagnostic code.
fn assert_code(output: &Output, code: &str, what: &str) {
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(
        !output.status.success(),
        "{what} should have failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stderr.contains(code),
        "{what} should report {code}\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}

/// Creates an application whose manifest points at the project's vendor
/// directory, then returns the app's directory.
fn app_with_sources(project: &Project) -> PathBuf {
    let app = project.app();
    let init = project.mink_in(&app, &["init", "--name", "demo"]);
    assert_ok(&init, "mink init");
    let manifest = fs::read_to_string(app.join("mink.toml")).expect("manifest");
    fs::write(
        app.join("mink.toml"),
        format!("{manifest}\n[sources]\nvendor = \"../vendor\"\n"),
    )
    .expect("write manifest");
    app
}

/// Inserts dependency declarations into the app's existing `[dependencies]`
/// table (a second header would be a duplicate table, which the manifest
/// reader rejects).
fn declare_dependencies(app: &Path, lines: &[&str]) {
    let path = app.join("mink.toml");
    let manifest = fs::read_to_string(&path).expect("manifest");
    let mut out = String::new();
    let mut inserted = false;
    for line in manifest.lines() {
        out.push_str(line);
        out.push('\n');
        if !inserted && line.trim() == "[dependencies]" {
            for declaration in lines {
                out.push_str(declaration);
                out.push('\n');
            }
            inserted = true;
        }
    }
    assert!(inserted, "the manifest has a [dependencies] table");
    fs::write(&path, out).expect("write manifest");
}

/// Removes a dependency declaration line from the app's manifest.
fn undeclare_dependency(app: &Path, name: &str) {
    let path = app.join("mink.toml");
    let manifest = fs::read_to_string(&path).expect("manifest");
    let declaration = format!("{name} =");
    let out: String = manifest
        .lines()
        .filter(|line| !line.trim_start().starts_with(&declaration))
        .map(|line| format!("{line}\n"))
        .collect();
    fs::write(&path, out).expect("write manifest");
}

// ---------------------------------------------------------------------------
// End-to-end: install, then compile and run a real program that imports it
// ---------------------------------------------------------------------------

#[test]
fn p05_install_then_run_imports_the_installed_package() {
    let project = Project::new("e2e");
    project.publish(
        "util",
        "1.0.0",
        "pub fn util_version() -> Int { return 10; }\n",
        "",
    );
    let app = app_with_sources(&project);
    let added = project.mink_in(&app, &["add", "util", "--version", "^1.0.0"]);
    assert_ok(&added, "mink add");
    assert!(app.join("mink.lock").is_file(), "the lock was written");
    assert!(
        app.join(".mink").join("packages").join("util").is_dir(),
        "the package was installed"
    );

    project.write(
        "app/main.mink",
        "mod util;\nuse util::util_version;\n\nfn main() {\n    rt_print_int(util_version());\n}\n",
    );
    let run = project.mink_in(&app, &["run", "main.mink"]);
    let stdout = assert_ok(&run, "mink run");
    assert_eq!(
        stdout.trim(),
        "10",
        "the program printed the package result"
    );
}

#[test]
fn p05_a_path_dependency_is_installed_and_importable() {
    let project = Project::new("pathdep");
    project.write(
        "helper/mink.toml",
        "[package]\nname = \"helper\"\nversion = \"0.5.0\"\n",
    );
    project.write(
        "helper/helper.mink",
        "pub fn helper_value() -> Int { return 7; }\n",
    );
    let app = app_with_sources(&project);
    let added = project.mink_in(&app, &["add", "helper", "--path", "../helper"]);
    assert_ok(&added, "mink add --path");
    assert!(app.join(".mink").join("packages").join("helper").is_dir());
    project.write(
        "app/main.mink",
        "mod helper;\nuse helper::helper_value;\n\nfn main() {\n    rt_print_int(helper_value());\n}\n",
    );
    let run = project.mink_in(&app, &["run", "main.mink"]);
    assert_eq!(assert_ok(&run, "mink run").trim(), "7");
}

#[test]
fn p06_install_picks_the_highest_compatible_version_and_update_moves_it() {
    let project = Project::new("highest");
    project.publish("util", "1.0.0", "pub fn v() -> Int { return 0; }\n", "");
    project.publish("util", "1.4.0", "pub fn v() -> Int { return 4; }\n", "");
    let app = app_with_sources(&project);
    assert_ok(
        &project.mink_in(&app, &["add", "util", "--version", "^1.0.0"]),
        "mink add",
    );
    let lock = fs::read_to_string(app.join("mink.lock")).expect("lock");
    assert!(lock.contains("version = \"1.4.0\""), "lock:\n{lock}");

    // A newer release appears; `install` keeps the locked version…
    project.publish("util", "1.9.0", "pub fn v() -> Int { return 9; }\n", "");
    assert_ok(&project.mink_in(&app, &["install"]), "mink install");
    let lock = fs::read_to_string(app.join("mink.lock")).expect("lock");
    assert!(
        lock.contains("version = \"1.4.0\""),
        "still pinned:\n{lock}"
    );

    // …and `update` re-resolves to it.
    assert_ok(&project.mink_in(&app, &["update"]), "mink update");
    let lock = fs::read_to_string(app.join("mink.lock")).expect("lock");
    assert!(lock.contains("version = \"1.9.0\""), "updated:\n{lock}");
}

#[test]
fn p06_transitive_dependencies_are_resolved_and_installed() {
    let project = Project::new("transitive");
    project.publish(
        "leaf",
        "1.0.0",
        "pub fn leaf_value() -> Int { return 3; }\n",
        "",
    );
    project.publish(
        "mid",
        "1.0.0",
        "pub fn mid_value() -> Int { return 4; }\n",
        "leaf = \"^1.0.0\"\n",
    );
    let app = app_with_sources(&project);
    assert_ok(
        &project.mink_in(&app, &["add", "mid", "--version", "^1.0.0"]),
        "mink add",
    );
    assert!(app.join(".mink").join("packages").join("leaf").is_dir());
    assert!(app.join(".mink").join("packages").join("mid").is_dir());
    let lock = fs::read_to_string(app.join("mink.lock")).expect("lock");
    assert!(lock.contains("name = \"leaf\""), "lock:\n{lock}");
    assert!(
        lock.contains("dependencies = [\"leaf\"]"),
        "the lock records the edge:\n{lock}"
    );

    project.write(
        "app/main.mink",
        "mod mid;\nuse mid::mid_value;\n\nfn main() {\n    rt_print_int(mid_value());\n}\n",
    );
    let run = project.mink_in(&app, &["run", "main.mink"]);
    assert_eq!(assert_ok(&run, "mink run").trim(), "4");
}

#[test]
fn p05_remove_uninstalls_what_is_no_longer_required() {
    let project = Project::new("remove");
    project.publish("util", "1.0.0", "pub fn v() -> Int { return 1; }\n", "");
    project.publish(
        "mid",
        "1.0.0",
        "pub fn m() -> Int { return 2; }\n",
        "util = \"^1.0.0\"\n",
    );
    let app = app_with_sources(&project);
    assert_ok(
        &project.mink_in(&app, &["add", "mid", "--version", "^1.0.0"]),
        "mink add",
    );
    assert!(app.join(".mink").join("packages").join("util").is_dir());

    let removed = project.mink_in(&app, &["remove", "mid"]);
    let stdout = assert_ok(&removed, "mink remove");
    assert!(stdout.contains("removed mid"), "{stdout}");
    assert!(
        stdout.contains("removed util"),
        "the leaf went too: {stdout}"
    );
    assert!(!app.join(".mink").join("packages").join("mid").exists());
    assert!(!app.join(".mink").join("packages").join("util").exists());
    let manifest = fs::read_to_string(app.join("mink.toml")).expect("manifest");
    assert!(!manifest.contains("mid ="), "manifest:\n{manifest}");
}

// ---------------------------------------------------------------------------
// The resolver's failure paths, through the CLI
// ---------------------------------------------------------------------------

#[test]
fn p06_a_missing_package_is_reported_and_changes_nothing() {
    let project = Project::new("missing");
    let app = app_with_sources(&project);
    let before = fs::read_to_string(app.join("mink.toml")).expect("manifest");
    let output = project.mink_in(&app, &["add", "ghost", "--version", "^1.0.0"]);
    assert_code(&output, "E-PKG05", "mink add ghost");
    let after = fs::read_to_string(app.join("mink.toml")).expect("manifest");
    assert_eq!(before, after, "a failed add must not edit the manifest");
    assert!(!app.join("mink.lock").exists());
}

#[test]
fn p06_conflicting_requirements_report_both_sides() {
    let project = Project::new("conflict");
    project.publish("leaf", "1.0.0", "pub fn v() -> Int { return 1; }\n", "");
    project.publish("leaf", "2.0.0", "pub fn v() -> Int { return 2; }\n", "");
    project.publish(
        "mid",
        "1.0.0",
        "pub fn m() -> Int { return 2; }\n",
        "leaf = \"^2.0.0\"\n",
    );
    let app = app_with_sources(&project);
    declare_dependencies(&app, &["mid = \"^1.0.0\"", "leaf = \"^1.0.0\""]);
    let output = project.mink_in(&app, &["install"]);
    assert_code(&output, "E-PKG06", "a conflicting install");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("'demo' wants '^1.0.0'"), "{stderr}");
    assert!(stderr.contains("'mid' wants '^2.0.0'"), "{stderr}");
}

#[test]
fn p06_a_dependency_cycle_is_reported() {
    let project = Project::new("cycle");
    project.publish(
        "a",
        "1.0.0",
        "pub fn a() -> Int { return 1; }\n",
        "b = \"^1.0.0\"\n",
    );
    project.publish(
        "b",
        "1.0.0",
        "pub fn b() -> Int { return 2; }\n",
        "a = \"^1.0.0\"\n",
    );
    let app = app_with_sources(&project);
    assert_code(
        &project.mink_in(&app, &["add", "a", "--version", "^1.0.0"]),
        "E-PKG08",
        "a cyclic install",
    );
    assert!(!app.join("mink.lock").exists());
}

#[test]
fn p04_running_outside_a_project_is_reported() {
    let project = Project::new("noproject");
    let output = project.mink(&["install"]);
    assert_code(&output, "E-PKG01", "install outside a project");
}

#[test]
fn p08_a_malformed_manifest_is_reported_with_its_location() {
    let project = Project::new("broken");
    project.write(
        "app/mink.toml",
        "[package]\nname = \"demo\"\nversion = \"not-a-version\"\n",
    );
    let output = project.mink_in(&project.path("app"), &["install"]);
    assert_code(&output, "E-PKG03", "a malformed version");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("mink.toml"), "{stderr}");
}

// ---------------------------------------------------------------------------
// Integrity and reproducibility
// ---------------------------------------------------------------------------

#[test]
fn p07_repeated_installs_are_byte_stable_and_content_checked() {
    let project = Project::new("stable");
    project.publish("util", "1.0.0", "pub fn v() -> Int { return 1; }\n", "");
    let app = app_with_sources(&project);
    assert_ok(
        &project.mink_in(&app, &["add", "util", "--version", "^1.0.0"]),
        "mink add",
    );
    let first = fs::read_to_string(app.join("mink.lock")).expect("lock");
    assert_ok(&project.mink_in(&app, &["install"]), "second install");
    let second = fs::read_to_string(app.join("mink.lock")).expect("lock");
    assert_eq!(first, second, "the lock is byte-identical across installs");

    // `--check` accepts a faithful install…
    assert_ok(&project.mink_in(&app, &["install", "--check"]), "--check");
    // …and rejects a modified one.
    project.write("app/.mink/packages/util/tampered.mink", "fn x() {}\n");
    assert_code(
        &project.mink_in(&app, &["install", "--check"]),
        "E-PKG15",
        "--check after tampering",
    );
    // A plain install repairs it.
    assert_ok(&project.mink_in(&app, &["install"]), "repair install");
    assert!(
        !app.join(".mink")
            .join("packages")
            .join("util")
            .join("tampered.mink")
            .exists()
    );
    assert_ok(
        &project.mink_in(&app, &["install", "--check"]),
        "--check after repair",
    );
}

#[test]
fn p07_a_lock_that_disagrees_with_the_manifest_is_reported() {
    let project = Project::new("stalelock");
    project.publish("util", "1.0.0", "pub fn v() -> Int { return 1; }\n", "");
    project.publish("extra", "1.0.0", "pub fn e() -> Int { return 2; }\n", "");
    let app = app_with_sources(&project);
    assert_ok(
        &project.mink_in(&app, &["add", "util", "--version", "^1.0.0"]),
        "mink add util",
    );
    // Declare the second dependency by hand so the lock no longer matches.
    declare_dependencies(&app, &["extra = \"^1.0.0\""]);
    assert_code(
        &project.mink_in(&app, &["install"]),
        "E-PKG14",
        "install with a stale lock",
    );
    let stderr = String::from_utf8_lossy(&project.mink_in(&app, &["install"]).stderr).into_owned();
    assert!(stderr.contains("mink update"), "{stderr}");
    // `update` reconciles it.
    assert_ok(&project.mink_in(&app, &["update"]), "mink update");
    assert_ok(&project.mink_in(&app, &["install", "--check"]), "--check");
    let lock = fs::read_to_string(app.join("mink.lock")).expect("lock");
    assert!(lock.contains("name = \"extra\""), "lock:\n{lock}");
}

#[test]
fn p07_an_exact_requirement_is_honored() {
    let project = Project::new("exact");
    project.publish("util", "1.0.0", "pub fn v() -> Int { return 1; }\n", "");
    project.publish("util", "1.5.0", "pub fn v() -> Int { return 2; }\n", "");
    let app = app_with_sources(&project);
    assert_ok(
        &project.mink_in(&app, &["add", "util", "--version", "=1.0.0"]),
        "mink add =1.0.0",
    );
    let lock = fs::read_to_string(app.join("mink.lock")).expect("lock");
    assert!(lock.contains("version = \"1.0.0\""), "lock:\n{lock}");
    assert_ok(
        &project.mink_in(&app, &["update"]),
        "update stays on the exact version",
    );
    let lock = fs::read_to_string(app.join("mink.lock")).expect("lock");
    assert!(lock.contains("version = \"1.0.0\""), "lock:\n{lock}");
}

// ---------------------------------------------------------------------------
// Virtual environments (P09)
// ---------------------------------------------------------------------------

#[test]
fn p09_environments_isolate_installed_packages() {
    let project = Project::new("env");
    project.publish("util", "1.0.0", "pub fn v() -> Int { return 1; }\n", "");
    let app = app_with_sources(&project);
    assert_ok(
        &project.mink_in(&app, &["add", "util", "--version", "^1.0.0"]),
        "mink add",
    );
    let project_packages = app.join(".mink").join("packages").join("util");
    assert!(project_packages.is_dir(), "the project installed util");
    let project_source = fs::read_to_string(project_packages.join("util.mink")).expect("source");

    // A fresh environment starts empty: it does not inherit the project's
    // packages directory.
    assert_ok(&project.mink_in(&app, &["env", "new", "dev"]), "env new");
    let env_packages = app.join(".mink").join("envs").join("dev").join("packages");
    assert!(
        env_packages.is_dir(),
        "the environment has its own directory"
    );
    assert!(
        !env_packages.join("util").exists(),
        "a fresh environment must not inherit the project's packages"
    );

    // Installing while the environment is active fills the environment and
    // leaves the project's copy alone.
    assert_ok(&project.mink_in(&app, &["install"]), "install into the env");
    assert!(
        env_packages.join("util").is_dir(),
        "the env got its own copy"
    );
    assert_eq!(
        fs::read_to_string(project_packages.join("util.mink")).expect("source"),
        project_source,
        "the project's copy is untouched"
    );

    // Tampering inside the environment is reported as an environment problem,
    // not a project one.
    project.write("app/.mink/envs/dev/packages/util/extra.mink", "fn x() {}\n");
    assert_code(
        &project.mink_in(&app, &["install", "--check"]),
        "E-PKG15",
        "--check inside a tampered environment",
    );

    // Removing the environment leaves the project's install intact and clean.
    assert_ok(
        &project.mink_in(&app, &["env", "remove", "dev"]),
        "env remove",
    );
    assert!(!env_packages.exists(), "the environment directory is gone");
    assert_eq!(
        fs::read_to_string(project_packages.join("util.mink")).expect("source"),
        project_source
    );
    assert_ok(
        &project.mink_in(&app, &["install", "--check"]),
        "--check back in the project",
    );
    let listed = assert_ok(&project.mink_in(&app, &["env", "list"]), "env list");
    assert!(listed.contains("no environments"), "{listed}");
}

#[test]
fn p09_an_environment_selects_the_module_search_root() {
    let project = Project::new("envbuild");
    project.publish("util", "1.0.0", "pub fn v() -> Int { return 1; }\n", "");
    project.publish("util", "2.0.0", "pub fn v() -> Int { return 2; }\n", "");
    let app = app_with_sources(&project);
    assert_ok(
        &project.mink_in(&app, &["add", "util", "--version", "=1.0.0"]),
        "mink add =1.0.0",
    );
    project.write(
        "app/main.mink",
        "mod util;\nuse util::v;\n\nfn main() {\n    rt_print_int(v());\n}\n",
    );
    let project_run = project.mink_in(&app, &["run", "main.mink"]);
    assert_eq!(assert_ok(&project_run, "project run").trim(), "1");

    // A fresh environment with 2.x wins while it is active.
    let manifest = fs::read_to_string(app.join("mink.toml")).expect("manifest");
    let upgraded = manifest.replace("=1.0.0", "^2.0.0");
    assert_ok(
        &project.mink_in(&app, &["env", "new", "upgrade"]),
        "env new",
    );
    fs::write(app.join("mink.toml"), &upgraded).expect("manifest");
    fs::remove_file(app.join("mink.lock")).expect("drop the old lock");
    assert_ok(
        &project.mink_in(&app, &["install"]),
        "install 2.x into the env",
    );
    let env_run = project.mink_in(&app, &["run", "main.mink"]);
    assert_eq!(assert_ok(&env_run, "env run").trim(), "2");

    // Deactivating restores the project's own 1.x install.
    assert_ok(
        &project.mink_in(&app, &["env", "remove", "upgrade"]),
        "env remove",
    );
    fs::write(app.join("mink.toml"), &manifest).expect("manifest");
    fs::remove_file(app.join("mink.lock")).expect("drop the lock");
    assert_ok(&project.mink_in(&app, &["install"]), "reinstall 1.x");
    let restored = project.mink_in(&app, &["run", "main.mink"]);
    assert_eq!(assert_ok(&restored, "project run again").trim(), "1");
}

#[test]
fn p09_environment_lifecycle_errors_are_reported() {
    let project = Project::new("enverrors");
    let app = app_with_sources(&project);
    assert_code(
        &project.mink_in(&app, &["env", "use", "ghost"]),
        "E-PKG13",
        "env use of an unknown environment",
    );
    assert_code(
        &project.mink_in(&app, &["env", "remove", "ghost"]),
        "E-PKG13",
        "env remove of an unknown environment",
    );
    assert_code(
        &project.mink_in(&app, &["env", "new", "Bad Name"]),
        "E-PKG13",
        "an invalid environment name",
    );
    let listed = assert_ok(&project.mink_in(&app, &["env", "list"]), "env list");
    assert!(listed.contains("no environments"), "{listed}");
}

// ---------------------------------------------------------------------------
// Shapes and edge cases
// ---------------------------------------------------------------------------

#[test]
fn p04_an_empty_project_installs_nothing_and_succeeds() {
    let project = Project::new("empty");
    let app = app_with_sources(&project);
    let output = project.mink_in(&app, &["install"]);
    let stdout = assert_ok(&output, "install with no dependencies");
    assert!(stdout.contains("0 package(s)"), "{stdout}");
    assert!(
        app.join("mink.lock").is_file(),
        "an empty lock is still a lock"
    );
    assert_ok(&project.mink_in(&app, &["install", "--check"]), "--check");
}

#[test]
fn p04_spaces_and_unicode_in_the_project_path() {
    let project = Project::new("spaces");
    project.publish("util", "1.0.0", "pub fn v() -> Int { return 5; }\n", "");
    let app = project.path("my app ünicode");
    fs::create_dir_all(&app).expect("app directory");
    // No `--name`: the default must be derived from the directory name and
    // sanitised into a legal package name (`my app ünicode` -> `my-app-nicode`).
    assert_ok(&project.mink_in(&app, &["init"]), "init in a spacey path");
    let manifest = fs::read_to_string(app.join("mink.toml")).expect("manifest");
    assert!(
        manifest.contains("name = \"my-app-nicode\""),
        "the default name is derived from the directory: {manifest}"
    );
    fs::write(
        app.join("mink.toml"),
        format!("{manifest}\n[sources]\nvendor = \"../vendor\"\n"),
    )
    .expect("manifest");
    assert_ok(
        &project.mink_in(&app, &["add", "util", "--version", "^1.0.0"]),
        "add in a spacey path",
    );
    project.write(
        "my app ünicode/main.mink",
        "mod util;\nuse util::v;\n\nfn main() {\n    rt_print_int(v());\n}\n",
    );
    let run = project.mink_in(&app, &["run", "main.mink"]);
    assert_eq!(assert_ok(&run, "run in a spacey path").trim(), "5");
}

#[test]
fn p04_init_derives_a_legal_name_in_any_directory() {
    let project = Project::new("initname");
    // Directories whose names are not legal package names must still
    // initialise without the user inventing a `--name`.
    for directory in ["My Project", "proj-测试", "π", "2024 data"] {
        let app = project.path(directory);
        fs::create_dir_all(&app).expect("app directory");
        assert_ok(
            &project.mink_in(&app, &["init"]),
            "init in a directory needing a derived name",
        );
        let manifest = fs::read_to_string(app.join("mink.toml")).expect("manifest");
        let name = manifest
            .lines()
            .find_map(|line| line.strip_prefix("name = \""))
            .and_then(|rest| rest.strip_suffix('"'))
            .unwrap_or_else(|| panic!("no package name in {manifest}"));
        assert!(
            mink::package::manifest::is_valid_package_name(name),
            "'{directory}' derived an illegal name '{name}'"
        );
    }
    // An explicitly invalid `--name` is still rejected, as typed.
    let explicit = project.path("explicit");
    fs::create_dir_all(&explicit).expect("app directory");
    assert_code(
        &project.mink_in(&explicit, &["init", "--name", "Bad Name"]),
        "E-PKG03",
        "an explicit invalid name",
    );
}

#[test]
fn p05_many_dependencies_install_deterministically() {
    let project = Project::new("many");
    for index in 0..24 {
        project.publish(
            &format!("pkg{index:02}"),
            "1.0.0",
            &format!("pub fn v() -> Int {{ return {index}; }}\n"),
            "",
        );
    }
    let app = app_with_sources(&project);
    for index in 0..24 {
        let name = format!("pkg{index:02}");
        assert_ok(
            &project.mink_in(&app, &["add", &name, "--version", "^1.0.0"]),
            "add",
        );
    }
    let first = fs::read_to_string(app.join("mink.lock")).expect("lock");
    assert_ok(&project.mink_in(&app, &["install"]), "reinstall");
    let second = fs::read_to_string(app.join("mink.lock")).expect("lock");
    assert_eq!(first, second);
    assert_eq!(
        first.matches("[[package]]").count(),
        25,
        "the project plus 24 packages"
    );
    // Every package really is on disk and reachable from a program.
    project.write(
        "app/main.mink",
        "mod pkg23;\nuse pkg23::v;\n\nfn main() {\n    rt_print_int(v());\n}\n",
    );
    let run = project.mink_in(&app, &["run", "main.mink"]);
    assert_eq!(assert_ok(&run, "run with 24 packages").trim(), "23");
}

#[test]
fn p05_update_reconciles_a_removed_dependency() {
    let project = Project::new("reconcile");
    project.publish("util", "1.0.0", "pub fn v() -> Int { return 1; }\n", "");
    let app = app_with_sources(&project);
    assert_ok(
        &project.mink_in(&app, &["add", "util", "--version", "^1.0.0"]),
        "mink add",
    );
    // Drop the dependency from the manifest by hand, then update.
    undeclare_dependency(&app, "util");
    assert_code(
        &project.mink_in(&app, &["install"]),
        "E-PKG14",
        "install with a stale lock",
    );
    let updated = project.mink_in(&app, &["update"]);
    let stdout = assert_ok(&updated, "mink update");
    assert!(stdout.contains("removed util"), "{stdout}");
    assert!(!app.join(".mink").join("packages").join("util").exists());
}

#[test]
fn p04_init_reports_an_existing_project() {
    let project = Project::new("initexists");
    let app = project.app();
    assert_ok(
        &project.mink_in(&app, &["init", "--name", "demo"]),
        "first init",
    );
    assert_code(
        &project.mink_in(&app, &["init", "--name", "demo"]),
        "E-PKG03",
        "a second init",
    );
}

#[test]
fn p08_a_path_dependency_must_really_exist() {
    let project = Project::new("badpath");
    let app = app_with_sources(&project);
    assert_code(
        &project.mink_in(&app, &["add", "helper", "--path", "../nowhere"]),
        "E-PKG01",
        "add --path with no manifest there",
    );
    let manifest = fs::read_to_string(app.join("mink.toml")).expect("manifest");
    assert!(!manifest.contains("helper"), "manifest:\n{manifest}");
}

#[test]
fn p08_declaring_both_a_version_and_a_path_is_rejected() {
    let project = Project::new("both");
    let app = app_with_sources(&project);
    assert_code(
        &project.mink_in(
            &app,
            &[
                "add",
                "helper",
                "--version",
                "^1.0.0",
                "--path",
                "../helper",
            ],
        ),
        "E-PKG03",
        "add with both forms",
    );
}
