//! M4 W3 `dbshell` — Rust port over in-memory SQLite (`rusqlite`, bundled).
//!
//! Contract: `docs/benchmarks/M4/M4_SESSION_154.md`.
//!
//! BUILD STATUS: blocked offline. `rusqlite` is absent from the local cargo
//! cache, so this crate has not been compiled or run. The source is complete
//! against the documented `rusqlite` API and is committed as blocked work.

use std::io::{BufRead, Write};
use std::path::Path;
use std::process::ExitCode;

use rusqlite::types::ValueRef;
use rusqlite::Connection;

const USAGE: &str = "usage: dbshell <script>";

fn out(text: &str) {
    let mut stdout = std::io::stdout();
    let _ = stdout.write_all(text.as_bytes());
    let _ = stdout.write_all(b"\r\n");
    let _ = stdout.flush();
}

fn fail(label: &str, code: u8) -> ExitCode {
    let mut stderr = std::io::stderr();
    let _ = stderr.write_all(format!("dbshell: {label}\r\n").as_bytes());
    let _ = stderr.flush();
    ExitCode::from(code)
}

fn normalize(message: &str) -> String {
    if message.contains("no such table") {
        return "no such table".to_string();
    }
    if message.contains("no such column") {
        return "unknown column in WHERE".to_string();
    }
    message.to_string()
}

fn render(connection: &Connection, statement: &str) -> Result<String, rusqlite::Error> {
    let kind = statement
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_ascii_uppercase();

    if kind == "SELECT" {
        if statement.to_ascii_uppercase().contains("COUNT(") {
            let count: i64 = connection.query_row(statement, [], |row| row.get(0))?;
            return Ok(count.to_string());
        }
        let mut prepared = connection.prepare(statement)?;
        let columns = prepared.column_count();
        let mut rows = prepared.query([])?;
        let mut rendered = String::new();
        while let Some(row) = rows.next()? {
            for index in 0..columns {
                if index > 0 {
                    rendered.push('\t');
                }
                match row.get_ref(index)? {
                    ValueRef::Null => rendered.push_str("NULL"),
                    ValueRef::Integer(value) => rendered.push_str(&value.to_string()),
                    ValueRef::Real(value) => rendered.push_str(&value.to_string()),
                    ValueRef::Text(bytes) | ValueRef::Blob(bytes) => {
                        rendered.push_str(&String::from_utf8_lossy(bytes));
                    }
                }
            }
            rendered.push('\n');
        }
        return Ok(rendered);
    }

    connection.execute(statement, [])?;
    if kind == "UPDATE" || kind == "DELETE" {
        return Ok(format!("OK {}", connection.changes()));
    }
    Ok("OK".to_string())
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
        return fail(&format!("script does not exist: {path}"), 2);
    }
    if !Path::new(path).is_file() {
        return fail(&format!("not a regular file: {path}"), 2);
    }

    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(_) => return fail(&format!("script does not exist: {path}"), 2),
    };
    let connection = match Connection::open_in_memory() {
        Ok(connection) => connection,
        Err(_) => return fail("cannot open in-memory database", 3),
    };

    for line in std::io::BufReader::new(file).lines() {
        let line = match line {
            Ok(line) => line.trim().to_string(),
            Err(_) => break,
        };
        if line.is_empty() || line.starts_with("--") {
            continue;
        }
        out(&format!("sql> {line}"));
        match render(&connection, &line) {
            Ok(result) => out(&result),
            Err(error) => {
                out(&format!("ERROR: {}", normalize(&error.to_string())));
                return ExitCode::from(3);
            }
        }
    }

    ExitCode::SUCCESS
}
