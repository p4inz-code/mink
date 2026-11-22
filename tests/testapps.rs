//! The test apps in `testapps/`: each is built with the real compiler and run on known input.

use std::path::{Path, PathBuf};
use std::process::Command;

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Copies one app's source into a temp folder (so `main.exe` is not written into the repository),
/// builds it, and returns the path of the program.
fn build(app: &str) -> (PathBuf, PathBuf) {
    let dir = std::env::temp_dir().join(format!("mink_testapp_{app}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("main.mink");
    std::fs::copy(repo().join("testapps").join(app).join("main.mink"), &src).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_mink")).arg("build").arg(&src).output().unwrap();
    assert!(out.status.success(), "{app} did not build: {}", String::from_utf8_lossy(&out.stderr));
    (dir.join("main.exe"), dir)
}

fn run(exe: &Path, args: &[&str]) -> (i32, String) {
    let out = Command::new(exe).args(args).output().unwrap();
    (out.status.code().unwrap_or(-1), String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n"))
}

#[test]
fn jsonq_picks_values_and_reports_misses() {
    let (exe, dir) = build("jsonq");
    let json = dir.join("t.json");
    std::fs::write(&json, r#"{"name":"mink","items":[{"id":1,"tag":"a"},{"id":2,"tag":"b"}],"ok":true}"#).unwrap();
    let j = json.to_str().unwrap();
    assert_eq!(run(&exe, &[j, ".name"]), (0, "\"mink\"\n".to_string()));
    assert_eq!(run(&exe, &[j, ".name", "-r"]), (0, "mink\n".to_string()));
    assert_eq!(run(&exe, &[j, ".items[1].tag"]), (0, "\"b\"\n".to_string()));
    assert_eq!(run(&exe, &[j, ".items[0].id"]), (0, "1\n".to_string()));
    assert_eq!(run(&exe, &[j, ".ok"]), (0, "true\n".to_string()));
    assert_eq!(run(&exe, &[j, ".missing"]).0, 1);
    assert_eq!(run(&exe, &[j, ".items[9]"]).0, 1);
    assert_eq!(run(&exe, &[]).0, 2, "no arguments is a usage error");
    std::fs::write(&json, "{not json").unwrap();
    assert_eq!(run(&exe, &[j]).0, 1, "invalid JSON is refused");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn sitegen_fills_tokens_escapes_code_and_leaves_unknown_tokens() {
    let (exe, dir) = build("sitegen");
    let tpl = dir.join("t.html");
    let data = dir.join("d.txt");
    let code = dir.join("c.mink");
    let raw = dir.join("raw.txt");
    let out = dir.join("out.html");
    std::fs::write(&tpl, "<h1>{{TITLE}}</h1><pre>{{CODE}}</pre><p>{{RAW}} {{MISSING}}</p>").unwrap();
    std::fs::write(&code, "fn main() -> Int { return 0; } // a & b").unwrap();
    std::fs::write(&raw, "<b>bold</b>").unwrap();
    std::fs::write(
        &data,
        format!("TITLE=Hello\nCODE=@{}\nRAW=@@{}\n# comment\n", code.display(), raw.display()),
    )
    .unwrap();
    let (status, stdout) = run(&exe, &[tpl.to_str().unwrap(), data.to_str().unwrap(), out.to_str().unwrap()]);
    assert_eq!(status, 0, "{stdout}");
    let page = std::fs::read_to_string(&out).unwrap();
    assert!(page.contains("<h1>Hello</h1>"), "{page}");
    assert!(page.contains("-&gt; Int") && page.contains("a &amp; b"), "code is escaped: {page}");
    assert!(page.contains("<b>bold</b>"), "@@ is inserted as it is: {page}");
    assert!(page.contains("{{MISSING}}"), "unknown tokens are left alone: {page}");
    assert_eq!(run(&exe, &[]).0, 2);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn fgrep_searches_folders_ignores_case_and_skips_binary_files() {
    let (exe, dir) = build("fgrep");
    let tree = dir.join("tree");
    std::fs::create_dir_all(tree.join("sub")).unwrap();
    std::fs::write(tree.join("a.txt"), "one\nneedle here\nthree\n").unwrap();
    std::fs::write(tree.join("sub").join("b.txt"), "NEEDLE in caps\r\nnothing\r\n").unwrap();
    std::fs::write(tree.join("bin.dat"), b"needle\0binary").unwrap();
    let t = tree.to_str().unwrap();
    let (code, out) = run(&exe, &["needle", t]);
    assert_eq!(code, 0);
    assert_eq!(out.lines().count(), 1, "case-sensitive, binary skipped: {out}");
    assert!(out.contains("a.txt:2:needle here"), "{out}");
    let (code, out) = run(&exe, &["-i", "needle", t]);
    assert_eq!(code, 0);
    assert_eq!(out.lines().count(), 2, "{out}");
    assert!(out.contains("b.txt:1:NEEDLE in caps"), "carriage return is not part of the line: {out}");
    assert_eq!(run(&exe, &["absent", t]).0, 1);
    assert_eq!(run(&exe, &["x"]).0, 2);
    assert_eq!(run(&exe, &["x", dir.join("nope").to_str().unwrap()]).0, 2);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn dupfind_groups_identical_files_and_ignores_empty_and_same_size_files() {
    let (exe, dir) = build("dupfind");
    let tree = dir.join("tree");
    std::fs::create_dir_all(tree.join("sub")).unwrap();
    std::fs::write(tree.join("a.txt"), "same content here").unwrap();
    std::fs::write(tree.join("sub").join("b.txt"), "same content here").unwrap();
    std::fs::write(tree.join("d.txt"), "different but same!").unwrap();
    std::fs::write(tree.join("e.txt"), "different but diff..").unwrap();
    std::fs::write(tree.join("empty1.txt"), "").unwrap();
    std::fs::write(tree.join("empty2.txt"), "").unwrap();
    let (code, out) = run(&exe, &[tree.to_str().unwrap()]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("each file is 17 bytes"), "{out}");
    assert!(out.contains("a.txt") && out.contains("b.txt"), "{out}");
    assert!(!out.contains("d.txt") && !out.contains("empty"), "{out}");
    let numbers: Vec<&str> = out.lines().skip_while(|l| !l.starts_with("files scanned")).collect();
    assert_eq!(numbers[1], "6", "{out}");
    assert_eq!(numbers[3], "1", "one group: {out}");
    assert_eq!(numbers[5], "1", "one extra copy: {out}");
    assert_eq!(numbers[7], "17", "{out}");
    assert_eq!(run(&exe, &[]).0, 2);
    assert_eq!(run(&exe, &[tree.join("a.txt").to_str().unwrap()]).0, 2, "a file is not a folder");
    let _ = std::fs::remove_dir_all(&dir);
}

fn http_get(port: u16, request: &str) -> String {
    use std::io::{Read, Write};
    for _ in 0..40 {
        if let Ok(mut s) = std::net::TcpStream::connect(("127.0.0.1", port)) {
            s.set_read_timeout(Some(std::time::Duration::from_secs(5))).unwrap();
            s.write_all(request.as_bytes()).unwrap();
            let mut out = Vec::new();
            let _ = s.read_to_end(&mut out);
            return String::from_utf8_lossy(&out).into_owned();
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    panic!("the server never answered on port {port}");
}

#[test]
fn serve_serves_files_with_types_and_refuses_bad_requests() {
    let (exe, dir) = build("serve");
    let site = dir.join("site");
    std::fs::create_dir_all(site.join("sub")).unwrap();
    std::fs::write(site.join("index.html"), "<h1>home</h1>").unwrap();
    std::fs::write(site.join("a.css"), "body{}").unwrap();
    std::fs::write(site.join("sub").join("index.html"), "sub").unwrap();
    let port = 18_100 + (std::process::id() % 400) as u16;
    let mut server = Command::new(&exe)
        .args([site.to_str().unwrap(), &port.to_string(), "7"])
        .stdout(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let get = |path: &str| http_get(port, &format!("GET {path} HTTP/1.1\r\nHost: x\r\n\r\n"));
    let home = get("/");
    assert!(home.starts_with("HTTP/1.1 200") && home.contains("text/html") && home.ends_with("<h1>home</h1>"), "{home}");
    let css = get("/a.css?v=1");
    assert!(css.contains("text/css") && css.ends_with("body{}"), "{css}");
    assert!(get("/sub/").ends_with("sub"), "a folder path serves its index.html");
    assert!(get("/missing.html").starts_with("HTTP/1.1 404"));
    assert!(get("/../Cargo.toml").starts_with("HTTP/1.1 400"), "traversal is refused");
    assert!(http_get(port, "POST / HTTP/1.1\r\nHost: x\r\n\r\n").starts_with("HTTP/1.1 405"));
    let head = http_get(port, "HEAD / HTTP/1.1\r\nHost: x\r\n\r\n");
    assert!(head.starts_with("HTTP/1.1 200") && !head.contains("<h1>"), "HEAD sends no body: {head}");
    let status = server.wait().unwrap();
    assert_eq!(status.code(), Some(0), "after its seventh request the server ends cleanly with no leaks");
    let _ = std::fs::remove_dir_all(&dir);
}

fn body_of(response: &str) -> &str {
    response.split("\r\n\r\n").nth(1).unwrap_or("")
}

#[test]
fn todo_keeps_its_list_in_a_file_and_survives_a_restart() {
    let (exe, dir) = build("todo");
    let db = dir.join("todo.db");
    let port = 18_500 + (std::process::id() % 400) as u16;
    let start = |max: &str| {
        Command::new(&exe)
            .args([&port.to_string(), db.to_str().unwrap(), max])
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap()
    };
    let call = |method: &str, path: &str, body: &str| {
        http_get(
            port,
            &format!("{method} {path} HTTP/1.1\r\nHost: x\r\nContent-Length: {}\r\n\r\n{body}", body.len()),
        )
    };

    let mut first = start("7");
    assert!(call("GET", "/", "").contains("<title>Todo</title>"), "the page is inside the program");
    assert!(call("POST", "/api/todos", "Buy milk").starts_with("HTTP/1.1 201"));
    assert!(call("POST", "/api/todos", "Say \"hi\" \\ ok").starts_with("HTTP/1.1 201"));
    assert!(call("POST", "/api/todos", "").starts_with("HTTP/1.1 400"), "an empty title is refused");
    assert!(call("POST", "/api/todos/1/toggle", "").starts_with("HTTP/1.1 200"));
    let listed = call("GET", "/api/todos", "");
    assert_eq!(
        body_of(&listed),
        r#"[{"id":1,"title":"Buy milk","done":true},{"id":2,"title":"Say \"hi\" \\ ok","done":false}]"#
    );
    assert!(call("DELETE", "/api/todos/2", "").starts_with("HTTP/1.1 200"));
    assert_eq!(first.wait().unwrap().code(), Some(0), "no leaks");

    let mut second = start("3");
    assert_eq!(body_of(&call("GET", "/api/todos", "")), r#"[{"id":1,"title":"Buy milk","done":true}]"#, "the list came back from the file");
    assert!(call("POST", "/api/todos", "Second run").starts_with("HTTP/1.1 201"));
    assert_eq!(
        body_of(&call("GET", "/api/todos", "")),
        r#"[{"id":1,"title":"Buy milk","done":true},{"id":2,"title":"Second run","done":false}]"#,
        "a new id is one more than the largest id in the list"
    );
    assert_eq!(second.wait().unwrap().code(), Some(0));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn paste_stores_any_text_and_returns_it_byte_for_byte() {
    let (exe, dir) = build("paste");
    let db = dir.join("paste.db");
    let port = 18_900 + (std::process::id() % 400) as u16;
    let mut server = Command::new(&exe)
        .args([&port.to_string(), db.to_str().unwrap(), "6"])
        .stdout(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let call = |method: &str, path: &str, body: &str| {
        http_get(
            port,
            &format!("{method} {path} HTTP/1.1\r\nHost: x\r\nContent-Length: {}\r\n\r\n{body}", body.len()),
        )
    };
    let text = "line one\nline two\ttab \"quote\" \\ end, Z and z";
    let created = call("POST", "/paste", text);
    assert!(created.starts_with("HTTP/1.1 201"), "{created}");
    let id = body_of(&created).to_string();
    assert_eq!(id.len(), 6);
    assert!(id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit()), "{id}");
    assert_eq!(body_of(&call("GET", &format!("/p/{id}"), "")), text, "the text comes back unchanged");
    let second = body_of(&call("POST", "/paste", "another")).to_string();
    assert_ne!(second, id);
    assert!(call("GET", "/p/zzzzzz", "").starts_with("HTTP/1.1 404"));
    assert!(call("POST", "/paste", "").starts_with("HTTP/1.1 400"));
    assert!(call("GET", "/", "").contains("<title>Paste</title>"));
    assert_eq!(server.wait().unwrap().code(), Some(0), "no leaks");
    let _ = std::fs::remove_dir_all(&dir);
}
