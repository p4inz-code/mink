//! M4 W2 `netcheck` — Rust port.
//!
//! Contract: `docs/benchmarks/M4/M4_SESSION_153.md`.
//!
//!     netcheck <host> <port> <path>
//!
//! Transport failures and completed responses without a status line share
//! exit 2 and one stable message, matching the contract.

use std::io::{Read, Write};
use std::process::ExitCode;

use serde_json::Value;

const USAGE: &str = "usage: netcheck <host> <port> <path>";

fn out(text: &str) {
    let mut stdout = std::io::stdout();
    let _ = stdout.write_all(text.as_bytes());
    let _ = stdout.write_all(b"\r\n");
    let _ = stdout.flush();
}

fn fail(label: &str, code: u8) -> ExitCode {
    let mut stderr = std::io::stderr();
    let _ = stderr.write_all(format!("netcheck: {label}\r\n").as_bytes());
    let _ = stderr.flush();
    ExitCode::from(code)
}

/// One GET. A 4xx/5xx is a completed response, not an error.
fn fetch(url: &str) -> Result<(u16, Vec<u8>), ()> {
    let result = ureq::get(url)
        .timeout(std::time::Duration::from_secs(10))
        .call();
    let response = match result {
        Ok(response) => response,
        Err(ureq::Error::Status(_code, response)) => response,
        Err(_) => return Err(()),
    };
    let status = response.status();
    let mut body = Vec::new();
    response
        .into_reader()
        .read_to_end(&mut body)
        .map_err(|_| ())?;
    Ok((status, body))
}

/// `None` means the body is not JSON; `Some(None)` means valid JSON without
/// an integral `value`; `Some(Some(n))` means an integral value was found.
fn integral_value(body: &[u8]) -> Option<Option<i64>> {
    match serde_json::from_slice::<Value>(body) {
        Err(_) => None,
        Ok(Value::Object(map)) => Some(map.get("value").and_then(|value| value.as_i64())),
        Ok(_) => Some(None),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.iter().any(|arg| arg.starts_with('-')) {
        return fail("this tool has no options", 1);
    }
    if args.len() != 3 {
        return fail(USAGE, 1);
    }

    let port: u16 = match args[1].parse() {
        Ok(port) if port >= 1 => port,
        _ => return fail(USAGE, 1),
    };

    let url = format!("http://{}:{}{}", args[0], port, args[2]);
    let (status, body) = match fetch(&url) {
        Ok(exchange) => exchange,
        Err(()) => return fail("connection failed", 2),
    };

    let parsed = integral_value(&body);
    if status < 400 {
        match parsed {
            None => return fail("malformed JSON", 3),
            Some(None) => return fail("missing value", 3),
            Some(Some(_)) => {}
        }
    }

    out(&format!("STATUS {status}"));
    out(&format!("BODY {}", body.len()));
    if let Some(Some(value)) = parsed {
        out(&format!("VALUE {value}"));
    }
    ExitCode::SUCCESS
}
