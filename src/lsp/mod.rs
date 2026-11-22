//! `mink lsp`: the MINK language server.
//!
//! It speaks the Language Server Protocol over standard input and output, so any editor
//! with an LSP client can use it. The compiler itself produces the diagnostics, run on
//! the editor's unsaved text, and a token-level index of every loaded file answers
//! completion, hover, go to definition, references, rename, outline, signature help and
//! formatting. Because the index never needs the code to parse, those still work while
//! the file is half typed.
//!
//! One thing to know: each open file is checked on its own, as the root of a build.
//! A module that only makes sense through the file that loads it (it calls names
//! declared by its parent) may show errors that disappear when the parent is checked.

use std::collections::{HashMap, HashSet};
use std::io::{BufRead, Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use crate::installer::json::{self, Value};

pub mod analysis;
pub mod features;
pub mod position;
pub mod scan;

use analysis::{analyze, path_to_uri, uri_to_path, Analysis, IndexCache};
use position::{Lines, Position};

/// The usage text of `mink lsp`.
pub const USAGE: &str = "\
usage: mink lsp [--stdio]

  Starts the MINK language server on standard input and output. An editor starts it;
  you do not normally run it yourself. Diagnostics come from the compiler, run on the
  editor's unsaved text. Supported: completion, hover, go to definition, references,
  rename, outline, signature help and formatting.";

/// A request failed in a way the client should show as a message.
const REQUEST_FAILED: i64 = -32803;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;

struct Doc {
    uri: String,
    text: String,
    #[allow(dead_code)]
    version: i64,
}

/// The state of one language server session.
#[derive(Default)]
pub struct Server {
    docs: HashMap<PathBuf, Doc>,
    client_uris: HashMap<PathBuf, String>,
    cache: IndexCache,
    analyses: HashMap<String, Analysis>,
    /// Addresses that currently have diagnostics on the client's screen.
    published: HashSet<String>,
    shutdown: bool,
    exit_code: Option<i32>,
}

fn response(id: &Value, result: Value) -> Value {
    Value::object(vec![("jsonrpc", Value::text("2.0")), ("id", id.clone()), ("result", result)])
}

fn error_response(id: &Value, code: i64, message: &str) -> Value {
    Value::object(vec![
        ("jsonrpc", Value::text("2.0")),
        ("id", id.clone()),
        (
            "error",
            Value::object(vec![("code", Value::Number(code as f64)), ("message", Value::text(message))]),
        ),
    ])
}

fn notification(method: &str, params: Value) -> Value {
    Value::object(vec![("jsonrpc", Value::text("2.0")), ("method", Value::text(method)), ("params", params)])
}

fn capabilities() -> Value {
    let on = Value::Bool(true);
    Value::object(vec![
        (
            "capabilities",
            Value::object(vec![
                ("positionEncoding", Value::text("utf-16")),
                (
                    "textDocumentSync",
                    Value::object(vec![
                        ("openClose", on.clone()),
                        ("change", Value::Number(1.0)),
                        ("save", Value::object(vec![("includeText", Value::Bool(false))])),
                    ]),
                ),
                (
                    "completionProvider",
                    Value::object(vec![(
                        "triggerCharacters",
                        Value::List(vec![Value::text(":"), Value::text(".")]),
                    )]),
                ),
                ("hoverProvider", on.clone()),
                ("definitionProvider", on.clone()),
                ("referencesProvider", on.clone()),
                ("documentSymbolProvider", on.clone()),
                ("renameProvider", Value::object(vec![("prepareProvider", on.clone())])),
                (
                    "signatureHelpProvider",
                    Value::object(vec![(
                        "triggerCharacters",
                        Value::List(vec![Value::text("("), Value::text(",")]),
                    )]),
                ),
                ("documentFormattingProvider", on),
            ]),
        ),
        (
            "serverInfo",
            Value::object(vec![("name", Value::text("mink")), ("version", Value::text(env!("CARGO_PKG_VERSION")))]),
        ),
    ])
}

fn text_of<'a>(params: &'a Value, path: &[&str]) -> Option<&'a str> {
    let mut at = params;
    for key in path {
        at = at.get(key)?;
    }
    at.as_str()
}

fn number_of(params: &Value, path: &[&str]) -> Option<i64> {
    let mut at = params;
    for key in path {
        at = at.get(key)?;
    }
    match at {
        Value::Number(n) => Some(*n as i64),
        _ => None,
    }
}

impl Server {
    /// A new session.
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether the client has asked the server to exit, and the process exit code to use.
    pub fn exit_code(&self) -> Option<i32> {
        self.exit_code
    }

    /// Handles one incoming message and returns everything to send back.
    pub fn handle(&mut self, message: &Value) -> Vec<Value> {
        let Some(method) = message.get("method").and_then(Value::as_str) else {
            // A response to a request the server made; it makes none.
            return Vec::new();
        };
        let id = message.get("id");
        let params = message.get("params").cloned().unwrap_or(Value::Null);
        match (method, id) {
            ("initialize", Some(id)) => vec![response(id, capabilities())],
            ("initialized", _) => Vec::new(),
            ("shutdown", Some(id)) => {
                self.shutdown = true;
                vec![response(id, Value::Null)]
            }
            ("exit", _) => {
                self.exit_code = Some(if self.shutdown { 0 } else { 1 });
                Vec::new()
            }
            ("textDocument/didOpen", _) => self.did_open(&params),
            ("textDocument/didChange", _) => self.did_change(&params),
            ("textDocument/didSave", _) => self.did_save(&params),
            ("textDocument/didClose", _) => self.did_close(&params),
            ("$/cancelRequest" | "$/setTrace" | "workspace/didChangeConfiguration" | "workspace/didChangeWatchedFiles", _) => {
                Vec::new()
            }
            (method, Some(id)) => self.request(method, id, &params),
            _ => Vec::new(),
        }
    }

    // ---------------------------------------------------------------- documents

    fn doc_path(&self, uri: &str) -> Option<PathBuf> {
        let path = uri_to_path(uri)?;
        // Prefer the exact path an open document was registered under.
        self.docs
            .keys()
            .find(|p| p.as_os_str().eq_ignore_ascii_case(path.as_os_str()))
            .cloned()
            .or(Some(path))
    }

    fn did_open(&mut self, params: &Value) -> Vec<Value> {
        let (Some(uri), Some(text)) = (text_of(params, &["textDocument", "uri"]), text_of(params, &["textDocument", "text"])) else {
            return Vec::new();
        };
        let version = number_of(params, &["textDocument", "version"]).unwrap_or(0);
        let Some(path) = uri_to_path(uri) else { return Vec::new() };
        self.client_uris.insert(path.clone(), uri.to_string());
        self.docs.insert(
            path,
            Doc {
                uri: uri.to_string(),
                text: text.to_string(),
                version,
            },
        );
        self.check(uri)
    }

    fn did_change(&mut self, params: &Value) -> Vec<Value> {
        let Some(uri) = text_of(params, &["textDocument", "uri"]) else { return Vec::new() };
        let Some(Value::List(changes)) = params.get("contentChanges") else { return Vec::new() };
        // The server asked for whole-document sync, so the last change is the new text.
        let Some(text) = changes.iter().rev().find_map(|c| c.get("text").and_then(Value::as_str)) else {
            return Vec::new();
        };
        let version = number_of(params, &["textDocument", "version"]).unwrap_or(0);
        let Some(path) = self.doc_path(uri) else { return Vec::new() };
        self.client_uris.insert(path.clone(), uri.to_string());
        self.docs.insert(
            path,
            Doc {
                uri: uri.to_string(),
                text: text.to_string(),
                version,
            },
        );
        self.check(uri)
    }

    fn did_save(&mut self, params: &Value) -> Vec<Value> {
        match text_of(params, &["textDocument", "uri"]) {
            Some(uri) if self.doc_path(uri).is_some_and(|p| self.docs.contains_key(&p)) => self.check(uri),
            _ => Vec::new(),
        }
    }

    fn did_close(&mut self, params: &Value) -> Vec<Value> {
        let Some(uri) = text_of(params, &["textDocument", "uri"]) else { return Vec::new() };
        if let Some(path) = self.doc_path(uri) {
            self.docs.remove(&path);
            self.client_uris.remove(&path);
        }
        self.analyses.remove(uri);
        // Take the document's own problems off the screen.
        let mut out = Vec::new();
        if self.published.remove(uri) {
            out.push(diagnostics_message(uri, &[], &analysis::Analysis::default()));
        }
        out
    }

    /// Checks one open document and returns the diagnostics to publish.
    fn check(&mut self, uri: &str) -> Vec<Value> {
        let Some(path) = self.doc_path(uri) else { return Vec::new() };
        let overlays: Vec<(PathBuf, String)> = self.docs.iter().map(|(p, d)| (p.clone(), d.text.clone())).collect();
        let run = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            analyze(&path, &overlays, &self.client_uris, &mut self.cache)
        }));
        let analysis = match run {
            Ok(analysis) => analysis,
            Err(_) => {
                // The compiler should never panic, but a language server must never die from it.
                let mut analysis = Analysis::default();
                analysis.diagnostics.insert(
                    uri.to_string(),
                    vec![analysis::Diag {
                        range: (0, 0),
                        code: "E-INTERNAL".to_string(),
                        message: "the compiler stopped while checking this file; this is a bug, please report it".to_string(),
                        related: None,
                    }],
                );
                analysis
            }
        };
        let mut out = Vec::new();
        for (target, list) in &analysis.diagnostics {
            // Publish every file the check touched that has problems, had them before (so they get
            // cleared) or is the document itself.
            if !list.is_empty() || self.published.contains(target) || target == uri {
                out.push(diagnostics_message(target, list, &analysis));
            }
        }
        for (target, list) in &analysis.diagnostics {
            if list.is_empty() {
                self.published.remove(target);
            } else {
                self.published.insert(target.clone());
            }
        }
        self.analyses.insert(uri.to_string(), analysis);
        out
    }

    // ----------------------------------------------------------------- requests

    fn request(&mut self, method: &str, id: &Value, params: &Value) -> Vec<Value> {
        let Some(uri) = text_of(params, &["textDocument", "uri"]) else {
            return vec![error_response(id, METHOD_NOT_FOUND, &format!("unsupported request {method}"))];
        };
        // Make sure there is a fresh analysis to answer from.
        if !self.analyses.contains_key(uri) {
            let _ = self.check(uri);
        }
        let Some(analysis) = self.analyses.get(uri) else {
            return vec![response(id, Value::Null)];
        };
        let Some(file) = analysis.file(uri) else {
            return vec![response(id, Value::Null)];
        };
        let position = params.get("position").map(|p| Position {
            line: number_of(p, &["line"]).unwrap_or(0).max(0) as u32,
            character: number_of(p, &["character"]).unwrap_or(0).max(0) as u32,
        });
        let offset = position.map(|p| Lines::new(&file.text).offset(p));
        let result = match (method, offset) {
            ("textDocument/completion", Some(o)) => Ok(features::completion(analysis, file, o)),
            ("textDocument/hover", Some(o)) => Ok(features::hover(analysis, file, o)),
            ("textDocument/definition", Some(o)) => Ok(features::definition(analysis, file, o)),
            ("textDocument/references", Some(o)) => {
                let include = params
                    .get("context")
                    .and_then(|c| c.get("includeDeclaration"))
                    .and_then(Value::as_bool)
                    .unwrap_or(true);
                Ok(features::references(analysis, file, o, include))
            }
            ("textDocument/prepareRename", Some(o)) => features::prepare_rename(analysis, file, o),
            ("textDocument/rename", Some(o)) => match text_of(params, &["newName"]) {
                Some(name) => features::rename(analysis, file, o, name),
                None => return vec![error_response(id, INVALID_PARAMS, "rename needs a newName")],
            },
            ("textDocument/signatureHelp", Some(o)) => Ok(features::signature_help(analysis, file, o)),
            ("textDocument/documentSymbol", _) => Ok(features::document_symbols(file)),
            ("textDocument/formatting", _) => features::formatting(file),
            _ => return vec![error_response(id, METHOD_NOT_FOUND, &format!("unsupported request {method}"))],
        };
        match result {
            Ok(value) => vec![response(id, value)],
            Err(message) => vec![error_response(id, REQUEST_FAILED, &message)],
        }
    }
}

fn diagnostics_message(uri: &str, list: &[analysis::Diag], analysis: &Analysis) -> Value {
    let file = analysis.file(uri);
    let lines = file.map(|f| Lines::new(&f.text));
    let range = |a: usize, b: usize| -> Value {
        let (start, end) = match &lines {
            Some(lines) => (lines.position(a), lines.position(b.max(a))),
            None => (Position { line: 0, character: 0 }, Position { line: 0, character: 0 }),
        };
        Value::object(vec![
            (
                "start",
                Value::object(vec![("line", Value::Number(start.line as f64)), ("character", Value::Number(start.character as f64))]),
            ),
            (
                "end",
                Value::object(vec![("line", Value::Number(end.line as f64)), ("character", Value::Number(end.character as f64))]),
            ),
        ])
    };
    let items: Vec<Value> = list
        .iter()
        .map(|d| {
            let mut pairs = vec![
                ("range", range(d.range.0, d.range.1)),
                ("severity", Value::Number(1.0)),
                ("code", Value::text(&d.code)),
                ("source", Value::text("mink")),
                ("message", Value::text(&d.message)),
            ];
            if let Some((related_uri, (a, b), note)) = &d.related {
                let related_lines = analysis.file(related_uri).map(|f| Lines::new(&f.text));
                let (start, end) = match &related_lines {
                    Some(l) => (l.position(*a), l.position(*b)),
                    None => (Position { line: 0, character: 0 }, Position { line: 0, character: 0 }),
                };
                let loc = Value::object(vec![
                    ("uri", Value::text(related_uri)),
                    (
                        "range",
                        Value::object(vec![
                            (
                                "start",
                                Value::object(vec![
                                    ("line", Value::Number(start.line as f64)),
                                    ("character", Value::Number(start.character as f64)),
                                ]),
                            ),
                            (
                                "end",
                                Value::object(vec![
                                    ("line", Value::Number(end.line as f64)),
                                    ("character", Value::Number(end.character as f64)),
                                ]),
                            ),
                        ]),
                    ),
                ]);
                pairs.push((
                    "relatedInformation",
                    Value::List(vec![Value::object(vec![("location", loc), ("message", Value::text(note))])]),
                ));
            }
            Value::object(pairs)
        })
        .collect();
    notification(
        "textDocument/publishDiagnostics",
        Value::object(vec![("uri", Value::text(uri)), ("diagnostics", Value::List(items))]),
    )
}

// ------------------------------------------------------------------ the transport

/// Reads one framed message: `Content-Length: N`, a blank line, then N bytes of JSON.
/// Returns `None` at end of input.
pub fn read_message(reader: &mut impl BufRead) -> Option<Result<Value, String>> {
    let mut length: Option<usize> = None;
    loop {
        let mut line = String::new();
        let n = reader.read_line(&mut line).ok()?;
        if n == 0 {
            return None;
        }
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            break;
        }
        if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
            length = value.trim().parse().ok();
        }
    }
    let length = length?;
    // A hostile or broken client must not make the server allocate without limit.
    if length > 64 * 1024 * 1024 {
        return Some(Err("message too large".to_string()));
    }
    let mut body = vec![0u8; length];
    reader.read_exact(&mut body).ok()?;
    Some(match String::from_utf8(body) {
        Ok(text) => json::parse(&text).ok_or_else(|| "the message is not valid JSON".to_string()),
        Err(_) => Err("the message is not valid UTF-8".to_string()),
    })
}

/// Writes one framed message.
pub fn write_message(writer: &mut impl Write, message: &Value) -> std::io::Result<()> {
    let body = message.to_json();
    write!(writer, "Content-Length: {}\r\n\r\n{body}", body.len())?;
    writer.flush()
}

/// Runs a session until the client exits or the input ends. Returns the exit code.
pub fn serve(reader: &mut impl BufRead, writer: &mut impl Write) -> i32 {
    let mut server = Server::new();
    while let Some(read) = read_message(reader) {
        let message = match read {
            Ok(message) => message,
            Err(why) => {
                eprintln!("mink lsp: ignoring a message: {why}");
                continue;
            }
        };
        for out in server.handle(&message) {
            if write_message(writer, &out).is_err() {
                return 1;
            }
        }
        if let Some(code) = server.exit_code() {
            return code;
        }
    }
    // The client went away without saying exit.
    if server.shutdown { 0 } else { 1 }
}

/// Entry point of `mink lsp`, called by the command line.
pub fn main(args: &[String]) -> ExitCode {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    for arg in args {
        if arg != "--stdio" {
            eprintln!("mink: error: unknown option '{arg}' for 'lsp'\n{USAGE}");
            return ExitCode::from(2);
        }
    }
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let code = serve(&mut stdin.lock(), &mut stdout.lock());
    ExitCode::from(code.clamp(0, 255) as u8)
}

/// The address of a path, as the server would send it.
pub fn uri_for(path: &std::path::Path) -> String {
    path_to_uri(path)
}

#[cfg(test)]
mod tests;
