//! Tests for the language server: the messages it sends for each request, against real files
//! in a temporary folder and the bundled standard library.

use super::*;
use std::fs;
use std::path::Path;

struct Project {
    dir: PathBuf,
    server: Server,
    next_id: f64,
}

fn num(n: usize) -> Value {
    Value::Number(n as f64)
}

fn pos(line: usize, character: usize) -> Value {
    Value::object(vec![("line", num(line)), ("character", num(character))])
}

impl Project {
    fn new(label: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("mink_lsp_{label}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        Project {
            dir,
            server: Server::new(),
            next_id: 1.0,
        }
    }

    fn write(&self, name: &str, text: &str) -> PathBuf {
        let path = self.dir.join(name);
        fs::write(&path, text).unwrap();
        path
    }

    fn uri(&self, name: &str) -> String {
        path_to_uri(&self.dir.join(name))
    }

    fn notify(&mut self, method: &str, params: Value) -> Vec<Value> {
        self.server.handle(&notification(method, params))
    }

    /// Opens a document with the given text (and writes it to disk so `mod` lookups work).
    fn open(&mut self, name: &str, text: &str) -> Vec<Value> {
        self.write(name, text);
        let uri = self.uri(name);
        self.notify(
            "textDocument/didOpen",
            Value::object(vec![(
                "textDocument",
                Value::object(vec![
                    ("uri", Value::text(uri)),
                    ("languageId", Value::text("mink")),
                    ("version", Value::Number(1.0)),
                    ("text", Value::text(text)),
                ]),
            )]),
        )
    }

    fn change(&mut self, name: &str, text: &str) -> Vec<Value> {
        let uri = self.uri(name);
        self.notify(
            "textDocument/didChange",
            Value::object(vec![
                ("textDocument", Value::object(vec![("uri", Value::text(uri)), ("version", Value::Number(2.0))])),
                ("contentChanges", Value::List(vec![Value::object(vec![("text", Value::text(text))])])),
            ]),
        )
    }

    /// Sends a request about a position and returns the response message.
    fn ask(&mut self, method: &str, name: &str, line: usize, character: usize, extra: Vec<(&str, Value)>) -> Value {
        let id = self.next_id;
        self.next_id += 1.0;
        let mut params = vec![
            ("textDocument", Value::object(vec![("uri", Value::text(self.uri(name)))])),
            ("position", pos(line, character)),
        ];
        params.extend(extra);
        let message = Value::object(vec![
            ("jsonrpc", Value::text("2.0")),
            ("id", Value::Number(id)),
            ("method", Value::text(method)),
            ("params", Value::object(params)),
        ]);
        let mut out = self.server.handle(&message);
        assert_eq!(out.len(), 1, "one response expected for {method}: {out:?}");
        out.remove(0)
    }

    fn result(&mut self, method: &str, name: &str, line: usize, character: usize) -> Value {
        let response = self.ask(method, name, line, character, Vec::new());
        response.get("result").cloned().unwrap_or_else(|| panic!("no result in {}", response.to_json()))
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn diagnostics_for<'a>(messages: &'a [Value], uri: &str) -> Option<&'a Vec<Value>> {
    messages.iter().find_map(|m| {
        if m.get("method")?.as_str()? != "textDocument/publishDiagnostics" {
            return None;
        }
        let params = m.get("params")?;
        if params.get("uri")?.as_str()? != uri {
            return None;
        }
        match params.get("diagnostics")? {
            Value::List(items) => Some(items),
            _ => None,
        }
    })
}

fn s(value: Option<&Value>) -> String {
    value.and_then(Value::as_str).unwrap_or_default().to_string()
}

fn field<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut at = value;
    for key in path {
        at = at.get(key)?;
    }
    Some(at)
}

fn line_of(range_start: &Value) -> usize {
    match field(range_start, &["start", "line"]) {
        Some(Value::Number(n)) => *n as usize,
        _ => usize::MAX,
    }
}

// ------------------------------------------------------------------ lifecycle

#[test]
fn initialize_announces_every_feature() {
    let mut server = Server::new();
    let out = server.handle(&Value::object(vec![
        ("jsonrpc", Value::text("2.0")),
        ("id", Value::Number(1.0)),
        ("method", Value::text("initialize")),
        ("params", Value::object(Vec::<(String, Value)>::new())),
    ]));
    let caps = field(&out[0], &["result", "capabilities"]).expect("capabilities");
    for feature in [
        "hoverProvider", "definitionProvider", "referencesProvider", "documentSymbolProvider",
        "renameProvider", "signatureHelpProvider", "completionProvider", "documentFormattingProvider",
        "textDocumentSync",
    ] {
        assert!(caps.get(feature).is_some(), "{feature}");
    }
    assert_eq!(s(field(&out[0], &["result", "serverInfo", "name"])), "mink");
    assert_eq!(s(caps.get("positionEncoding")), "utf-16");
}

#[test]
fn shutdown_then_exit_is_a_clean_exit_and_exit_alone_is_not() {
    let mut server = Server::new();
    let out = server.handle(&Value::object(vec![("id", Value::Number(2.0)), ("method", Value::text("shutdown"))]));
    assert_eq!(out[0].get("result"), Some(&Value::Null));
    server.handle(&Value::object(vec![("method", Value::text("exit"))]));
    assert_eq!(server.exit_code(), Some(0));
    let mut rude = Server::new();
    rude.handle(&Value::object(vec![("method", Value::text("exit"))]));
    assert_eq!(rude.exit_code(), Some(1));
}

#[test]
fn unknown_requests_get_a_method_not_found_error_and_unknown_notifications_are_ignored() {
    let mut server = Server::new();
    let out = server.handle(&Value::object(vec![
        ("id", Value::Number(9.0)),
        ("method", Value::text("textDocument/somethingNew")),
        ("params", Value::object(vec![("textDocument", Value::object(vec![("uri", Value::text("file:///x.mink"))]))])),
    ]));
    assert_eq!(field(&out[0], &["error", "code"]), Some(&Value::Number(-32601.0)));
    assert!(server.handle(&Value::object(vec![("method", Value::text("$/progress"))])).is_empty());
    assert!(server.handle(&Value::object(vec![("id", Value::Number(1.0)), ("result", Value::Null)])).is_empty());
}

// ---------------------------------------------------------------- diagnostics

#[test]
fn an_error_in_the_buffer_is_published_and_cleared_when_fixed() {
    let mut p = Project::new("diag");
    let uri = p.uri("a.mink");
    let out = p.open("a.mink", "fn main() {\n    let x = missing_name;\n}\n");
    let list = diagnostics_for(&out, &uri).expect("diagnostics for the document");
    assert_eq!(list.len(), 1, "{}", out[0].to_json());
    assert_eq!(s(list[0].get("code")), "E-S01");
    assert_eq!(s(list[0].get("source")), "mink");
    assert_eq!(list[0].get("severity"), Some(&Value::Number(1.0)));
    assert!(s(list[0].get("message")).contains("missing_name"));
    assert_eq!(line_of(list[0].get("range").unwrap()), 1, "the error is on the second line");
    assert_eq!(field(&list[0], &["range", "start", "character"]), Some(&Value::Number(12.0)));

    let out = p.change("a.mink", "fn main() {\n    let x = 1;\n}\n");
    let list = diagnostics_for(&out, &uri).expect("the fix publishes an empty list");
    assert!(list.is_empty());
}

#[test]
fn the_unsaved_buffer_wins_over_the_file_on_disk() {
    let mut p = Project::new("buffer");
    let uri = p.uri("a.mink");
    // The disk copy is fine; the buffer is not.
    p.write("a.mink", "fn main() {}\n");
    let out = p.notify(
        "textDocument/didOpen",
        Value::object(vec![(
            "textDocument",
            Value::object(vec![
                ("uri", Value::text(&uri)),
                ("version", Value::Number(1.0)),
                ("text", Value::text("fn main() { let a = ; }\n")),
            ]),
        )]),
    );
    let list = diagnostics_for(&out, &uri).unwrap();
    assert!(!list.is_empty(), "{}", out[0].to_json());
}

#[test]
fn columns_are_utf16_so_text_before_an_error_does_not_shift_it() {
    let mut p = Project::new("utf16");
    let uri = p.uri("a.mink");
    // "😀" is two UTF-16 units and four bytes.
    let out = p.open("a.mink", "// \u{1f600}\nfn main() { let x = nope; }\n");
    let list = diagnostics_for(&out, &uri).unwrap();
    assert_eq!(line_of(list[0].get("range").unwrap()), 1);
    assert_eq!(field(&list[0], &["range", "start", "character"]), Some(&Value::Number(20.0)));
}

#[test]
fn a_duplicate_definition_carries_its_first_declaration_as_related_information() {
    let mut p = Project::new("related");
    let uri = p.uri("a.mink");
    let out = p.open("a.mink", "fn dup() {}\nfn dup() {}\nfn main() {}\n");
    let list = diagnostics_for(&out, &uri).unwrap();
    assert_eq!(s(list[0].get("code")), "E-S02");
    let related = match list[0].get("relatedInformation") {
        Some(Value::List(items)) => items,
        other => panic!("no related information: {other:?}"),
    };
    assert_eq!(line_of(field(&related[0], &["location", "range"]).unwrap()), 0);
}

#[test]
fn closing_a_document_clears_its_diagnostics_and_non_file_documents_are_ignored() {
    let mut p = Project::new("close");
    let uri = p.uri("a.mink");
    p.open("a.mink", "fn main() { let x = nope; }\n");
    let out = p.notify(
        "textDocument/didClose",
        Value::object(vec![("textDocument", Value::object(vec![("uri", Value::text(&uri))]))]),
    );
    assert!(diagnostics_for(&out, &uri).unwrap().is_empty());
    let out = p.notify(
        "textDocument/didOpen",
        Value::object(vec![(
            "textDocument",
            Value::object(vec![
                ("uri", Value::text("untitled:Untitled-1")),
                ("version", Value::Number(1.0)),
                ("text", Value::text("fn main() {}")),
            ]),
        )]),
    );
    assert!(out.is_empty());
}

// ----------------------------------------------------------- hover and friends

const SAMPLE: &str = "// Adds two numbers.\nfn add(a: Int, b: Int) -> Int {\n    let sum = a + b;\n    return sum;\n}\n\nfn main() {\n    let total = add(1, 2);\n    rt_print_int(total);\n}\n";

#[test]
fn hover_shows_signatures_docs_locals_builtins_and_keywords() {
    let mut p = Project::new("hover");
    p.open("a.mink", SAMPLE);
    let call = p.result("textDocument/hover", "a.mink", 7, 17);
    let text = s(field(&call, &["contents", "value"]));
    assert!(text.contains("fn add(a: Int, b: Int) -> Int"), "{text}");
    assert!(text.contains("Adds two numbers."), "{text}");
    let local = p.result("textDocument/hover", "a.mink", 8, 18);
    let text = s(field(&local, &["contents", "value"]));
    assert!(text.contains("let total") && text.contains("local variable"), "{text}");
    let builtin = p.result("textDocument/hover", "a.mink", 8, 6);
    assert!(s(field(&builtin, &["contents", "value"])).contains("rt_print_int(Int) -> Unit"));
    let keyword = p.result("textDocument/hover", "a.mink", 1, 1);
    assert!(s(field(&keyword, &["contents", "value"])).contains("Declares a function"));
    let nothing = p.result("textDocument/hover", "a.mink", 0, 0);
    assert_eq!(nothing, Value::Null, "a comment has no hover");
}

#[test]
fn definition_finds_functions_locals_and_names_in_other_modules() {
    let mut p = Project::new("definition");
    p.write("util.mink", "fn helper() -> Int {\n    return 7;\n}\n");
    let uri_util = p.uri("util.mink");
    p.open("main.mink", "mod util;\nfn main() {\n    let v = helper();\n    rt_print_int(v);\n}\n");
    let across = p.result("textDocument/definition", "main.mink", 2, 14);
    assert_eq!(s(across.get("uri")), uri_util);
    assert_eq!(line_of(across.get("range").unwrap()), 0);
    let local = p.result("textDocument/definition", "main.mink", 3, 17);
    assert_eq!(s(local.get("uri")), p.uri("main.mink"));
    assert_eq!(line_of(local.get("range").unwrap()), 2);
    let builtin = p.result("textDocument/definition", "main.mink", 3, 6);
    assert_eq!(builtin, Value::Null, "built-ins have no source");
}

#[test]
fn qualified_paths_resolve_through_the_module_name() {
    let mut p = Project::new("qualified");
    p.open("main.mink", "mod math;\nfn main() {\n    let r = math::math_abs(-3);\n    rt_print_int(r);\n}\n");
    let def = p.result("textDocument/definition", "main.mink", 2, 22);
    assert!(s(def.get("uri")).ends_with("/stdlib/math.mink"), "{}", def.to_json());
    let hover = p.result("textDocument/hover", "main.mink", 2, 22);
    assert!(s(field(&hover, &["contents", "value"])).contains("fn math_abs(n: Int) -> Int"));
}

#[test]
fn signature_help_tracks_the_argument_being_typed() {
    let mut p = Project::new("sig");
    p.open("a.mink", "fn add(a: Int, b: Int) -> Int { return a + b; }\nfn main() {\n    add(1, 2);\n}\n");
    let first = p.result("textDocument/signatureHelp", "a.mink", 2, 8);
    assert_eq!(s(field(&first, &["signatures"]).and_then(|l| match l { Value::List(i) => i.first(), _ => None }).and_then(|v| v.get("label"))), "fn add(a: Int, b: Int) -> Int");
    assert_eq!(first.get("activeParameter"), Some(&Value::Number(0.0)));
    let second = p.result("textDocument/signatureHelp", "a.mink", 2, 11);
    assert_eq!(second.get("activeParameter"), Some(&Value::Number(1.0)));
    let outside = p.result("textDocument/signatureHelp", "a.mink", 1, 3);
    assert_eq!(outside, Value::Null);
}

#[test]
fn the_outline_lists_declarations_with_fields_and_variants_underneath() {
    let mut p = Project::new("outline");
    p.open("a.mink", "struct Point { x: Int, y: Int }\nenum Color { Red, Green }\nfn main() {}\n");
    let response = p.ask("textDocument/documentSymbol", "a.mink", 0, 0, Vec::new());
    let list = match response.get("result") {
        Some(Value::List(items)) => items.clone(),
        other => panic!("{other:?}"),
    };
    let names: Vec<String> = list.iter().map(|v| s(v.get("name"))).collect();
    assert_eq!(names, vec!["Point", "Color", "main"]);
    let kids = match list[0].get("children") {
        Some(Value::List(k)) => k.iter().map(|v| s(v.get("name"))).collect::<Vec<_>>(),
        other => panic!("{other:?}"),
    };
    assert_eq!(kids, vec!["x", "y"]);
}

// ----------------------------------------------------------------- completion

#[test]
fn completion_offers_locals_functions_builtins_and_keywords() {
    let mut p = Project::new("complete");
    p.open("a.mink", "fn helper() {}\nfn main() {\n    let counter = 1;\n    cou\n}\n");
    let result = p.result("textDocument/completion", "a.mink", 3, 7);
    let labels: Vec<String> = match result.get("items") {
        Some(Value::List(items)) => items.iter().map(|i| s(i.get("label"))).collect(),
        other => panic!("{other:?}"),
    };
    for expected in ["counter", "helper", "main", "rt_print_int", "return", "let"] {
        assert!(labels.contains(&expected.to_string()), "{expected} missing from {} items", labels.len());
    }
    let first_local = labels.iter().position(|l| l == "counter").unwrap();
    let a_function = labels.iter().position(|l| l == "helper").unwrap();
    assert!(first_local < a_function, "locals come first");
}

#[test]
fn completion_after_a_module_path_lists_that_modules_members() {
    let mut p = Project::new("complete_path");
    p.open("a.mink", "mod math;\nfn main() {\n    math::\n}\n");
    let result = p.result("textDocument/completion", "a.mink", 2, 10);
    let labels: Vec<String> = match result.get("items") {
        Some(Value::List(items)) => items.iter().map(|i| s(i.get("label"))).collect(),
        other => panic!("{other:?}"),
    };
    assert!(labels.contains(&"math_abs".to_string()), "{labels:?}");
    assert!(!labels.contains(&"return".to_string()), "no keywords after a module path");
}

#[test]
fn completion_after_an_enum_name_and_double_colon_lists_its_variants() {
    let mut p = Project::new("complete_enum");
    p.open("a.mink", "enum Color { Red, Green }\nfn main() {\n    let c = Color::\n}\n");
    let result = p.result("textDocument/completion", "a.mink", 2, 19);
    let labels: Vec<String> = match result.get("items") {
        Some(Value::List(items)) => items.iter().map(|i| s(i.get("label"))).collect(),
        other => panic!("{other:?}"),
    };
    assert_eq!(labels, vec!["Red", "Green"]);
}

// ------------------------------------------------------------ references, rename

#[test]
fn references_find_every_use_and_skip_member_accesses() {
    let mut p = Project::new("refs");
    p.open(
        "a.mink",
        "struct S { add: Int }\nfn add(a: Int, b: Int) -> Int { return a + b; }\nfn main() {\n    let s = S { add: 1 };\n    let r = add(s.add, 2);\n    rt_print_int(r);\n}\n",
    );
    let all = p.ask(
        "textDocument/references",
        "a.mink",
        1,
        4,
        vec![("context", Value::object(vec![("includeDeclaration", Value::Bool(true))]))],
    );
    let lines: Vec<usize> = match all.get("result") {
        Some(Value::List(items)) => items.iter().map(|l| line_of(l.get("range").unwrap())).collect(),
        other => panic!("{other:?}"),
    };
    assert_eq!(lines, vec![1, 4], "the declaration and the call, not `s.add` or the field");
    let without = p.ask(
        "textDocument/references",
        "a.mink",
        1,
        4,
        vec![("context", Value::object(vec![("includeDeclaration", Value::Bool(false))]))],
    );
    assert!(matches!(without.get("result"), Some(Value::List(items)) if items.len() == 1));
}

#[test]
fn renaming_a_local_changes_only_that_function() {
    let mut p = Project::new("rename_local");
    p.open("a.mink", "fn one() {\n    let value = 1;\n    rt_print_int(value);\n}\nfn two() {\n    let value = 2;\n    rt_print_int(value);\n}\n");
    let response = p.ask("textDocument/rename", "a.mink", 1, 9, vec![("newName", Value::text("amount"))]);
    let edits = field(&response, &["result", "changes"]).and_then(|c| c.get(&p.uri("a.mink"))).cloned().expect("edits");
    let lines: Vec<usize> = match edits {
        Value::List(items) => items.iter().map(|e| line_of(e.get("range").unwrap())).collect(),
        other => panic!("{other:?}"),
    };
    assert_eq!(lines, vec![1, 2], "only `one`, not `two`");
}

#[test]
fn renaming_a_function_updates_every_file_that_loads_it() {
    let mut p = Project::new("rename_global");
    p.write("util.mink", "fn helper() -> Int {\n    return 7;\n}\n");
    p.open("main.mink", "mod util;\nfn main() {\n    let v = helper();\n    rt_print_int(v);\n}\n");
    let response = p.ask("textDocument/rename", "main.mink", 2, 14, vec![("newName", Value::text("assistant"))]);
    let changes = field(&response, &["result", "changes"]).expect("changes");
    assert!(changes.get(&p.uri("util.mink")).is_some(), "the declaring file is edited: {}", response.to_json());
    assert!(changes.get(&p.uri("main.mink")).is_some());
}

#[test]
fn renaming_refuses_bad_names_taken_names_keywords_builtins_and_the_standard_library() {
    let mut p = Project::new("rename_refuse");
    p.open("a.mink", "mod math;\nfn alpha() {}\nfn beta() {}\nfn main() {\n    let n = math::math_abs(1);\n    alpha();\n    rt_print_int(n);\n}\n");
    let refuse = |p: &mut Project, line: usize, col: usize, name: &str| -> String {
        let response = p.ask("textDocument/rename", "a.mink", line, col, vec![("newName", Value::text(name))]);
        assert_eq!(field(&response, &["error", "code"]), Some(&Value::Number(-32803.0)), "{}", response.to_json());
        s(field(&response, &["error", "message"]))
    };
    assert!(refuse(&mut p, 5, 5, "1bad").contains("not a valid name"));
    assert!(refuse(&mut p, 5, 5, "return").contains("reserved word"));
    assert!(refuse(&mut p, 5, 5, "rt_mine").contains("reserved for the runtime"));
    assert!(refuse(&mut p, 5, 5, "beta").contains("already declared"));
    assert!(refuse(&mut p, 6, 6, "print_it").contains("cannot be renamed"), "a runtime function");
    assert!(refuse(&mut p, 4, 22, "absolute").contains("standard library"));
    let prepare = p.ask("textDocument/prepareRename", "a.mink", 0, 0, Vec::new());
    assert!(prepare.get("error").is_some(), "a keyword is not renamable");
    let ok = p.ask("textDocument/prepareRename", "a.mink", 5, 5, Vec::new());
    assert_eq!(s(field(&ok, &["result", "placeholder"])), "alpha");
}

// ------------------------------------------------------------------ formatting

#[test]
fn formatting_returns_edits_nothing_for_clean_files_and_an_error_for_broken_ones() {
    let mut p = Project::new("fmt");
    p.open("a.mink", "fn main(){let x=1;rt_print_int(x);}\n");
    let edits = p.result("textDocument/formatting", "a.mink", 0, 0);
    let list = match edits {
        Value::List(items) => items,
        other => panic!("{other:?}"),
    };
    assert_eq!(list.len(), 1);
    assert!(s(list[0].get("newText")).contains("    let x = 1;"));
    p.change("a.mink", "fn main() {}\n");
    assert_eq!(p.result("textDocument/formatting", "a.mink", 0, 0), Value::List(Vec::new()));
    p.change("a.mink", "fn main() { \"open }\n");
    let broken = p.ask("textDocument/formatting", "a.mink", 0, 0, Vec::new());
    assert!(s(field(&broken, &["error", "message"])).contains("line 1"), "{}", broken.to_json());
}

#[test]
fn a_formatting_edit_replaces_only_the_lines_that_differ() {
    let mut p = Project::new("fmt_small");
    p.open("a.mink", "fn a() {}\n\nfn b(){let x=1;let y=2;}\n\nfn c() {}\n");
    let edits = match p.result("textDocument/formatting", "a.mink", 0, 0) {
        Value::List(items) => items,
        other => panic!("{other:?}"),
    };
    let range = edits[0].get("range").unwrap();
    assert_eq!(line_of(range), 2, "the edit starts at the first changed line");
    assert_eq!(field(range, &["end", "line"]), Some(&Value::Number(3.0)));
}

// -------------------------------------------------------------- the transport

fn framed(messages: &[Value]) -> Vec<u8> {
    let mut out = Vec::new();
    for m in messages {
        write_message(&mut out, m).unwrap();
    }
    out
}

fn request(id: usize, method: &str, params: Value) -> Value {
    Value::object(vec![("jsonrpc", Value::text("2.0")), ("id", num(id)), ("method", Value::text(method)), ("params", params)])
}

fn read_all(bytes: &[u8]) -> Vec<Value> {
    let mut reader = std::io::BufReader::new(bytes);
    let mut out = Vec::new();
    while let Some(Ok(m)) = read_message(&mut reader) {
        out.push(m);
    }
    out
}

#[test]
fn a_whole_session_over_the_wire() {
    let input = framed(&[
        request(1, "initialize", Value::object(Vec::<(String, Value)>::new())),
        Value::object(vec![("method", Value::text("initialized")), ("params", Value::object(Vec::<(String, Value)>::new()))]),
        request(2, "shutdown", Value::Null),
        Value::object(vec![("method", Value::text("exit"))]),
    ]);
    let mut output = Vec::new();
    let code = serve(&mut std::io::BufReader::new(&input[..]), &mut output);
    assert_eq!(code, 0);
    let replies = read_all(&output);
    assert_eq!(replies.len(), 2);
    assert!(output.starts_with(b"Content-Length: "), "responses are framed");
}

#[test]
fn garbage_and_a_dropped_connection_do_not_crash_the_server() {
    let mut input = Vec::new();
    input.extend_from_slice(b"Content-Length: 5\r\n\r\nhello");
    input.extend_from_slice(b"Content-Length: 2\r\n\r\n{}");
    input.extend_from_slice(b"Not-A-Header\r\n\r\n");
    input.extend_from_slice(&framed(&[request(1, "initialize", Value::object(Vec::<(String, Value)>::new()))]));
    let mut output = Vec::new();
    let code = serve(&mut std::io::BufReader::new(&input[..]), &mut output);
    assert_eq!(code, 1, "the input ended without shutdown");
    assert_eq!(read_all(&output).len(), 1, "the one valid request was still answered");
}

#[test]
fn an_oversized_message_is_refused_without_allocating_it() {
    let input = b"Content-Length: 99999999999\r\n\r\n".to_vec();
    let mut reader = std::io::BufReader::new(&input[..]);
    assert!(matches!(read_message(&mut reader), Some(Err(_)) | None));
}

#[allow(dead_code)]
fn _keep(_: &Path) {}
