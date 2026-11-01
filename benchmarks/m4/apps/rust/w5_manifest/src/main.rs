//! M4 W5 `manifest` — Rust port.
//!
//! Contract: `docs/benchmarks/M4/M4_SESSION_156.md`.
//!
//! JSON config + recursive walk + SHA-256 + RFC 4180 CSV. Paths are relative
//! to the config root with `/` separators and sorted in byte order.
//!
//! The CSV writer is hand-written: the `csv` crate is not in the local cargo
//! cache and W5's report format is a fixed four-column RFC 4180 subset.
//! `serde_json` and `sha2` are pinned and cached.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde_json::Value;
use sha2::{Digest, Sha256};

const USAGE: &str = "usage: manifest <config.json>";

struct Config {
    root: String,
    out: String,
    extensions: Vec<String>,
}

fn out(text: &str) {
    let mut stdout = std::io::stdout();
    let _ = stdout.write_all(text.as_bytes());
    let _ = stdout.write_all(b"\r\n");
    let _ = stdout.flush();
}

fn fail(label: &str, code: u8) -> ExitCode {
    let mut stderr = std::io::stderr();
    let _ = stderr.write_all(format!("manifest: {label}\r\n").as_bytes());
    let _ = stderr.flush();
    ExitCode::from(code)
}

fn fail_file(label: &str, path: &str, code: u8) -> ExitCode {
    fail(&format!("{label}{path}"), code)
}

fn parse_config(raw: &[u8]) -> Result<Config, &'static str> {
    let document: Value =
        serde_json::from_slice(raw).map_err(|_| "config is not valid JSON")?;
    let object = match document {
        Value::Object(object) => object,
        _ => return Err("config must be a JSON object"),
    };
    let root = object
        .get("root")
        .ok_or("config needs root, out and extensions")?;
    let out_path = object
        .get("out")
        .ok_or("config needs root, out and extensions")?;
    let extensions = object
        .get("extensions")
        .ok_or("config needs root, out and extensions")?;

    let root = match root {
        Value::String(text) => text.clone(),
        _ => return Err("config types must be string, string, array"),
    };
    let out_path = match out_path {
        Value::String(text) => text.clone(),
        _ => return Err("config types must be string, string, array"),
    };
    let extensions = match extensions {
        Value::Array(items) => {
            let mut collected = Vec::new();
            for item in items {
                match item {
                    Value::String(text) => collected.push(text.clone()),
                    _ => return Err("config types must be string, string, array"),
                }
            }
            collected
        }
        _ => return Err("config types must be string, string, array"),
    };
    Ok(Config {
        root,
        out: out_path,
        extensions,
    })
}

fn selected(rel: &str, extensions: &[String]) -> bool {
    let ext = match Path::new(rel).extension().and_then(|value| value.to_str()) {
        Some(ext) => format!(".{ext}"),
        None => return false,
    };
    extensions
        .iter()
        .any(|wanted| wanted.eq_ignore_ascii_case(&ext))
}

fn walk(root: &Path, current: &Path, files: &mut Vec<String>) -> std::io::Result<()> {
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            walk(root, &path, files)?;
        } else if path.is_file() {
            let rel: PathBuf = match path.strip_prefix(root) {
                Ok(relative) => relative.to_path_buf(),
                Err(_) => continue,
            };
            let rel = rel
                .components()
                .map(|part| part.as_os_str().to_string_lossy().to_string())
                .collect::<Vec<_>>()
                .join("/");
            files.push(rel);
        }
    }
    Ok(())
}

fn csv_field(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') || value.contains('\r') {
        return format!("\"{}\"", value.replace('"', "\"\""));
    }
    value.to_string()
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.iter().any(|arg| arg.starts_with('-')) {
        return fail("this tool has no options", 1);
    }
    if args.len() != 1 {
        return fail(USAGE, 1);
    }

    let path = &args[0];
    if !Path::new(path).exists() {
        return fail_file("config does not exist: ", path, 2);
    }
    if !Path::new(path).is_file() {
        return fail_file("not a regular file: ", path, 2);
    }
    let raw = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(_) => return fail_file("config does not exist: ", path, 2),
    };
    let config = match parse_config(&raw) {
        Ok(config) => config,
        Err(message) => return fail(message, 3),
    };

    let root = Path::new(&config.root);
    if !root.is_dir() {
        return fail_file("root does not exist: ", &config.root, 4);
    }

    let mut files = Vec::new();
    if walk(root, root, &mut files).is_err() {
        return fail_file("root does not exist: ", &config.root, 4);
    }
    files.retain(|rel| selected(rel, &config.extensions));
    files.sort();
    if files.is_empty() {
        return fail("no file matched the extensions", 5);
    }

    let mut report = String::from("path,size,sha256,ext\n");
    let mut total_bytes: u64 = 0;
    for rel in &files {
        let full = root.join(rel);
        let data = match fs::read(&full) {
            Ok(data) => data,
            Err(_) => return fail_file("cannot read file: ", rel, 4),
        };
        total_bytes += data.len() as u64;
        let digest = Sha256::digest(&data);
        let hex = format!("{digest:x}");
        report.push_str(&csv_field(rel));
        report.push(',');
        report.push_str(&data.len().to_string());
        report.push(',');
        report.push_str(&hex[..16]);
        report.push(',');
        report.push_str(&csv_field(
            Path::new(rel)
                .extension()
                .and_then(|value| value.to_str())
                .map(|value| format!(".{value}"))
                .unwrap_or_default()
                .as_str(),
        ));
        report.push('\n');
    }

    if fs::write(&config.out, report.as_bytes()).is_err() {
        return fail_file("cannot write report: ", &config.out, 4);
    }

    out(&format!("files={}", files.len()));
    out(&format!("bytes={total_bytes}"));
    out(&format!("first={}", files[0]));
    ExitCode::SUCCESS
}
