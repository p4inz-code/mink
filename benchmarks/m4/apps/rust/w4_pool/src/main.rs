//! M4 W4 `poolrun` — Rust port.
//!
//! Contract: `docs/benchmarks/M4/M4_SESSION_155.md`.
//!
//! Ranges are disjoint, so subtotals and totals are deterministic; printing
//! order is join order, never completion order.

use std::io::Write;
use std::process::ExitCode;
use std::sync::{Arc, Mutex};
use std::thread;

const USAGE: &str = "usage: poolrun <squares|pipeline> <n> <workers>";

fn out(text: &str) {
    let mut stdout = std::io::stdout();
    let _ = stdout.write_all(text.as_bytes());
    let _ = stdout.write_all(b"\r\n");
    let _ = stdout.flush();
}

fn fail(label: &str) -> ExitCode {
    let mut stderr = std::io::stderr();
    let _ = stderr.write_all(format!("poolrun: {label}\r\n").as_bytes());
    let _ = stderr.flush();
    ExitCode::from(1)
}

fn ranges(n: i64, workers: i64) -> Vec<(i64, i64)> {
    let chunk = n / workers;
    let mut parts = Vec::new();
    for k in 0..workers {
        let start = k * chunk + 1;
        let mut end = (k + 1) * chunk;
        if k == workers - 1 {
            end = n;
        }
        parts.push((start, end));
    }
    parts
}

fn sum_squares(start: i64, end: i64) -> i64 {
    let mut total = 0i64;
    for value in start..=end {
        if end >= start {
            total += value * value;
        }
    }
    total
}

fn expected_squares(n: i64) -> i64 {
    n * (n + 1) * (2 * n + 1) / 6
}

fn run_squares(n: i64, workers: i64) -> ExitCode {
    let parts = ranges(n, workers);
    let accumulator = Arc::new(Mutex::new(0i64));
    let mut handles = Vec::new();

    for (start, end) in parts.iter().copied() {
        let accumulator = Arc::clone(&accumulator);
        handles.push(thread::spawn(move || {
            let subtotal = sum_squares(start, end);
            let mut locked = accumulator.lock().unwrap();
            *locked += subtotal;
            subtotal
        }));
    }

    out(&format!("workers={workers}"));
    out(&format!("n={n}"));
    let mut joined = 0i64;
    for (index, handle) in handles.into_iter().enumerate() {
        let subtotal = handle.join().unwrap();
        joined += subtotal;
        out(&format!("subtotal[{index}]={subtotal}"));
    }

    let locked_total = *accumulator.lock().unwrap();
    let expected = expected_squares(n);
    out(&format!("locked-total={locked_total}"));
    out(&format!("joined-total={joined}"));
    out(&format!("expected={expected}"));

    if locked_total != expected || joined != expected {
        return ExitCode::from(3);
    }
    ExitCode::SUCCESS
}

fn run_pipeline(n: i64, workers: i64) -> ExitCode {
    let parts = ranges(n, workers);
    let slots: Arc<Mutex<Vec<i64>>> = Arc::new(Mutex::new(vec![0i64; workers as usize]));
    let mut handles = Vec::new();

    for (index, (start, end)) in parts.iter().copied().enumerate() {
        let slots = Arc::clone(&slots);
        handles.push(thread::spawn(move || {
            let subtotal = sum_squares(start, end);
            slots.lock().unwrap()[index] = subtotal;
        }));
    }

    out(&format!("workers={workers}"));
    out(&format!("n={n}"));
    for handle in handles {
        let _ = handle.join(); // join barrier: stage 1 before stage 2
    }

    let published = slots.lock().unwrap().clone();
    for (index, value) in published.iter().enumerate() {
        out(&format!("stage1[{index}]={value}"));
    }

    let reducer_slots = Arc::clone(&slots);
    let reducer = thread::spawn(move || reducer_slots.lock().unwrap().iter().sum::<i64>());
    let total = reducer.join().unwrap();
    let expected = expected_squares(n);
    out(&format!("total={total}"));
    out(&format!("expected={expected}"));

    if total != expected {
        return ExitCode::from(3);
    }
    ExitCode::SUCCESS
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.iter().any(|arg| arg.starts_with('-')) {
        return fail("this tool has no options");
    }
    if args.len() != 3 {
        return fail(USAGE);
    }

    let n: i64 = match args[1].parse() {
        Ok(value) if value > 0 => value,
        _ => return fail(USAGE),
    };
    let workers: i64 = match args[2].parse() {
        Ok(value) if value > 0 => value,
        _ => return fail(USAGE),
    };

    match args[0].as_str() {
        "squares" => run_squares(n, workers),
        "pipeline" => run_pipeline(n, workers),
        _ => fail(USAGE),
    }
}
