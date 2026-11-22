//! `mink lsp` as an editor would run it: a real process speaking the protocol over stdio.

use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Command, Stdio};

fn send(stdin: &mut impl Write, body: &str) {
    write!(stdin, "Content-Length: {}\r\n\r\n{}", body.len(), body).unwrap();
    stdin.flush().unwrap();
}

fn receive(out: &mut impl BufRead) -> String {
    let mut length = 0usize;
    loop {
        let mut line = String::new();
        assert!(out.read_line(&mut line).unwrap() > 0, "the server closed its output early");
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        if let Some(v) = line.strip_prefix("Content-Length: ") {
            length = v.parse().unwrap();
        }
    }
    let mut body = vec![0u8; length];
    out.read_exact(&mut body).unwrap();
    String::from_utf8(body).unwrap()
}

#[test]
fn a_session_over_stdio_finds_an_error_and_exits_cleanly() {
    let dir = std::env::temp_dir().join(format!("mink_lsp_cli_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("a.mink");
    std::fs::write(&file, "fn main() {}\n").unwrap();
    let uri = format!("file:///{}", file.to_string_lossy().replace('\\', "/"));

    let mut child = Command::new(env!("CARGO_BIN_EXE_mink"))
        .arg("lsp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());

    send(&mut stdin, r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#);
    let init = receive(&mut out);
    assert!(init.contains("\"hoverProvider\""), "{init}");
    send(&mut stdin, r#"{"jsonrpc":"2.0","method":"initialized","params":{}}"#);
    send(
        &mut stdin,
        &format!(
            r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"{uri}","languageId":"mink","version":1,"text":"fn main() {{ let x = nope; }}\n"}}}}}}"#
        ),
    );
    let diagnostics = receive(&mut out);
    assert!(diagnostics.contains("publishDiagnostics") && diagnostics.contains("E-S01"), "{diagnostics}");
    send(&mut stdin, r#"{"jsonrpc":"2.0","id":2,"method":"shutdown"}"#);
    assert!(receive(&mut out).contains("\"result\":null"));
    send(&mut stdin, r#"{"jsonrpc":"2.0","method":"exit"}"#);
    drop(stdin);
    assert_eq!(child.wait().unwrap().code(), Some(0));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_unknown_option_is_a_usage_error_and_help_works() {
    let bad = Command::new(env!("CARGO_BIN_EXE_mink")).args(["lsp", "--bogus"]).output().unwrap();
    assert_eq!(bad.status.code(), Some(2));
    let help = Command::new(env!("CARGO_BIN_EXE_mink")).args(["lsp", "--help"]).output().unwrap();
    assert_eq!(help.status.code(), Some(0));
    assert!(!help.stdout.is_empty());
}
