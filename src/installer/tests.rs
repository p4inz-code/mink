//! Tests for the installer: the engine and the flow against a temporary folder
//! and a fake machine, and the window's server against real local connections.

use super::engine::{self, Choices, InstallError, Layout, Quiet, Record};
use super::json::{self, Value};
use super::flow::{self, InstallRequest};
use super::host::{Host, UninstallEntry};
use super::manifest::{Manifest, MANIFEST_NAME};
use super::*;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::Mutex;

fn scratch(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mink_installer_{label}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// A payload folder for `version` with a fake `mink.exe`, the standard library, the kit and a manifest.
fn make_payload(dir: &Path, version: &str) -> PathBuf {
    let payload = dir.join(format!("payload-{version}"));
    let _ = fs::remove_dir_all(&payload);
    for (path, contents) in [
        ("bin/mink.exe", format!("MZ fake mink.exe {version}")),
        ("stdlib/json.mink", "fn json_x() {}".to_string()),
        ("stdlib/math.mink", "fn math_x() {}".to_string()),
        ("windows/register-mink-filetype.ps1", "# kit".to_string()),
        ("windows/mink-path.ps1", "# path".to_string()),
        ("windows/mink-app.ico", "ico".to_string()),
        ("windows/mink-file.ico", "ico".to_string()),
        ("windows/template.mink", "fn main() {}".to_string()),
        ("LICENSE", "license".to_string()),
    ] {
        let target = payload.join(path);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(target, contents).unwrap();
    }
    let manifest = Manifest::from_directory(&payload, version).unwrap();
    fs::write(payload.join(MANIFEST_NAME), manifest.to_text()).unwrap();
    payload
}

#[derive(Default)]
struct FakeHost {
    scripts: Mutex<Vec<(String, Vec<String>)>>,
    entries: Mutex<Vec<UninstallEntry>>,
    removed_entry: Mutex<bool>,
    elevated: bool,
    script_code: i32,
    script_text: String,
}

impl Host for FakeHost {
    fn run_script(&self, script: &Path, args: &[String]) -> Result<(i32, String), String> {
        let name = script.file_name().unwrap().to_string_lossy().into_owned();
        self.scripts.lock().unwrap().push((name, args.to_vec()));
        Ok((self.script_code, self.script_text.clone()))
    }
    fn write_uninstall_entry(&self, entry: &UninstallEntry) -> Result<(), String> {
        self.entries.lock().unwrap().push(entry.clone());
        Ok(())
    }
    fn remove_uninstall_entry(&self) -> Result<(), String> {
        *self.removed_entry.lock().unwrap() = true;
        Ok(())
    }
    fn is_elevated(&self) -> bool {
        self.elevated
    }
}

impl FakeHost {
    fn calls(&self, script: &str) -> Vec<Vec<String>> {
        self.scripts
            .lock()
            .unwrap()
            .iter()
            .filter(|(name, _)| name == script)
            .map(|(_, args)| args.clone())
            .collect()
    }
}

fn request(root: &Path, choices: Choices) -> InstallRequest {
    InstallRequest {
        root: root.to_path_buf(),
        choices,
        allow_elevated: false,
    }
}

// ------------------------------------------------------------------- install

#[test]
fn a_fresh_install_lays_out_every_file_and_wires_the_machine() {
    let work = scratch("fresh");
    let payload = make_payload(&work, "1.0.5");
    let root = work.join("MINK");
    let host = FakeHost::default();
    let outcome = flow::perform_install(&payload, &request(&root, Choices::defaults()), &host, &mut Quiet).unwrap();
    let layout = Layout::new(&root);

    assert_eq!(outcome.version, "1.0.5");
    assert!(outcome.warnings.is_empty(), "{:?}", outcome.warnings);
    assert!(layout.version_dir("1.0.5").join("bin").join("mink.exe").is_file());
    assert!(layout.version_dir("1.0.5").join("stdlib").join("json.mink").is_file());
    assert!(layout.version_dir("1.0.5").join("LICENSE").is_file());
    assert!(layout.version_dir("1.0.5").join(MANIFEST_NAME).is_file(), "the manifest travels with the version");
    assert!(!layout.version_dir("1.0.5").join("windows").exists(), "the kit is shared, not per version");
    assert!(layout.kit_dir().join("register-mink-filetype.ps1").is_file());
    assert!(layout.kit_dir().join("template.mink").is_file());
    assert_eq!(fs::read(layout.shim()).unwrap(), b"MZ fake mink.exe 1.0.5", "the launcher is a copy of that version");
    assert_eq!(fs::read_to_string(layout.default_path()).unwrap().trim(), "1.0.5");
    assert!(!layout.versions_dir().join("1.0.5.new").exists(), "no staging folder is left");
    assert!(!root.join("windows.new").exists());

    let record = Record::read(&layout.record_path()).unwrap();
    assert_eq!(record, outcome.record);
    assert_eq!(record.default, "1.0.5");
    assert_eq!(record.versions, vec!["1.0.5"]);
    assert!(record.path_added);

    // The machine was wired up through the kit that was just installed.
    let association = host.calls("register-mink-filetype.ps1");
    assert_eq!(association.len(), 1);
    assert_eq!(association[0][0], "-MinkExe");
    assert_eq!(association[0][1], layout.shim().display().to_string());
    assert!(association[0].contains(&"-Quiet".to_string()));
    let path = host.calls("mink-path.ps1");
    assert_eq!(path[0], vec!["-Add".to_string(), "-Directory".to_string(), layout.bin_dir().display().to_string()]);
    let entries = host.entries.lock().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].version, "1.0.5");
    assert_eq!(entries[0].uninstall_command, format!("\"{}\" installer --uninstall", layout.shim().display()));
    let _ = fs::remove_dir_all(work);
}

#[test]
fn choices_decide_which_scripts_run_and_with_what() {
    let work = scratch("choices");
    let payload = make_payload(&work, "1.0.5");
    let root = work.join("MINK");
    let host = FakeHost::default();
    let mut choices = Choices::defaults();
    choices.add_to_path = false;
    choices.context_menu = false;
    choices.new_menu = false;
    choices.editor = Some(PathBuf::from("C:\\e\\code.exe"));
    flow::perform_install(&payload, &request(&root, choices), &host, &mut Quiet).unwrap();
    assert!(host.calls("mink-path.ps1").is_empty(), "PATH is not touched");
    let args = &host.calls("register-mink-filetype.ps1")[0];
    for expected in ["-NoContextMenu", "-NoNewMenu", "-Editor", "C:\\e\\code.exe"] {
        assert!(args.contains(&expected.to_string()), "{expected} in {args:?}");
    }
    assert!(!args.contains(&"-NoPreview".to_string()));

    let work2 = scratch("nofiletype");
    let payload2 = make_payload(&work2, "1.0.5");
    let host2 = FakeHost::default();
    let mut none = Choices::defaults();
    none.file_type = false;
    none.add_to_path = false;
    flow::perform_install(&payload2, &request(&work2.join("MINK"), none), &host2, &mut Quiet).unwrap();
    assert!(host2.calls("register-mink-filetype.ps1").is_empty());
    assert!(host2.calls("mink-path.ps1").is_empty());
    assert_eq!(host2.entries.lock().unwrap().len(), 1, "Apps & features is always written");
    let _ = fs::remove_dir_all(work);
    let _ = fs::remove_dir_all(work2);
}

#[test]
fn an_upgrade_keeps_the_old_version_and_makes_the_new_one_the_default() {
    let work = scratch("upgrade");
    let root = work.join("MINK");
    let host = FakeHost::default();
    let first = make_payload(&work, "1.0.4");
    flow::perform_install(&first, &request(&root, Choices::defaults()), &host, &mut Quiet).unwrap();
    let second = make_payload(&work, "1.0.5");
    let outcome = flow::perform_install(&second, &request(&root, Choices::defaults()), &host, &mut Quiet).unwrap();
    let layout = Layout::new(&root);
    assert_eq!(outcome.upgraded_from.as_deref(), Some("1.0.4"));
    assert_eq!(engine::list_versions(&layout), vec!["1.0.5", "1.0.4"]);
    assert_eq!(engine::default_version(&layout).as_deref(), Some("1.0.5"));
    assert_eq!(fs::read(layout.shim()).unwrap(), b"MZ fake mink.exe 1.0.5", "the launcher follows the newest");
    assert_eq!(fs::read(layout.version_dir("1.0.4").join("bin").join("mink.exe")).unwrap(), b"MZ fake mink.exe 1.0.4");
    assert_eq!(outcome.record.versions, vec!["1.0.4", "1.0.5"]);

    engine::set_default(&layout, "1.0.4").unwrap();
    assert_eq!(engine::default_version(&layout).as_deref(), Some("1.0.4"));
    assert_eq!(Record::read(&layout.record_path()).unwrap().default, "1.0.4");
    assert!(engine::set_default(&layout, "9.9.9").unwrap_err().to_string().contains("not installed"));
    let _ = fs::remove_dir_all(work);
}

#[test]
fn reinstalling_the_same_version_replaces_it_cleanly() {
    let work = scratch("again");
    let payload = make_payload(&work, "1.0.5");
    let root = work.join("MINK");
    let host = FakeHost::default();
    flow::perform_install(&payload, &request(&root, Choices::defaults()), &host, &mut Quiet).unwrap();
    // Damage the installed copy, then install again.
    let layout = Layout::new(&root);
    fs::write(layout.version_dir("1.0.5").join("stdlib").join("json.mink"), "broken").unwrap();
    flow::perform_install(&payload, &request(&root, Choices::defaults()), &host, &mut Quiet).unwrap();
    assert_eq!(fs::read_to_string(layout.version_dir("1.0.5").join("stdlib").join("json.mink")).unwrap(), "fn json_x() {}");
    for leftover in ["1.0.5.new", "1.0.5.old"] {
        assert!(!layout.versions_dir().join(leftover).exists(), "{leftover}");
    }
    assert!(!layout.bin_dir().join("mink.exe.old").exists());
    let _ = fs::remove_dir_all(work);
}

// ------------------------------------------------------------------ refusals

#[test]
fn a_damaged_payload_is_refused_before_anything_is_written() {
    let work = scratch("damaged");
    let payload = make_payload(&work, "1.0.5");
    fs::write(payload.join("stdlib").join("json.mink"), "tampered").unwrap();
    let root = work.join("MINK");
    let host = FakeHost::default();
    let error = flow::perform_install(&payload, &request(&root, Choices::defaults()), &host, &mut Quiet).unwrap_err();
    assert!(matches!(error, InstallError::Payload(_)), "{error:?}");
    assert_eq!(error.exit_code(), 7);
    assert!(error.to_string().contains("json.mink"), "{error}");
    assert!(!root.exists(), "no folder was created");
    assert!(host.scripts.lock().unwrap().is_empty());
    let _ = fs::remove_dir_all(work);
}

#[test]
fn a_folder_with_other_files_is_refused_and_left_alone() {
    let work = scratch("foreign");
    let payload = make_payload(&work, "1.0.5");
    let root = work.join("Documents");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("my-notes.txt"), "mine").unwrap();
    let error = flow::perform_install(&payload, &request(&root, Choices::defaults()), &FakeHost::default(), &mut Quiet).unwrap_err();
    assert!(matches!(error, InstallError::Folder(_)));
    assert_eq!(error.exit_code(), 6);
    assert!(error.to_string().contains("already holds other files"));
    let names: Vec<_> = fs::read_dir(&root).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert_eq!(names.len(), 1, "nothing was added");
    assert_eq!(fs::read_to_string(root.join("my-notes.txt")).unwrap(), "mine");

    assert!(engine::check_folder(Path::new("C:\\")).is_err(), "a drive root is not a place to install");
    let file = work.join("a-file");
    fs::write(&file, "x").unwrap();
    assert!(engine::check_folder(&file).unwrap_err().to_string().contains("is a file"));
    assert!(engine::check_folder(&work.join("new")).unwrap().is_none());
    let empty = work.join("empty");
    fs::create_dir_all(&empty).unwrap();
    assert!(engine::check_folder(&empty).unwrap().is_none());
    let _ = fs::remove_dir_all(work);
}

#[test]
fn running_as_an_administrator_is_refused_unless_allowed() {
    let work = scratch("elevated");
    let payload = make_payload(&work, "1.0.5");
    let root = work.join("MINK");
    let host = FakeHost {
        elevated: true,
        ..Default::default()
    };
    let error = flow::perform_install(&payload, &request(&root, Choices::defaults()), &host, &mut Quiet).unwrap_err();
    assert_eq!(error, InstallError::Elevated);
    assert_eq!(error.exit_code(), 3);
    assert!(!root.exists());
    let mut allowed = request(&root, Choices::defaults());
    allowed.allow_elevated = true;
    assert!(flow::perform_install(&payload, &allowed, &host, &mut Quiet).is_ok());
    let _ = fs::remove_dir_all(work);
}

#[test]
fn a_failed_copy_leaves_the_previous_install_working() {
    let work = scratch("rollback");
    let root = work.join("MINK");
    let host = FakeHost::default();
    let first = make_payload(&work, "1.0.4");
    flow::perform_install(&first, &request(&root, Choices::defaults()), &host, &mut Quiet).unwrap();
    let layout = Layout::new(&root);
    let before = fs::read(layout.shim()).unwrap();
    // The second install cannot stage because a file sits where its staging folder must go.
    let second = make_payload(&work, "1.0.5");
    fs::write(layout.versions_dir().join("1.0.5.new"), "in the way").unwrap();
    let error = flow::perform_install(&second, &request(&root, Choices::defaults()), &host, &mut Quiet).unwrap_err();
    assert!(matches!(error, InstallError::Io(_) | InstallError::Locked(_)), "{error:?}");
    assert_eq!(fs::read(layout.shim()).unwrap(), before, "the launcher is untouched");
    assert_eq!(engine::default_version(&layout).as_deref(), Some("1.0.4"));
    assert_eq!(engine::list_versions(&layout), vec!["1.0.4"]);
    let _ = fs::remove_dir_all(work);
}

#[test]
fn script_failures_become_warnings_not_a_failed_install() {
    let work = scratch("warn");
    let payload = make_payload(&work, "1.0.5");
    let host = FakeHost {
        script_code: 1,
        script_text: "owned by Contoso".to_string(),
        ..Default::default()
    };
    let outcome = flow::perform_install(&payload, &request(&work.join("MINK"), Choices::defaults()), &host, &mut Quiet).unwrap();
    assert_eq!(outcome.warnings.len(), 2, "{:?}", outcome.warnings);
    assert!(outcome.warnings[0].contains("file type was not set up"));
    assert!(outcome.warnings[0].contains("owned by Contoso"));
    assert!(outcome.warnings[1].contains("PATH was not changed"));
    assert!(Layout::new(work.join("MINK")).shim().is_file(), "the files are installed");
    let _ = fs::remove_dir_all(work);
}

#[test]
fn a_path_entry_that_was_already_there_is_not_ours_to_remove() {
    let work = scratch("pathold");
    let payload = make_payload(&work, "1.0.5");
    let root = work.join("MINK");
    let host = FakeHost {
        script_text: "C:\\x is already on your PATH.".to_string(),
        ..Default::default()
    };
    let outcome = flow::perform_install(&payload, &request(&root, Choices::defaults()), &host, &mut Quiet).unwrap();
    assert!(!outcome.record.path_added);
    // An earlier install of ours put it there; reinstalling keeps owning it.
    let layout = Layout::new(&root);
    let mut record = Record::read(&layout.record_path()).unwrap();
    record.path_added = true;
    record.write(&layout.record_path()).unwrap();
    let again = flow::perform_install(&payload, &request(&root, Choices::defaults()), &host, &mut Quiet).unwrap();
    assert!(again.record.path_added);
    let _ = fs::remove_dir_all(work);
}

// ----------------------------------------------------------------- uninstall

#[test]
fn uninstall_removes_what_was_installed_and_only_that() {
    let work = scratch("remove");
    let payload = make_payload(&work, "1.0.5");
    let root = work.join("MINK");
    let host = FakeHost::default();
    flow::perform_install(&payload, &request(&root, Choices::defaults()), &host, &mut Quiet).unwrap();
    let second = make_payload(&work, "1.0.6");
    flow::perform_install(&second, &request(&root, Choices::defaults()), &host, &mut Quiet).unwrap();
    // A file of the user's, dropped into the folder afterwards, must survive.
    fs::write(root.join("notes.txt"), "keep me").unwrap();

    let host = FakeHost::default();
    let warnings = flow::perform_uninstall(&root, &host, &mut Quiet).unwrap();
    assert!(warnings.is_empty());
    let layout = Layout::new(&root);
    assert!(!layout.versions_dir().exists() && !layout.bin_dir().exists() && !layout.kit_dir().exists());
    assert!(!layout.record_path().exists() && !layout.default_path().exists());
    assert_eq!(fs::read_to_string(root.join("notes.txt")).unwrap(), "keep me");
    assert!(root.exists(), "the folder stays because something else is in it");
    assert!(host.calls("register-mink-filetype.ps1")[0].contains(&"-Uninstall".to_string()));
    assert_eq!(host.calls("mink-path.ps1")[0][0], "-Remove");
    assert!(*host.removed_entry.lock().unwrap());
    let _ = fs::remove_dir_all(work);
}

#[test]
fn uninstall_removes_the_folder_when_nothing_else_is_in_it() {
    let work = scratch("removeall");
    let payload = make_payload(&work, "1.0.5");
    let root = work.join("MINK");
    flow::perform_install(&payload, &request(&root, Choices::defaults()), &FakeHost::default(), &mut Quiet).unwrap();
    flow::perform_uninstall(&root, &FakeHost::default(), &mut Quiet).unwrap();
    assert!(!root.exists());
    let _ = fs::remove_dir_all(work);
}

#[test]
fn uninstall_leaves_the_path_alone_when_it_was_never_changed() {
    let work = scratch("nopath");
    let payload = make_payload(&work, "1.0.5");
    let root = work.join("MINK");
    let mut choices = Choices::defaults();
    choices.add_to_path = false;
    flow::perform_install(&payload, &request(&root, choices), &FakeHost::default(), &mut Quiet).unwrap();
    let host = FakeHost::default();
    flow::perform_uninstall(&root, &host, &mut Quiet).unwrap();
    assert!(host.calls("mink-path.ps1").is_empty());
    let _ = fs::remove_dir_all(work);
}

#[test]
fn uninstall_refuses_a_folder_that_is_not_an_install() {
    let work = scratch("notinstall");
    fs::write(work.join("a.txt"), "x").unwrap();
    let error = flow::perform_uninstall(&work, &FakeHost::default(), &mut Quiet).unwrap_err();
    assert!(error.to_string().contains("does not hold a MINK install"));
    assert_eq!(fs::read_to_string(work.join("a.txt")).unwrap(), "x");
    let _ = fs::remove_dir_all(work);
}

// ------------------------------------------------------------------ arguments

fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

#[test]
fn installer_arguments_parse() {
    let parsed = parse_args(&args(&[
        "--silent", "--dir", "C:\\M", "--payload", "D:\\p", "--editor", "notepad", "--no-association",
        "--no-context-menu", "--no-new-menu", "--no-preview", "--no-default-apps", "--no-path", "--log", "x.log",
    ]))
    .unwrap();
    assert!(parsed.silent && parsed.no_association && parsed.no_path);
    assert_eq!(parsed.dir, Some(PathBuf::from("C:\\M")));
    let choices = choices_from(&parsed);
    assert!(!choices.file_type && !choices.context_menu && !choices.new_menu);
    assert!(!choices.preview && !choices.default_apps && !choices.add_to_path);
    assert_eq!(choices.editor, None);
    assert!(parse_args(&args(&["/S"])).unwrap().silent, "the usual installer switch works");
    assert!(parse_args(&args(&["--bogus"])).unwrap_err().contains("unknown option"));
    assert!(parse_args(&args(&["--dir"])).unwrap_err().contains("missing value"));
    assert!(parse_args(&args(&["--port", "x"])).is_err());
    let editor = parse_args(&args(&["--editor", "C:\\e\\code.exe"])).unwrap();
    assert_eq!(choices_from(&editor).editor, Some(PathBuf::from("C:\\e\\code.exe")));
}

#[test]
fn make_manifest_writes_a_manifest_the_installer_accepts() {
    let work = scratch("makemanifest");
    let payload = make_payload(&work, "1.0.5");
    fs::remove_file(payload.join(MANIFEST_NAME)).unwrap();
    let parsed = parse_args(&args(&["--make-manifest", payload.to_str().unwrap(), "--version", "2.0.0"])).unwrap();
    assert_eq!(run(&parsed, Arc::new(FakeHost::default())), 0);
    let manifest = flow::read_manifest(&payload).unwrap();
    assert_eq!(manifest.version, "2.0.0");
    assert_eq!(manifest.files.len(), 9);
    let missing_version = parse_args(&args(&["--make-manifest", payload.to_str().unwrap()])).unwrap();
    assert_eq!(run(&missing_version, Arc::new(FakeHost::default())), 2);
    let _ = fs::remove_dir_all(work);
}

// --------------------------------------------------------------- the window

struct Response {
    status: u16,
    headers: String,
    body: String,
}

fn http(port: u16, method: &str, path: &str, headers: &[(&str, &str)], body: &str) -> Response {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    let mut request = format!("{method} {path} HTTP/1.1\r\n");
    let mut has_host = false;
    for (name, value) in headers {
        if name.eq_ignore_ascii_case("host") {
            has_host = true;
        }
        request.push_str(&format!("{name}: {value}\r\n"));
    }
    if !has_host {
        request.push_str(&format!("Host: 127.0.0.1:{port}\r\n"));
    }
    request.push_str(&format!("Content-Length: {}\r\n\r\n{body}", body.len()));
    stream.write_all(request.as_bytes()).unwrap();
    let mut raw = String::new();
    stream.read_to_string(&mut raw).unwrap();
    let (head, body) = raw.split_once("\r\n\r\n").unwrap_or((&raw, ""));
    let status = head.split_whitespace().nth(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    Response {
        status,
        headers: head.to_string(),
        body: body.to_string(),
    }
}

struct Running {
    port: u16,
    token: String,
    handle: std::thread::JoinHandle<u8>,
}

fn start_server(task: server::Task, payload: Option<PathBuf>, root: PathBuf, idle_ms: u64) -> Running {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let token = server::make_token();
    let config = server::Config {
        token: token.clone(),
        version: "1.0.5".to_string(),
        task,
        payload,
        payload_size: 12_345_678,
        default_root: root.clone(),
        existing: None,
        uninstall_root: Some(root),
        editors: vec![crate::setup::Editor {
            name: "Visual Studio Code".to_string(),
            path: PathBuf::from("C:\\vs\\Code.exe"),
        }],
        on_path: false,
        allow_elevated: false,
        idle_limit: Duration::from_millis(idle_ms),
    };
    let handle = std::thread::spawn(move || server::serve(config, Arc::new(FakeHost::default()), listener));
    Running { port, token, handle }
}

fn api<'a>(running: &'a Running) -> Vec<(&'a str, &'a str)> {
    vec![("X-Mink-Token", running.token.as_str())]
}

#[test]
fn the_page_needs_its_token_and_the_right_host() {
    let work = scratch("page");
    let running = start_server(server::Task::Install, None, work.join("MINK"), 20_000);
    let p = running.port;

    let no_token = http(p, "GET", "/", &[], "");
    assert_eq!(no_token.status, 403);
    let wrong = http(p, "GET", "/?t=nope", &[], "");
    assert_eq!(wrong.status, 403);
    let page = http(p, "GET", &format!("/?t={}", running.token), &[], "");
    assert_eq!(page.status, 200);
    assert!(page.body.contains("MINK Setup"));
    assert!(page.body.contains(&running.token), "the token is placed in the page");
    assert!(page.headers.contains("Content-Security-Policy"), "{}", page.headers);
    assert!(page.headers.contains("default-src 'none'"));
    assert!(!page.body.contains("__TOKEN__"));

    // A page served under another name (DNS rebinding) is refused.
    let rebound = http(p, "GET", &format!("/?t={}", running.token), &[("Host", "evil.example:80")], "");
    assert_eq!(rebound.status, 403);

    for asset in ["/installer.css", "/installer.js", "/logo.svg"] {
        let response = http(p, "GET", asset, &[], "");
        assert_eq!(response.status, 200, "{asset}");
        assert!(!response.body.is_empty());
    }
    assert_eq!(http(p, "GET", "/nothing", &[], "").status, 404);

    http(p, "POST", "/api/finish", &api(&running), "{}");
    assert_eq!(running.handle.join().unwrap(), 8, "closing before anything happened is a cancel");
    let _ = fs::remove_dir_all(work);
}

#[test]
fn the_api_needs_the_token() {
    let work = scratch("token");
    let running = start_server(server::Task::Install, None, work.join("MINK"), 20_000);
    let p = running.port;
    assert_eq!(http(p, "GET", "/api/state", &[], "").status, 403);
    assert_eq!(http(p, "GET", "/api/state", &[("X-Mink-Token", "wrong")], "").status, 403);
    assert_eq!(http(p, "POST", "/api/install", &[], "{}").status, 403);
    let state = http(p, "GET", "/api/state", &api(&running), "");
    assert_eq!(state.status, 200);
    let value = json::parse(&state.body).unwrap();
    assert_eq!(value.get("version").and_then(|v| v.as_str()), Some("1.0.5"));
    assert_eq!(value.get("task").and_then(|v| v.as_str()), Some("install"));
    assert_eq!(value.get("sizeBytes"), Some(&Value::Number(12_345_678.0)));
    assert_eq!(value.get("elevated").and_then(|v| v.as_bool()), Some(false));
    http(p, "POST", "/api/finish", &api(&running), "{}");
    running.handle.join().unwrap();
    let _ = fs::remove_dir_all(work);
}

#[test]
fn an_install_through_the_window_runs_and_reports_progress() {
    let work = scratch("window");
    let payload = make_payload(&work, "1.0.5");
    let root = work.join("MINK");
    let running = start_server(server::Task::Install, Some(payload), root.clone(), 20_000);
    let p = running.port;
    let body = Value::object(vec![
        ("dir", Value::text(root.display().to_string())),
        ("fileType", Value::Bool(true)),
        ("contextMenu", Value::Bool(true)),
        ("newMenu", Value::Bool(false)),
        ("preview", Value::Bool(true)),
        ("defaultApps", Value::Bool(true)),
        ("addToPath", Value::Bool(false)),
        ("editor", Value::text("C:\\vs\\Code.exe")),
    ])
    .to_json();
    let started = http(p, "POST", "/api/install", &api(&running), &body);
    assert_eq!(started.status, 200, "{}", started.body);

    let mut last = Value::Null;
    for _ in 0..200 {
        let progress = http(p, "GET", "/api/progress", &api(&running), "");
        last = json::parse(&progress.body).unwrap();
        if last.get("state").and_then(|s| s.as_str()) == Some("done") {
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    assert_eq!(last.get("state").and_then(|s| s.as_str()), Some("done"), "{}", last.to_json());
    assert_eq!(last.get("percent"), Some(&Value::Number(100.0)));
    let summary = last.get("summary").unwrap();
    assert_eq!(summary.get("version").and_then(|v| v.as_str()), Some("1.0.5"));
    assert!(Layout::new(&root).shim().is_file());
    let record = Record::read(&Layout::new(&root).record_path()).unwrap();
    assert!(!record.choices.new_menu && record.choices.context_menu);
    assert_eq!(record.choices.editor, Some(PathBuf::from("C:\\vs\\Code.exe")));

    http(p, "POST", "/api/finish", &api(&running), "{}");
    assert_eq!(running.handle.join().unwrap(), 0);
    let _ = fs::remove_dir_all(work);
}

#[test]
fn the_window_refuses_bad_requests_without_touching_the_disk() {
    let work = scratch("badreq");
    let payload = make_payload(&work, "1.0.5");
    let root = work.join("MINK");
    let running = start_server(server::Task::Install, Some(payload), root.clone(), 20_000);
    let p = running.port;
    let post = |body: &str| http(p, "POST", "/api/install", &api(&running), body);
    assert_eq!(post("not json").status, 400);
    assert_eq!(post("{}").status, 400);
    let relative = post(r#"{"dir":"MINK","fileType":true}"#);
    assert_eq!(relative.status, 400);
    assert!(relative.body.contains("full path"), "{}", relative.body);
    let parent = post(&format!(r#"{{"dir":"{}\\..\\x","fileType":true}}"#, work.display().to_string().replace('\\', "\\\\")));
    assert_eq!(parent.status, 400);
    assert!(parent.body.contains(".."), "{}", parent.body);
    let editor = post(&format!(
        r#"{{"dir":"{}","fileType":true,"editor":"C:\\Windows\\System32\\cmd.exe"}}"#,
        root.display().to_string().replace('\\', "\\\\")
    ));
    assert_eq!(editor.status, 400, "an editor the installer did not find is refused");
    assert!(!root.exists());
    http(p, "POST", "/api/finish", &api(&running), "{}");
    running.handle.join().unwrap();
    let _ = fs::remove_dir_all(work);
}

#[test]
fn a_failed_install_through_the_window_reports_the_reason() {
    let work = scratch("windowfail");
    let payload = make_payload(&work, "1.0.5");
    fs::write(payload.join("LICENSE"), "tampered").unwrap();
    let root = work.join("MINK");
    let running = start_server(server::Task::Install, Some(payload), root.clone(), 20_000);
    let p = running.port;
    let body = format!(r#"{{"dir":"{}","fileType":false}}"#, root.display().to_string().replace('\\', "\\\\"));
    assert_eq!(http(p, "POST", "/api/install", &api(&running), &body).status, 200);
    let mut last = Value::Null;
    for _ in 0..200 {
        last = json::parse(&http(p, "GET", "/api/progress", &api(&running), "").body).unwrap();
        if last.get("state").and_then(|s| s.as_str()) == Some("error") {
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    assert_eq!(last.get("state").and_then(|s| s.as_str()), Some("error"));
    assert!(last.get("error").and_then(|e| e.as_str()).unwrap().contains("LICENSE"));
    http(p, "POST", "/api/finish", &api(&running), "{}");
    assert_eq!(running.handle.join().unwrap(), 7, "the exit code is the damaged-payload code");
    let _ = fs::remove_dir_all(work);
}

#[test]
fn a_removal_through_the_window_works() {
    let work = scratch("windowremove");
    let payload = make_payload(&work, "1.0.5");
    let root = work.join("MINK");
    flow::perform_install(&payload, &request(&root, Choices::defaults()), &FakeHost::default(), &mut Quiet).unwrap();
    let running = start_server(server::Task::Uninstall, None, root.clone(), 20_000);
    let p = running.port;
    assert_eq!(http(p, "POST", "/api/uninstall", &api(&running), "{}").status, 200);
    let mut last = Value::Null;
    for _ in 0..200 {
        last = json::parse(&http(p, "GET", "/api/progress", &api(&running), "").body).unwrap();
        if last.get("state").and_then(|s| s.as_str()) == Some("done") {
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    assert_eq!(last.get("state").and_then(|s| s.as_str()), Some("done"));
    assert!(!root.exists());
    http(p, "POST", "/api/finish", &api(&running), "{}");
    assert_eq!(running.handle.join().unwrap(), 0);
    let _ = fs::remove_dir_all(work);
}

#[test]
fn a_window_that_never_connects_or_goes_quiet_is_a_cancel() {
    let work = scratch("quiet");
    // Never connects: the first wait is three times the idle limit.
    let never = start_server(server::Task::Install, None, work.join("MINK"), 100);
    assert_eq!(never.handle.join().unwrap(), 8);
    // Connects, pings once, then goes quiet.
    let running = start_server(server::Task::Install, None, work.join("MINK2"), 200);
    http(running.port, "POST", "/api/ping", &api(&running), "{}");
    assert_eq!(running.handle.join().unwrap(), 8);
    let _ = fs::remove_dir_all(work);
}

#[test]
fn oversized_and_malformed_requests_do_not_hang_the_server() {
    let work = scratch("hostile");
    let running = start_server(server::Task::Install, None, work.join("MINK"), 20_000);
    let p = running.port;
    // A body larger than the limit is dropped.
    let big = "x".repeat(70_000);
    let mut stream = TcpStream::connect(("127.0.0.1", p)).unwrap();
    let _ = stream.write_all(
        format!("POST /api/install HTTP/1.1\r\nHost: 127.0.0.1:{p}\r\nX-Mink-Token: {}\r\nContent-Length: {}\r\n\r\n{big}", running.token, big.len())
            .as_bytes(),
    );
    let mut sink = String::new();
    let _ = stream.read_to_string(&mut sink);
    // Garbage instead of HTTP.
    let mut garbage = TcpStream::connect(("127.0.0.1", p)).unwrap();
    let _ = garbage.write_all(b"\x00\x01\x02 not http at all\r\n\r\n");
    let mut sink = String::new();
    let _ = garbage.read_to_string(&mut sink);
    // The server still answers.
    assert_eq!(http(p, "GET", "/api/state", &api(&running), "").status, 200);
    http(p, "POST", "/api/finish", &api(&running), "{}");
    running.handle.join().unwrap();
    let _ = fs::remove_dir_all(work);
}

#[test]
fn tokens_differ_between_calls() {
    let a = server::make_token();
    let b = server::make_token();
    assert_eq!(a.len(), 64);
    assert_ne!(a, b);
    assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn version_names_cannot_leave_the_versions_folder() {
    use crate::installer::engine::is_version_name;
    for good in ["1.0.5", "1.0.5-beta.1", "2.0.0+build_7", "v1"] {
        assert!(is_version_name(good), "{good}");
    }
    for bad in ["", ".", "..", "..\\..\\evil", "../x", "a\\b", "C:\\x", ".hidden", "1.0 5", "a;b", &"9".repeat(65)] {
        assert!(!is_version_name(bad), "{bad:?} must be refused");
    }
}

#[test]
fn a_manifest_with_an_unsafe_version_is_refused() {
    let digest = "0".repeat(64);
    let text = format!("mink-payload 1\nversion ..\\..\\evil\n{digest} 1 a.txt\n");
    let err = crate::installer::manifest::Manifest::parse(&text).unwrap_err();
    assert!(err.contains("unsafe version"), "{err}");
}

#[test]
fn a_hostile_version_file_never_reaches_the_launcher_path() {
    use crate::installer::engine::{default_version, Layout};
    let dir = std::env::temp_dir().join(format!("mink_hostile_ver_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("default-version.txt"), "..\\..\\evil\n").unwrap();
    assert_eq!(default_version(&Layout::new(&dir)), None);
    let _ = std::fs::remove_dir_all(&dir);
}
