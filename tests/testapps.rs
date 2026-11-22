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
