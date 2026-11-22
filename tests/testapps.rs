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

#[test]
fn livelog_streams_history_and_new_lines_and_ends_cleanly() {
    use std::io::{Read, Write};
    let (exe, dir) = build("livelog");
    let log = dir.join("app.log");
    std::fs::write(&log, "old 1\nold 2\n").unwrap();
    let port = 19_300 + (std::process::id() % 400) as u16;
    let mut server = Command::new(&exe)
        .args([log.to_str().unwrap(), &port.to_string(), "2"])
        .stdout(std::process::Stdio::null())
        .spawn()
        .unwrap();
    assert!(http_get(port, "GET / HTTP/1.1\r\nHost: x\r\n\r\n").contains("EventSource"), "the page streams from /events");
    let mut stream = None;
    for _ in 0..40 {
        if let Ok(s) = std::net::TcpStream::connect(("127.0.0.1", port)) {
            stream = Some(s);
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    let mut stream = stream.expect("the server never answered");
    stream.set_read_timeout(Some(std::time::Duration::from_millis(300))).unwrap();
    stream.write_all(b"GET /events HTTP/1.1\r\nHost: x\r\n\r\n").unwrap();
    let mut seen = String::new();
    let mut read_until = |wanted: &str, seen: &mut String| {
        for _ in 0..40 {
            let mut buf = [0u8; 1024];
            if let Ok(n) = stream.read(&mut buf) {
                seen.push_str(&String::from_utf8_lossy(&buf[..n]));
            }
            if seen.contains(wanted) {
                return true;
            }
        }
        false
    };
    assert!(read_until("data: old 2", &mut seen), "history first: {seen}");
    assert!(seen.contains("text/event-stream") && seen.contains("data: old 1"), "{seen}");
    let mut file = std::fs::OpenOptions::new().append(true).open(&log).unwrap();
    file.write_all(b"new A\r\n").unwrap();
    assert!(read_until("data: new A\n", &mut seen), "a new line arrives live, without its carriage return: {seen:?}");
    drop(stream);
    // Streams that were closed are noticed on the next keep-alive, then the server ends.
    let mut waited = 0;
    let status = loop {
        if let Some(s) = server.try_wait().unwrap() {
            break s;
        }
        waited += 1;
        assert!(waited < 100, "the server did not end after the stream closed");
        std::thread::sleep(std::time::Duration::from_millis(100));
    };
    assert_eq!(status.code(), Some(0), "no leaks");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn mini_follows_precedence_keeps_variables_and_explains_errors() {
    use std::io::Write;
    let (exe, dir) = build("mini");
    let script = "let x = 3 + 4 * 2\n(x - 1) % 4\nlet y = -x * (2 + 3)\ny\n100 / 7 % 5\n-7 / 2\nx / 0\n2 +\n(1 + 2\nfoo + 1\n3 4\n\nvars\n";
    let mut child = Command::new(&exe)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(script.as_bytes()).unwrap();
    let out = child.wait_with_output().unwrap();
    assert_eq!(out.status.code(), Some(0), "no leaks");
    let text = String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n");
    let expected = "x = 11\n2\ny = -55\n-55\n4\n-3\n\
        line 7, column 6: division by zero\n\
        line 8, column 4: the line ends too soon\n\
        line 9, column 7: missing )\n\
        line 10, column 1: unknown name\n\
        line 11, column 3: unexpected text after the expression\n\
        x = 11\ny = -55\n";
    assert_eq!(text, expected);
    // A file path works too.
    let file = dir.join("s.txt");
    std::fs::write(&file, "1 + 1\n").unwrap();
    assert_eq!(run(&exe, &[file.to_str().unwrap()]), (0, "2\n".to_string()));
    assert_eq!(run(&exe, &[dir.join("missing.txt").to_str().unwrap()]).0, 2);
    let _ = std::fs::remove_dir_all(&dir);
}

fn snake_numbers(out: &str) -> (i64, i64, i64) {
    let lines: Vec<&str> = out.lines().collect();
    let pick = |label: &str| -> i64 {
        let at = lines.iter().position(|l| *l == label).unwrap_or_else(|| panic!("no {label} in {out}"));
        lines[at + 1].trim().parse().unwrap()
    };
    (pick("score:"), pick("length:"), pick("ticks:"))
}

#[test]
fn snake_autopilot_is_deterministic_obeys_the_rules_and_ends_cleanly() {
    let (exe, dir) = build("snake");
    // Before any move: length 3, the head at column 5 of row 6.
    let (code, start) = run(&exe, &["--demo", "0", "1"]);
    assert_eq!(code, 0);
    assert_eq!(snake_numbers(&start), (0, 3, 0));
    assert!(start.lines().nth(7).unwrap().starts_with("| ooo@") || start.lines().nth(7).unwrap().contains("@"), "{start}");
    assert!(start.contains("still alive"));
    // The same seed gives the same game; a different one does not.
    let a = run(&exe, &["--demo", "300", "7"]);
    let b = run(&exe, &["--demo", "300", "7"]);
    assert_eq!(a, b, "a seed always gives the same game");
    assert_ne!(a.1, run(&exe, &["--demo", "300", "8"]).1);
    // The rules: the score is always the growth in length, and the frame is a 24 by 12 board.
    for seed in 1..=12 {
        let (code, out) = run(&exe, &["--demo", "500", &seed.to_string()]);
        assert_eq!(code, 0, "seed {seed}: no leaks");
        let (score, length, ticks) = snake_numbers(&out);
        assert_eq!(length, score + 3, "seed {seed}: each food adds one segment");
        assert!(ticks <= 500);
        assert_eq!(out.lines().filter(|l| l.starts_with('|')).count(), 12, "seed {seed}");
        assert_eq!(out.lines().filter(|l| l.starts_with('|')).map(|l| l.matches('@').count()).sum::<usize>(), 1, "one head");
        assert_eq!(out.lines().filter(|l| l.starts_with('|')).map(|l| l.matches('*').count()).sum::<usize>(), 1, "one piece of food");
    }
    assert_eq!(run(&exe, &["--nonsense"]).0, 2);
    let _ = std::fs::remove_dir_all(&dir);
}
