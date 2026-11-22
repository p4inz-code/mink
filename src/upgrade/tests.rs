//! Unit tests for `mink upgrade`, against a fake network and a recording launcher.

use super::*;
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};

fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

/// A trimmed copy of the shape GitHub returns for a release.
fn release_json(tag: &str, with_checksum: bool) -> String {
    let checksum = if with_checksum {
        format!(
            r#",{{"url":"https://api.github.com/x/2","id":2,"name":"mink-Setup-{v}.exe.sha256","label":null,"uploader":{{"login":"p4inz-code","id":1}},"size":90,"browser_download_url":"https://github.com/p4inz-code/mink/releases/download/v{v}/mink-Setup-{v}.exe.sha256"}}"#,
            v = tag.trim_start_matches('v')
        )
    } else {
        String::new()
    };
    format!(
        r#"{{"url":"https://api.github.com/x","id":9,"author":{{"login":"p4inz-code","id":1}},"tag_name":"{tag}","name":"MINK {tag}","draft":false,
"assets":[{{"url":"https://api.github.com/x/1","id":1,"name":"mink-Setup-{v}.exe","label":null,"uploader":{{"login":"p4inz-code","id":1}},"size":4000000,"browser_download_url":"https://github.com/p4inz-code/mink/releases/download/v{v}/mink-Setup-{v}.exe"}}{checksum},{{"id":3,"name":"mink-{v}-portable.zip","browser_download_url":"https://github.com/p4inz-code/mink/releases/download/v{v}/mink-{v}-portable.zip"}}]}}"#,
        v = tag.trim_start_matches('v')
    )
}

struct Scripted {
    answers: VecDeque<String>,
    output: String,
    interactive: bool,
}

impl Scripted {
    fn new(answers: &[&str]) -> Self {
        Self {
            answers: answers.iter().map(|s| s.to_string()).collect(),
            output: String::new(),
            interactive: true,
        }
    }
}

impl Console for Scripted {
    fn print(&mut self, text: &str) {
        self.output.push_str(text);
    }
    fn read_line(&mut self) -> Option<String> {
        self.answers.pop_front()
    }
    fn is_interactive(&self) -> bool {
        self.interactive
    }
}

/// Serves fixed texts and writes fixed bytes for downloads.
#[derive(Default)]
struct FakeNet {
    texts: HashMap<String, String>,
    files: HashMap<String, Vec<u8>>,
    requests: RefCell<Vec<String>>,
}

impl Fetcher for FakeNet {
    fn get_text(&self, url: &str) -> Result<String, String> {
        self.requests.borrow_mut().push(url.to_string());
        self.texts.get(url).cloned().ok_or_else(|| format!("no such document {url}"))
    }
    fn download(&self, url: &str, to: &Path) -> Result<(), String> {
        self.requests.borrow_mut().push(url.to_string());
        let bytes = self.files.get(url).ok_or_else(|| format!("no such file {url}"))?;
        std::fs::write(to, bytes).map_err(|e| e.to_string())
    }
}

#[derive(Default)]
struct Recorder {
    launched: RefCell<Vec<(PathBuf, Vec<String>)>>,
}

impl Launcher for Recorder {
    fn launch(&self, installer: &Path, args: &[&str]) -> Result<(), String> {
        self.launched
            .borrow_mut()
            .push((installer.to_path_buf(), args.iter().map(|s| s.to_string()).collect()));
        Ok(())
    }
}

fn scratch(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mink_upgrade_{label}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// An installer-style layout: `<root>\mink-install.json` and `<root>\bin\mink.exe`.
fn installer_layout(label: &str) -> (PathBuf, PathBuf) {
    let root = scratch(label);
    std::fs::create_dir_all(root.join("bin")).unwrap();
    std::fs::write(root.join("mink-install.json"), "{}").unwrap();
    (root.clone(), root.join("bin").join("mink.exe"))
}

fn context(current: &str, exe: &Path, work: &Path) -> Context {
    Context {
        current: current.to_string(),
        mink_exe: exe.to_path_buf(),
        work_dir: work.to_path_buf(),
        api_override: None,
    }
}

// ------------------------------------------------------------------ arguments

#[test]
fn arguments_parse() {
    assert_eq!(parse_args(&[]).unwrap(), UpgradeArgs::default());
    let parsed = parse_args(&args(&["--check", "-y", "--to", "v1.0.5"])).unwrap();
    assert!(parsed.check && parsed.yes);
    assert_eq!(parsed.to.as_deref(), Some("1.0.5"));
    assert!(parse_args(&args(&["--to"])).is_err());
    assert!(parse_args(&args(&["--to", "soon"])).unwrap_err().contains("not a version"));
    assert!(parse_args(&args(&["--bogus"])).unwrap_err().contains("unknown option"));
}

// ------------------------------------------------------------------- the JSON

#[test]
fn json_strings_reads_values_and_decodes_escapes() {
    let json = r#"{"a": "one", "b":"two \"q\" \u0041", "a" : "three", "c": 5, "a": null}"#;
    assert_eq!(json_strings(json, "a"), vec!["one", "three"]);
    assert_eq!(json_strings(json, "b"), vec!["two \"q\" A"]);
    assert!(json_strings(json, "c").is_empty(), "numbers are not strings");
    assert!(json_strings(json, "missing").is_empty());
}

#[test]
fn a_release_is_read_with_its_assets() {
    let release = parse_release(&release_json("v1.0.5", true)).unwrap();
    assert_eq!(release.version, "1.0.5");
    assert_eq!(release.assets.len(), 3);
    assert_eq!(release.assets[0].name, "mink-Setup-1.0.5.exe");
    assert!(release.assets[0].url.ends_with("/v1.0.5/mink-Setup-1.0.5.exe"));
    let installer = find_installer(&release).unwrap();
    assert_eq!(installer.name, "mink-Setup-1.0.5.exe");
    let checksum = find_checksum(&release, installer).unwrap();
    assert_eq!(checksum.name, "mink-Setup-1.0.5.exe.sha256");
}

#[test]
fn a_release_without_a_checksum_or_installer_is_noticed() {
    let release = parse_release(&release_json("v1.0.5", false)).unwrap();
    let installer = find_installer(&release).unwrap();
    assert!(find_checksum(&release, installer).is_none());
    let bare = Release {
        version: "1.0.5".into(),
        assets: vec![Asset { name: "notes.txt".into(), url: "u".into() }],
    };
    assert!(find_installer(&bare).is_none());
}

#[test]
fn bad_release_answers_are_errors() {
    assert!(parse_release("{}").unwrap_err().contains("no tag_name"));
    assert!(parse_release(r#"{"tag_name":"nightly"}"#).unwrap_err().contains("not a version"));
}

#[test]
fn checksum_files_are_read_in_both_common_forms() {
    let digest = "ab".repeat(32);
    assert_eq!(expected_digest(&digest), Some(digest.clone()));
    assert_eq!(expected_digest(&format!("{digest}  mink-Setup-1.0.5.exe\n")), Some(digest.clone()));
    assert_eq!(expected_digest(&format!("{}\n", digest.to_uppercase())), Some(digest));
    assert_eq!(expected_digest("not a digest"), None);
    assert_eq!(expected_digest(""), None);
    assert_eq!(expected_digest(&"a".repeat(63)), None);
}

// ------------------------------------------------------------- install kinds

#[test]
fn the_install_kind_comes_from_where_mink_exe_is() {
    let npm = Path::new("C:\\Users\\a\\AppData\\Roaming\\npm\\node_modules\\@p4inz-code\\mink\\bin\\mink.exe");
    assert_eq!(install_kind(npm), InstallKind::Npm);
    let upper = Path::new("c:/Users/a/AppData/Roaming/npm/NODE_MODULES/@P4INZ-CODE/MINK/bin/mink.exe");
    assert_eq!(install_kind(upper), InstallKind::Npm);
    let (root, exe) = installer_layout("kind");
    assert_eq!(install_kind(&exe), InstallKind::Installer);
    assert_eq!(install_kind(Path::new("C:\\portable\\mink\\bin\\mink.exe")), InstallKind::Other);
    let _ = std::fs::remove_dir_all(root);
}

// --------------------------------------------------------------- the command

fn net_with(tag: &str, installer: &[u8], checksum: Option<&str>) -> FakeNet {
    let mut net = FakeNet::default();
    let v = tag.trim_start_matches('v');
    net.texts.insert(LATEST_URL.to_string(), release_json(tag, checksum.is_some()));
    let base = format!("https://github.com/p4inz-code/mink/releases/download/v{v}");
    net.files.insert(format!("{base}/mink-Setup-{v}.exe"), installer.to_vec());
    if let Some(sum) = checksum {
        net.texts.insert(format!("{base}/mink-Setup-{v}.exe.sha256"), sum.to_string());
    }
    net
}

#[test]
fn an_up_to_date_install_is_told_so() {
    let net = net_with("v1.0.5", b"x", None);
    let mut console = Scripted::new(&[]);
    let (root, exe) = installer_layout("current");
    let code = run_with(
        &UpgradeArgs::default(),
        &context("1.0.5", &exe, &root.join("dl")),
        &mut console,
        &net,
        &Recorder::default(),
    );
    assert_eq!(code, exit::OK);
    assert!(console.output.contains("1.0.5 is the newest version"));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn an_older_release_than_the_running_version_is_not_an_upgrade() {
    let net = net_with("v1.0.4", b"x", None);
    let mut console = Scripted::new(&[]);
    let (root, exe) = installer_layout("older");
    let launcher = Recorder::default();
    let code = run_with(&UpgradeArgs::default(), &context("1.0.5", &exe, &root.join("dl")), &mut console, &net, &launcher);
    assert_eq!(code, exit::OK);
    assert!(launcher.launched.borrow().is_empty());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn check_reports_without_downloading() {
    let net = net_with("v1.0.6", b"x", Some(&"0".repeat(64)));
    let mut console = Scripted::new(&[]);
    let (root, exe) = installer_layout("check");
    let launcher = Recorder::default();
    let parsed = UpgradeArgs { check: true, ..Default::default() };
    let code = run_with(&parsed, &context("1.0.5", &exe, &root.join("dl")), &mut console, &net, &launcher);
    assert_eq!(code, exit::OK);
    assert!(console.output.contains("MINK 1.0.6 is available (you have 1.0.5)"));
    assert_eq!(net.requests.borrow().len(), 1, "only the release list was fetched");
    assert!(launcher.launched.borrow().is_empty());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn an_npm_install_gets_the_command_and_nothing_is_downloaded() {
    let net = net_with("v1.0.6", b"x", None);
    let mut console = Scripted::new(&[]);
    let exe = Path::new("C:\\Users\\a\\AppData\\Roaming\\npm\\node_modules\\@p4inz-code\\mink\\bin\\mink.exe");
    let launcher = Recorder::default();
    let code = run_with(&UpgradeArgs::default(), &context("1.0.5", exe, &scratch("npm")), &mut console, &net, &launcher);
    assert_eq!(code, exit::OK);
    assert!(console.output.contains("npm install -g @p4inz-code/mink@latest"));
    assert!(launcher.launched.borrow().is_empty());
    assert_eq!(net.requests.borrow().len(), 1);
}

#[test]
fn another_kind_of_install_is_pointed_at_the_download_page() {
    let net = net_with("v1.0.6", b"x", None);
    let mut console = Scripted::new(&[]);
    let exe = Path::new("C:\\portable\\mink\\bin\\mink.exe");
    run_with(&UpgradeArgs::default(), &context("1.0.5", exe, &scratch("other")), &mut console, &net, &Recorder::default());
    assert!(console.output.contains("not put here by the installer"));
    assert!(console.output.contains("github.com/p4inz-code/mink/releases"));
}

#[test]
fn a_matching_checksum_starts_the_installer_silently() {
    let installer = b"pretend installer bytes";
    let digest = hex_digest(installer);
    let net = net_with("v1.0.6", installer, Some(&format!("{digest}  mink-Setup-1.0.6.exe")));
    let (root, exe) = installer_layout("good");
    let work = root.join("dl");
    let mut console = Scripted::new(&["y"]);
    let launcher = Recorder::default();
    let code = run_with(&UpgradeArgs::default(), &context("1.0.5", &exe, &work), &mut console, &net, &launcher);
    assert_eq!(code, exit::OK, "{}", console.output);
    let launched = launcher.launched.borrow();
    assert_eq!(launched.len(), 1);
    assert!(launched[0].0.ends_with("mink-Setup-1.0.6.exe"));
    assert_eq!(launched[0].1, vec!["--silent"]);
    assert!(console.output.contains("The checksum matches"));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_wrong_checksum_deletes_the_download_and_starts_nothing() {
    let net = net_with("v1.0.6", b"tampered", Some(&"ab".repeat(32)));
    let (root, exe) = installer_layout("bad");
    let work = root.join("dl");
    let mut console = Scripted::new(&[]);
    let launcher = Recorder::default();
    let parsed = UpgradeArgs { yes: true, ..Default::default() };
    let code = run_with(&parsed, &context("1.0.5", &exe, &work), &mut console, &net, &launcher);
    assert_eq!(code, exit::FAILED);
    assert!(console.output.contains("does not match its checksum"));
    assert!(launcher.launched.borrow().is_empty());
    assert!(!work.join("mink-Setup-1.0.6.exe").exists(), "the file is deleted");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_release_without_a_checksum_is_never_started() {
    let net = net_with("v1.0.6", b"x", None);
    let (root, exe) = installer_layout("nosum");
    let mut console = Scripted::new(&[]);
    let launcher = Recorder::default();
    let parsed = UpgradeArgs { yes: true, ..Default::default() };
    let code = run_with(&parsed, &context("1.0.5", &exe, &root.join("dl")), &mut console, &net, &launcher);
    assert_eq!(code, exit::FAILED);
    assert!(console.output.contains("no checksum"));
    assert!(launcher.launched.borrow().is_empty());
    assert!(!net.requests.borrow().iter().any(|u| u.ends_with(".exe")), "nothing was downloaded");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn declining_the_question_changes_nothing() {
    let digest = hex_digest(b"x");
    let net = net_with("v1.0.6", b"x", Some(&digest));
    let (root, exe) = installer_layout("no");
    let mut console = Scripted::new(&["n"]);
    let launcher = Recorder::default();
    let code = run_with(&UpgradeArgs::default(), &context("1.0.5", &exe, &root.join("dl")), &mut console, &net, &launcher);
    assert_eq!(code, exit::OK);
    assert!(console.output.contains("Nothing was changed."));
    assert!(launcher.launched.borrow().is_empty());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn without_a_terminal_or_yes_the_installer_is_not_started() {
    let digest = hex_digest(b"x");
    let net = net_with("v1.0.6", b"x", Some(&digest));
    let (root, exe) = installer_layout("pipe");
    let mut console = Scripted::new(&[]);
    console.interactive = false;
    let code = run_with(&UpgradeArgs::default(), &context("1.0.5", &exe, &root.join("dl")), &mut console, &net, &Recorder::default());
    assert_eq!(code, exit::USAGE);
    assert!(console.output.contains("mink upgrade --yes"));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_specific_version_asks_for_that_tag() {
    let mut net = FakeNet::default();
    net.texts.insert(format!("{TAG_URL}1.0.4"), release_json("v1.0.4", false));
    let (root, exe) = installer_layout("to");
    let mut console = Scripted::new(&[]);
    let parsed = UpgradeArgs { check: true, to: Some("1.0.4".into()), ..Default::default() };
    let code = run_with(&parsed, &context("1.0.5", &exe, &root.join("dl")), &mut console, &net, &Recorder::default());
    assert_eq!(code, exit::OK);
    assert!(console.output.contains("MINK 1.0.4 is available"), "an older version can be chosen on purpose");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn network_trouble_is_reported() {
    let net = FakeNet::default();
    let mut console = Scripted::new(&[]);
    let (root, exe) = installer_layout("down");
    let code = run_with(&UpgradeArgs::default(), &context("1.0.5", &exe, &root.join("dl")), &mut console, &net, &Recorder::default());
    assert_eq!(code, exit::FAILED);
    assert!(console.output.contains("could not read the release list"));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn an_overridden_release_address_is_used_instead_of_github() {
    let mut net = FakeNet::default();
    net.texts.insert("file:///mirror/latest.json".to_string(), release_json("v1.0.5", false));
    let (root, exe) = installer_layout("mirror");
    let mut ctx = context("1.0.5", &exe, &root.join("dl"));
    ctx.api_override = Some("file:///mirror/latest.json".to_string());
    let mut console = Scripted::new(&[]);
    let code = run_with(&UpgradeArgs::default(), &ctx, &mut console, &net, &Recorder::default());
    assert_eq!(code, exit::OK);
    assert_eq!(net.requests.borrow().as_slice(), ["file:///mirror/latest.json"]);
    let _ = std::fs::remove_dir_all(root);
}
