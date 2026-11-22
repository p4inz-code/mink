//! The installer's window.
//!
//! The window is a page served by this module on `127.0.0.1` and shown in a
//! browser app window (Microsoft Edge's `--app` mode, which has no tabs or
//! address bar). The page asks for the installer's state, sends the user's
//! choices and polls for progress. Nothing else is reachable: the server listens
//! on the loopback address only, every request must carry the exact `Host` the
//! server expects (which defeats DNS rebinding), and every call to `/api/` must
//! carry a random token that only the page it served knows.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::engine::{Choices, InstallError, Progress, Record};
use super::flow::{self, InstallRequest};
use super::host::Host;
use super::json::{self, Value};
use crate::setup::Editor;

/// The page.
pub const INDEX_HTML: &str = include_str!("../../assets/installer/index.html");
/// Its styles.
pub const APP_CSS: &str = include_str!("../../assets/installer/installer.css");
/// Its script.
pub const APP_JS: &str = include_str!("../../assets/installer/installer.js");
/// The white MINK symbol, shown faintly behind the setup window.
pub const MARK_SVG: &str = include_str!("../../assets/brand/logo/symbol/mink-symbol-white.svg");
/// The fonts, bundled so setup looks the same on every PC and never uses a system font.
const FONTS: &[(&str, &[u8])] = &[
    ("AtkinsonHyperlegibleNext-Regular.ttf", include_bytes!("../../assets/installer/fonts/AtkinsonHyperlegibleNext-Regular.ttf")),
    ("AtkinsonHyperlegibleNext-Medium.ttf", include_bytes!("../../assets/installer/fonts/AtkinsonHyperlegibleNext-Medium.ttf")),
    ("AtkinsonHyperlegibleNext-SemiBold.ttf", include_bytes!("../../assets/installer/fonts/AtkinsonHyperlegibleNext-SemiBold.ttf")),
    ("AtkinsonHyperlegibleNext-Bold.ttf", include_bytes!("../../assets/installer/fonts/AtkinsonHyperlegibleNext-Bold.ttf")),
    ("FamiljenGroteskwght.ttf", include_bytes!("../../assets/installer/fonts/FamiljenGroteskwght.ttf")),
];
/// The compact logo shown in the side panel.
pub const LOGO_SVG: &str = include_str!("../../assets/brand/logo/compact/mink-logo-compact-white.svg");

const MAX_BODY: usize = 64 * 1024;
const CSP: &str = "default-src 'none'; style-src 'self'; script-src 'self'; img-src 'self'; font-src 'self'; connect-src 'self'; base-uri 'none'; form-action 'none'";

/// What the window is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Task {
    /// Install or upgrade MINK.
    Install,
    /// Remove MINK.
    Uninstall,
}

/// Everything the server needs to know.
pub struct Config {
    /// The secret the page must send with every API call.
    pub token: String,
    /// The version being installed (or, when removing, the installed one).
    pub version: String,
    /// What the window does.
    pub task: Task,
    /// The payload folder when installing.
    pub payload: Option<PathBuf>,
    /// The size of the files to install, in bytes (0 when removing).
    pub payload_size: u64,
    /// The default install folder.
    pub default_root: PathBuf,
    /// An install that is already there: its folder and record.
    pub existing: Option<(PathBuf, Record)>,
    /// The install folder to remove, when uninstalling.
    pub uninstall_root: Option<PathBuf>,
    /// Editors found on this PC.
    pub editors: Vec<Editor>,
    /// Whether the folder holding `mink.exe` is already on the PATH.
    pub on_path: bool,
    /// Allow running as administrator.
    pub allow_elevated: bool,
    /// How long the page may stay silent before the installer treats the window as closed.
    pub idle_limit: Duration,
}

#[derive(Clone, Default)]
struct ProgressView {
    state: &'static str,
    percent: u8,
    log: Vec<String>,
    error: Option<String>,
    warnings: Vec<String>,
    summary: Option<Value>,
}

struct Shared {
    progress: ProgressView,
    last_ping: Option<Instant>,
    finished: bool,
    code: u8,
}

struct Sink(Arc<Mutex<Shared>>);

impl Progress for Sink {
    fn update(&mut self, percent: u8, message: &str) {
        if let Ok(mut shared) = self.0.lock() {
            shared.progress.percent = percent;
            if shared.progress.log.last().map(String::as_str) != Some(message) {
                shared.progress.log.push(message.to_string());
            }
        }
    }
}

struct Request {
    method: String,
    path: String,
    query: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Request {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

fn read_request(stream: &mut TcpStream) -> Option<Request> {
    let mut reader = BufReader::new(stream.try_clone().ok()?);
    let mut line = String::new();
    reader.read_line(&mut line).ok()?;
    let mut parts = line.split_whitespace();
    let method = parts.next()?.to_string();
    let target = parts.next()?.to_string();
    let (path, query) = match target.split_once('?') {
        Some((p, q)) => (p.to_string(), q.to_string()),
        None => (target, String::new()),
    };
    let mut headers = Vec::new();
    for _ in 0..64 {
        let mut header = String::new();
        reader.read_line(&mut header).ok()?;
        let header = header.trim_end();
        if header.is_empty() {
            break;
        }
        let (name, value) = header.split_once(':')?;
        headers.push((name.trim().to_string(), value.trim().to_string()));
    }
    let length: usize = headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, v)| v.parse().ok())
        .unwrap_or(0);
    if length > MAX_BODY {
        return None;
    }
    let mut body = vec![0u8; length];
    reader.read_exact(&mut body).ok()?;
    Some(Request {
        method,
        path,
        query,
        headers,
        body,
    })
}

fn respond(stream: &mut TcpStream, status: &str, content_type: &str, body: &[u8], page: bool) {
    let mut head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nReferrer-Policy: no-referrer\r\nConnection: close\r\n",
        body.len()
    );
    if page {
        head.push_str(&format!("Content-Security-Policy: {CSP}\r\n"));
    }
    head.push_str("\r\n");
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body);
    let _ = stream.flush();
}

fn json_reply(stream: &mut TcpStream, status: &str, value: &Value) {
    respond(stream, status, "application/json; charset=utf-8", value.to_json().as_bytes(), false);
}

fn error_reply(stream: &mut TcpStream, status: &str, message: &str) {
    json_reply(stream, status, &Value::object(vec![("error", Value::text(message))]));
}

struct Server {
    config: Config,
    host: Arc<dyn Host + Send + Sync>,
    shared: Arc<Mutex<Shared>>,
    port: u16,
}

impl Server {
    fn host_ok(&self, request: &Request) -> bool {
        let Some(host) = request.header("host") else {
            return false;
        };
        host == format!("127.0.0.1:{}", self.port) || host == format!("localhost:{}", self.port)
    }

    fn token_ok(&self, request: &Request) -> bool {
        request.header("x-mink-token") == Some(self.config.token.as_str())
    }

    fn page(&self) -> String {
        INDEX_HTML.replace("__TOKEN__", &self.config.token)
    }

    fn state(&self) -> Value {
        let existing = match &self.config.existing {
            Some((dir, record)) => Value::object(vec![
                ("dir", Value::text(dir.display().to_string())),
                ("version", Value::text(&record.default)),
                (
                    "versions",
                    Value::List(record.versions.iter().map(Value::text).collect()),
                ),
            ]),
            None => Value::Null,
        };
        let editors = self
            .config
            .editors
            .iter()
            .map(|e| {
                Value::object(vec![
                    ("name", Value::text(&e.name)),
                    ("path", Value::text(e.path.display().to_string())),
                ])
            })
            .collect();
        Value::object(vec![
            ("task", Value::text(if self.config.task == Task::Install { "install" } else { "uninstall" })),
            ("version", Value::text(&self.config.version)),
            ("sizeBytes", Value::Number(self.config.payload_size as f64)),
            ("defaultDir", Value::text(self.config.default_root.display().to_string())),
            ("existing", existing),
            ("editors", Value::List(editors)),
            ("onPath", Value::Bool(self.config.on_path)),
            ("elevated", Value::Bool(self.host.is_elevated() && !self.config.allow_elevated)),
        ])
    }

    fn progress(&self) -> Value {
        let shared = self.shared.lock().expect("state lock");
        let p = &shared.progress;
        Value::object(vec![
            ("state", Value::text(if p.state.is_empty() { "idle" } else { p.state })),
            ("percent", Value::Number(p.percent as f64)),
            ("log", Value::List(p.log.iter().map(Value::text).collect())),
            ("error", p.error.as_ref().map(Value::text).unwrap_or(Value::Null)),
            ("warnings", Value::List(p.warnings.iter().map(Value::text).collect())),
            ("summary", p.summary.clone().unwrap_or(Value::Null)),
        ])
    }

    fn choices_from(&self, body: &std::collections::BTreeMap<String, Value>) -> Result<(PathBuf, Choices), String> {
        let dir = body
            .get("dir")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|d| !d.is_empty())
            .ok_or("choose an install folder")?;
        let dir = PathBuf::from(dir.trim_matches('"'));
        if !dir.is_absolute() {
            return Err("the install folder must be a full path such as C:\\Users\\you\\MINK".to_string());
        }
        if dir.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
            return Err("the install folder cannot contain '..'".to_string());
        }
        let flag = |name: &str| body.get(name).and_then(Value::as_bool).unwrap_or(false);
        let file_type = flag("fileType");
        let editor = match body.get("editor").and_then(Value::as_str).filter(|e| !e.is_empty()) {
            None | Some("notepad") => None,
            Some(path) => {
                // Only an editor this installer found can be chosen from the page.
                let known = self
                    .config
                    .editors
                    .iter()
                    .find(|e| e.path.display().to_string() == path)
                    .map(|e| e.path.clone());
                Some(known.ok_or("that editor was not found on this PC")?)
            }
        };
        Ok((
            dir,
            Choices {
                file_type,
                context_menu: file_type && flag("contextMenu"),
                new_menu: file_type && flag("newMenu"),
                preview: file_type && flag("preview"),
                default_apps: file_type && flag("defaultApps"),
                add_to_path: flag("addToPath"),
                editor: if file_type { editor } else { None },
            },
        ))
    }

    fn start_install(&self, body: &std::collections::BTreeMap<String, Value>) -> Result<(), String> {
        let Some(payload) = self.config.payload.clone() else {
            return Err("this window has no files to install".to_string());
        };
        let (root, choices) = self.choices_from(body)?;
        {
            let mut shared = self.shared.lock().expect("state lock");
            if shared.progress.state == "running" {
                return Err("an install is already running".to_string());
            }
            shared.progress = ProgressView {
                state: "running",
                ..Default::default()
            };
        }
        let request = InstallRequest {
            root: root.clone(),
            choices,
            allow_elevated: self.config.allow_elevated,
        };
        let shared = Arc::clone(&self.shared);
        let host = Arc::clone(&self.host);
        std::thread::spawn(move || {
            let mut sink = Sink(Arc::clone(&shared));
            let result = flow::perform_install(&payload, &request, host.as_ref(), &mut sink);
            let mut state = shared.lock().expect("state lock");
            match result {
                Ok(outcome) => {
                    state.progress.state = "done";
                    state.progress.percent = 100;
                    state.progress.warnings = outcome.warnings.clone();
                    state.progress.summary = Some(Value::object(vec![
                        ("version", Value::text(&outcome.version)),
                        ("dir", Value::text(root.display().to_string())),
                        ("bin", Value::text(root.join("bin").display().to_string())),
                        (
                            "upgradedFrom",
                            outcome.upgraded_from.as_ref().map(Value::text).unwrap_or(Value::Null),
                        ),
                    ]));
                    state.code = 0;
                }
                Err(error) => fail(&mut state, &error),
            }
        });
        Ok(())
    }

    fn start_uninstall(&self) -> Result<(), String> {
        let Some(root) = self.config.uninstall_root.clone() else {
            return Err("this window has nothing to remove".to_string());
        };
        {
            let mut shared = self.shared.lock().expect("state lock");
            if shared.progress.state == "running" {
                return Err("a removal is already running".to_string());
            }
            shared.progress = ProgressView {
                state: "running",
                ..Default::default()
            };
        }
        let shared = Arc::clone(&self.shared);
        let host = Arc::clone(&self.host);
        std::thread::spawn(move || {
            let mut sink = Sink(Arc::clone(&shared));
            let result = flow::perform_uninstall(&root, host.as_ref(), &mut sink);
            let mut state = shared.lock().expect("state lock");
            match result {
                Ok(warnings) => {
                    state.progress.state = "done";
                    state.progress.percent = 100;
                    state.progress.warnings = warnings;
                    state.code = 0;
                }
                Err(error) => fail(&mut state, &error),
            }
        });
        Ok(())
    }

    fn handle(&self, stream: &mut TcpStream) {
        let Some(request) = read_request(stream) else {
            respond(stream, "400 Bad Request", "text/plain", b"bad request", false);
            return;
        };
        if !self.host_ok(&request) {
            respond(stream, "403 Forbidden", "text/plain", b"wrong host", false);
            return;
        }
        match (request.method.as_str(), request.path.as_str()) {
            ("GET", "/") => {
                if request.query != format!("t={}", self.config.token) {
                    respond(stream, "403 Forbidden", "text/plain", b"open the installer from its own window", false);
                } else {
                    respond(stream, "200 OK", "text/html; charset=utf-8", self.page().as_bytes(), true);
                }
            }
            ("GET", "/installer.css") => respond(stream, "200 OK", "text/css; charset=utf-8", APP_CSS.as_bytes(), false),
            ("GET", "/installer.js") => {
                respond(stream, "200 OK", "text/javascript; charset=utf-8", APP_JS.as_bytes(), false)
            }
            ("GET", path) if path.starts_with("/fonts/") => {
                match FONTS.iter().find(|(name, _)| path["/fonts/".len()..] == **name) {
                    Some((_, bytes)) => respond(stream, "200 OK", "font/ttf", bytes, false),
                    None => respond(stream, "404 Not Found", "text/plain", b"not found", false),
                }
            }
            ("GET", "/mark.svg") => respond(stream, "200 OK", "image/svg+xml", MARK_SVG.as_bytes(), false),
            ("GET", "/logo.svg") => respond(stream, "200 OK", "image/svg+xml", LOGO_SVG.as_bytes(), false),
            (_, path) if path.starts_with("/api/") => {
                if !self.token_ok(&request) {
                    error_reply(stream, "403 Forbidden", "missing or wrong token");
                    return;
                }
                self.api(stream, &request);
            }
            _ => respond(stream, "404 Not Found", "text/plain", b"not found", false),
        }
    }

    fn api(&self, stream: &mut TcpStream, request: &Request) {
        match (request.method.as_str(), request.path.as_str()) {
            ("GET", "/api/state") => json_reply(stream, "200 OK", &self.state()),
            ("GET", "/api/progress") => json_reply(stream, "200 OK", &self.progress()),
            ("POST", "/api/ping") => {
                self.shared.lock().expect("state lock").last_ping = Some(Instant::now());
                json_reply(stream, "200 OK", &Value::object(vec![("ok", Value::Bool(true))]));
            }
            ("POST", "/api/install") => {
                let body = std::str::from_utf8(&request.body).ok().and_then(json::parse_object);
                match body.ok_or_else(|| "the request was not understood".to_string()).and_then(|b| self.start_install(&b)) {
                    Ok(()) => json_reply(stream, "200 OK", &Value::object(vec![("ok", Value::Bool(true))])),
                    Err(why) => error_reply(stream, "400 Bad Request", &why),
                }
            }
            ("POST", "/api/uninstall") => match self.start_uninstall() {
                Ok(()) => json_reply(stream, "200 OK", &Value::object(vec![("ok", Value::Bool(true))])),
                Err(why) => error_reply(stream, "400 Bad Request", &why),
            },
            ("POST", "/api/finish") => {
                {
                    let mut shared = self.shared.lock().expect("state lock");
                    shared.finished = true;
                }
                json_reply(stream, "200 OK", &Value::object(vec![("ok", Value::Bool(true))]));
            }
            _ => error_reply(stream, "404 Not Found", "unknown call"),
        }
    }
}

fn fail(state: &mut Shared, error: &InstallError) {
    state.progress.state = "error";
    state.progress.error = Some(error.to_string());
    state.code = error.exit_code();
}

/// Serves the window until the page calls `/api/finish`, the page goes quiet, or
/// nothing connects in time. Returns the exit code: 0 when the work finished,
/// the error's code when it failed, 8 when the window was closed first.
pub fn serve(config: Config, host: Arc<dyn Host + Send + Sync>, listener: TcpListener) -> u8 {
    let port = listener.local_addr().map(|a| a.port()).unwrap_or(0);
    let idle = config.idle_limit;
    let shared = Arc::new(Mutex::new(Shared {
        progress: ProgressView::default(),
        last_ping: None,
        finished: false,
        code: 8,
    }));
    let server = Arc::new(Server {
        config,
        host,
        shared: Arc::clone(&shared),
        port,
    });
    let _ = listener.set_nonblocking(true);
    let started = Instant::now();
    loop {
        match listener.accept() {
            Ok((mut stream, _)) => {
                let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
                let _ = stream.set_nonblocking(false);
                let server = Arc::clone(&server);
                std::thread::spawn(move || server.handle(&mut stream));
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(_) => break,
        }
        let state = shared.lock().expect("state lock");
        if state.finished {
            return state.code;
        }
        let quiet_for = match state.last_ping {
            Some(at) => at.elapsed(),
            None => started.elapsed(),
        };
        // Edge takes a while to start, so the first wait is longer than the gap allowed between pings.
        let limit = if state.last_ping.is_some() { idle } else { idle * 3 };
        if quiet_for > limit && state.progress.state != "running" {
            // The window was closed (or never opened).
            return match state.progress.state {
                "done" => 0,
                "error" => state.code,
                _ => 8,
            };
        }
    }
    8
}

/// A random-looking token. It only has to be unguessable from another local program
/// for the few minutes the installer runs.
pub fn make_token() -> String {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    let mut out = String::new();
    for round in 0..4u32 {
        let mut hasher = RandomState::new().build_hasher();
        hasher.write_u32(round);
        hasher.write_u128(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0),
        );
        hasher.write_u32(std::process::id());
        out.push_str(&format!("{:016x}", hasher.finish()));
    }
    out
}

/// Opens the window in Microsoft Edge's app mode. Returns false when Edge cannot be found.
pub fn open_window(port: u16, token: &str, title_dir: &Path) -> bool {
    let candidates = [
        std::env::var("ProgramFiles(x86)").ok().map(|p| PathBuf::from(p).join("Microsoft\\Edge\\Application\\msedge.exe")),
        std::env::var("ProgramFiles").ok().map(|p| PathBuf::from(p).join("Microsoft\\Edge\\Application\\msedge.exe")),
        std::env::var("LOCALAPPDATA").ok().map(|p| PathBuf::from(p).join("Microsoft\\Edge\\Application\\msedge.exe")),
    ];
    let Some(edge) = candidates.into_iter().flatten().find(|p| p.is_file()) else {
        return false;
    };
    let profile = title_dir.join("window-profile");
    std::process::Command::new(edge)
        .arg(format!("--app=http://127.0.0.1:{port}/?t={token}"))
        .arg("--window-size=1040,720")
        .arg(format!("--user-data-dir={}", profile.display()))
        .args(["--no-first-run", "--no-default-browser-check", "--disable-extensions", "--disable-sync"])
        .spawn()
        .is_ok()
}
