//! M4 W1 `jsontool` — Rust port.
//!
//! Contract: `docs/benchmarks/M4/M4_SESSION_151.md` (reduced from the M1
//! `validation/c_json` application).
//!
//! The file is read and parsed before the command is dispatched (M1 ordering),
//! and any `-`-prefixed argument is a usage error because the tool has no
//! options.

use std::env;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::ExitCode;

use serde_json::Value;

const USAGE: &str = "usage: jsontool <validate|type|sum|compact> <file> [args]";

fn write_line(stream: &mut dyn Write, text: &str) {
    let _ = stream.write_all(text.as_bytes());
    let _ = stream.write_all(b"\r\n");
    let _ = stream.flush();
}

fn out(text: &str) {
    let mut stdout = std::io::stdout();
    write_line(&mut stdout, text);
}

fn err(text: &str) {
    let mut stderr = std::io::stderr();
    write_line(&mut stderr, &format!("jsontool: {text}"));
}

fn fail_usage(text: &str) -> ExitCode {
    err(text);
    ExitCode::from(1)
}

fn fail_file(label: &str, path: &str) -> ExitCode {
    let mut stderr = std::io::stderr();
    write_line(&mut stderr, &format!("jsontool: {label}{path}"));
    ExitCode::from(2)
}

fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn compact(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_default()
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();

    if args.iter().any(|arg| arg.starts_with('-')) {
        err("this tool has no options");
        return ExitCode::from(1);
    }
    if args.len() < 2 {
        return fail_usage(USAGE);
    }

    let command = args[0].as_str();
    let path = args[1].as_str();

    if !Path::new(path).exists() {
        return fail_file("input file does not exist: ", path);
    }
    if !Path::new(path).is_file() {
        return fail_file("not a regular file: ", path);
    }
    let raw = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(_) => return fail_file("input file does not exist: ", path),
    };
    let doc: Value = match serde_json::from_slice(&raw) {
        Ok(value) => value,
        Err(_) => {
            err("invalid JSON");
            return ExitCode::from(3);
        }
    };

    match command {
        "validate" => {
            if args.len() != 2 {
                return fail_usage("usage: jsontool validate <file>");
            }
            out("ok");
            ExitCode::SUCCESS
        }
        "type" => {
            if args.len() != 2 {
                return fail_usage("usage: jsontool type <file>");
            }
            out(type_name(&doc));
            ExitCode::SUCCESS
        }
        "sum" => {
            if args.len() != 3 {
                return fail_usage("usage: jsontool sum <file> <field>");
            }
            let field = args[2].as_str();
            let Some(records) = doc.as_array() else {
                err("document is not an array");
                return ExitCode::from(4);
            };
            let mut total: i64 = 0;
            for record in records {
                if let Some(value) = record.get(field) {
                    if let Some(number) = value.as_i64() {
                        total += number;
                    }
                }
            }
            out(&total.to_string());
            ExitCode::SUCCESS
        }
        "compact" => {
            if args.len() != 2 {
                return fail_usage("usage: jsontool compact <file>");
            }
            out(&compact(&doc));
            ExitCode::SUCCESS
        }
        _ => fail_usage(USAGE),
    }
}
