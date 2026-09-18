//! Embedded SQL database integration tests — Windows Python parity S41.
//!
//! Python parity target: the `sqlite3` module's core capability — an embedded
//! relational store with SQL creation, insertion, querying, updates, deletes
//! and transactions.
//!
//! Every test prepends `stdlib/sqlite.mink`, compiles a genuine Windows PE
//! through the real `mink build` CLI and executes it. `rt_exit(0)` runs the
//! runtime arena validation, so a clean exit (no `E-R06`, exit code 106) is
//! also the ownership/leak proof for the engine.
//!
//! Generated programs emit a `.` line after every value, so a test asserts on
//! the ordered list of results rather than on raw line offsets.
//!
//! Test artifacts are written under `target/mink-artifacts/` rather than the
//! system temp directory: Windows Defender quarantines freshly linked PEs in
//! `%TEMP%` (see the Session 109 report), which made those harnesses
//! non-deterministic.

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn sqlite_source() -> String {
    std::fs::read_to_string("stdlib/sqlite.mink").expect("failed to read stdlib/sqlite.mink")
}

/// Output sinks and the `ex` helper that runs one statement and hands the
/// updated database back, so generated programs stay readable.
const PRELUDE: &str = r#"
fn say_i(v: Int) { rt_print_int(v); rt_print_str("."); }
fn say_b(v: Bool) { if v { rt_print_str("T"); } else { rt_print_str("F"); } rt_print_str("."); }
fn say_s(s: Str) { rt_print_str(s); rt_print_str("."); rt_str_free(s); }
fn ex(db: Str, sql: Str) -> Str {
    let r = sql_exec(db, sql);
    rt_print_str(r.0);
    rt_print_str(".");
    rt_str_free(r.0);
    return r.1;
}
"#;

fn artifact_dir() -> PathBuf {
    let dir = PathBuf::from("target").join("mink-artifacts");
    std::fs::create_dir_all(&dir).expect("failed to create target/mink-artifacts");
    dir
}

/// Compile and run one generated program. `decls` holds top-level helper
/// functions and `body` holds the statements of `fn main`.
fn build_and_run(decls: &str, body: &str) -> (i32, String) {
    let source = format!(
        "{}\n{}\n{}\nfn main() {{\n{}\n}}\n",
        sqlite_source(),
        PRELUDE,
        decls,
        body
    );
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = artifact_dir().join(format!("sqlite_test_{n}.mink"));
    std::fs::write(&path, source.replace("\r\n", "\n")).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_mink"))
        .arg("build")
        .arg(&path)
        .output()
        .unwrap();
    let exe = path.with_extension("exe");
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let _ = std::fs::remove_file(&exe);
        panic!(
            "build failed (source kept at {}):\n{stderr}",
            path.display()
        );
    }
    let run = Command::new(&exe).output().expect("failed to run test exe");
    let code = run.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&run.stdout).to_string();
    let stderr = String::from_utf8_lossy(&run.stderr).to_string();
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&exe);
    // A clean exit also proves the arena/leak check passed (E-R06 = exit 106).
    assert!(stderr.is_empty(), "unexpected stderr: {stderr:?}");
    (code, stdout)
}

/// Split program output into one entry per emitted value.
fn results(out: &str) -> Vec<String> {
    let mut v: Vec<String> = Vec::new();
    let mut cur = String::new();
    for line in out.lines() {
        if line == "." {
            v.push(cur.trim_end_matches('\n').to_string());
            cur.clear();
        } else {
            if !cur.is_empty() {
                cur.push('\n');
            }
            cur.push_str(line);
        }
    }
    if !cur.is_empty() {
        v.push(cur);
    }
    v
}

/// The shared schema used by most tests: three rows, the third with a NULL.
const SCHEMA: &str = "fn seed() -> Str {\n\
    let mut db = sql_new();\n\
    db = ex(db, \"CREATE TABLE users (id INT, name STR)\");\n\
    db = ex(db, \"INSERT INTO users VALUES (1, 'alice')\");\n\
    db = ex(db, \"INSERT INTO users VALUES (2, 'bob')\");\n\
    db = ex(db, \"INSERT INTO users VALUES (3, NULL)\");\n\
    return db;\n\
}\n";

const SEED: [&str; 4] = ["OK", "OK", "OK", "OK"];

// ============================================================================
// Normal operation
// ============================================================================

#[test]
fn s41_create_insert_select_count() {
    let (code, out) = build_and_run(
        SCHEMA,
        "let mut db = seed();\n\
         let t = sql_tables(db); say_s(t.0); db = t.1;\n\
         db = ex(db, \"SELECT * FROM users\");\n\
         db = ex(db, \"SELECT COUNT(*) FROM users\");\n\
         db = ex(db, \"BEGIN\");\n\
         db = ex(db, \"COMMIT\");\n\
         sql_free(db);\n",
    );
    assert_eq!(code, 0, "exit={code}\n{out}");
    let r = results(&out);
    assert_eq!(&r[0..4], &SEED);
    assert_eq!(
        &r[4..],
        &["users", "1\talice\n2\tbob\n3\tNULL", "3", "OK", "OK"]
    );
}

#[test]
fn s41_where_operators() {
    let mut body = String::from("let mut db = seed();\n");
    for op in ["=", "!=", "<", ">", "<=", ">="] {
        body.push_str(&format!(
            "db = ex(db, \"SELECT * FROM users WHERE id {op} 2\");\n"
        ));
    }
    body.push_str("sql_free(db);\n");
    let (code, out) = build_and_run(SCHEMA, &body);
    assert_eq!(code, 0, "exit={code}\n{out}");
    let r = results(&out);
    assert_eq!(&r[0..4], &SEED);
    assert_eq!(
        &r[4..],
        &[
            "2\tbob",
            "1\talice\n3\tNULL",
            "1\talice",
            "3\tNULL",
            "1\talice\n2\tbob",
            "2\tbob\n3\tNULL",
        ]
    );
}

#[test]
fn s41_null_handling() {
    let (code, out) = build_and_run(
        SCHEMA,
        "let mut db = seed();\n\
         db = ex(db, \"INSERT INTO users VALUES (4, '')\");\n\
         db = ex(db, \"SELECT COUNT(*) FROM users WHERE name IS NULL\");\n\
         db = ex(db, \"SELECT COUNT(*) FROM users WHERE name IS NOT NULL\");\n\
         db = ex(db, \"SELECT * FROM users WHERE name IS NULL\");\n\
         db = ex(db, \"SELECT * FROM users WHERE id = 4\");\n\
         sql_free(db);\n",
    );
    assert_eq!(code, 0, "exit={code}\n{out}");
    let r = results(&out);
    // NULL is distinct from the empty string: only row 3 is NULL.
    assert_eq!(&r[4..], &["OK", "1", "3", "3\tNULL", "4\t"]);
}

#[test]
fn s41_types_negative_and_large() {
    let (code, out) = build_and_run(
        "",
        "let mut db = sql_new();\n\
         db = ex(db, \"CREATE TABLE n (v INT, s STR)\");\n\
         db = ex(db, \"INSERT INTO n VALUES (0, 'zero')\");\n\
         db = ex(db, \"INSERT INTO n VALUES (-42, 'neg')\");\n\
         db = ex(db, \"INSERT INTO n VALUES (9007199254740991, 'big')\");\n\
         db = ex(db, \"SELECT * FROM n WHERE v < 0\");\n\
         db = ex(db, \"SELECT * FROM n WHERE v >= 9007199254740991\");\n\
         db = ex(db, \"SELECT * FROM n WHERE v != 0\");\n\
         db = ex(db, \"SELECT COUNT(*) FROM n\");\n\
         sql_free(db);\n",
    );
    assert_eq!(code, 0, "exit={code}\n{out}");
    let r = results(&out);
    assert_eq!(
        &r[4..],
        &[
            "-42\tneg",
            "9007199254740991\tbig",
            "-42\tneg\n9007199254740991\tbig",
            "3",
        ]
    );
}

#[test]
fn s41_quoted_strings_and_quotes() {
    let (code, out) = build_and_run(
        "",
        "let mut db = sql_new();\n\
         db = ex(db, \"CREATE TABLE t (a STR, b STR)\");\n\
         db = ex(db, \"INSERT INTO t VALUES ('O''Brien', 'two words')\");\n\
         db = ex(db, \"INSERT INTO t VALUES ('', 'tail')\");\n\
         db = ex(db, \"SELECT * FROM t WHERE a = 'O''Brien'\");\n\
         db = ex(db, \"SELECT COUNT(*) FROM t\");\n\
         sql_free(db);\n",
    );
    assert_eq!(code, 0, "exit={code}\n{out}");
    let r = results(&out);
    assert_eq!(&r[3..], &["O'Brien\ttwo words", "2"]);
}

#[test]
fn s41_column_list_insert() {
    let (code, out) = build_and_run(
        "",
        "let mut db = sql_new();\n\
         db = ex(db, \"CREATE TABLE t (a INT, b STR, c STR)\");\n\
         db = ex(db, \"INSERT INTO t (c, a) VALUES ('only-c', 7)\");\n\
         db = ex(db, \"INSERT INTO t (a, b, c) VALUES (1, 'x', 'y')\");\n\
         db = ex(db, \"SELECT * FROM t WHERE a = 7\");\n\
         db = ex(db, \"SELECT COUNT(*) FROM t\");\n\
         sql_free(db);\n",
    );
    assert_eq!(code, 0, "exit={code}\n{out}");
    let r = results(&out);
    assert_eq!(&r[3..], &["7\tNULL\tonly-c", "2"]);
}

#[test]
fn s41_update_and_delete() {
    let (code, out) = build_and_run(
        SCHEMA,
        "let mut db = seed();\n\
         db = ex(db, \"UPDATE users SET name = 'ALICE' WHERE id = 1\");\n\
         db = ex(db, \"DELETE FROM users WHERE id = 3\");\n\
         db = ex(db, \"SELECT * FROM users\");\n\
         db = ex(db, \"UPDATE users SET name = 'all'\");\n\
         db = ex(db, \"DELETE FROM users\");\n\
         db = ex(db, \"SELECT COUNT(*) FROM users\");\n\
         sql_free(db);\n",
    );
    assert_eq!(code, 0, "exit={code}\n{out}");
    let r = results(&out);
    assert_eq!(
        &r[4..],
        &["OK 1", "OK 1", "1\tALICE\n2\tbob", "OK 2", "OK 2", "0"]
    );
}

#[test]
fn s41_transactions_commit_and_rollback() {
    let (code, out) = build_and_run(
        SCHEMA,
        "let mut db = seed();\n\
         db = ex(db, \"BEGIN\");\n\
         db = ex(db, \"INSERT INTO users VALUES (10, 'ten')\");\n\
         db = ex(db, \"INSERT INTO users VALUES (11, 'eleven')\");\n\
         db = ex(db, \"ROLLBACK\");\n\
         db = ex(db, \"SELECT COUNT(*) FROM users\");\n\
         db = ex(db, \"BEGIN\");\n\
         db = ex(db, \"INSERT INTO users VALUES (12, 'twelve')\");\n\
         db = ex(db, \"COMMIT\");\n\
         db = ex(db, \"SELECT COUNT(*) FROM users\");\n\
         db = ex(db, \"SELECT * FROM users WHERE id = 12\");\n\
         sql_free(db);\n",
    );
    assert_eq!(code, 0, "exit={code}\n{out}");
    let r = results(&out);
    assert_eq!(
        &r[4..],
        &[
            "OK",
            "OK",
            "OK",
            "OK",
            "3",
            "OK",
            "OK",
            "OK",
            "4",
            "12\ttwelve",
        ]
    );
}

// ============================================================================
// Malformed and invalid input
// ============================================================================

#[test]
fn s41_malformed_sql_is_rejected() {
    let cases = [
        ("", "ERROR: empty statement"),
        ("   ", "ERROR: empty statement"),
        ("DROP TABLE t", "ERROR: unsupported statement"),
        (
            "SELECT 1",
            "ERROR: only SELECT * / SELECT COUNT(*) is supported",
        ),
        ("SELECT * FROM nope", "ERROR: no such table"),
        ("INSERT INTO nope VALUES (1)", "ERROR: no such table"),
        ("UPDATE nope SET a = 1", "ERROR: no such table"),
        ("DELETE FROM nope", "ERROR: no such table"),
        ("CREATE TABLE", "ERROR: missing table name"),
        ("CREATE TABLE x", "ERROR: expected '(' after table name"),
        (
            "CREATE TABLE x (a INT",
            "ERROR: expected ')' after column list",
        ),
        ("CREATE TABLE x (a BLOB)", "ERROR: unsupported column type"),
        ("CREATE TABLE x ()", "ERROR: malformed column list"),
        ("INSERT INTO t VALUES ()", "ERROR: wrong number of values"),
        (
            "INSERT INTO t VALUES (1, 2)",
            "ERROR: wrong number of values",
        ),
        (
            "INSERT INTO t (zz) VALUES (1)",
            "ERROR: unknown column in INSERT",
        ),
        (
            "INSERT INTO t (a) VALUES (1",
            "ERROR: expected ')' after values",
        ),
        (
            "SELECT * FROM t WHERE zz = 1",
            "ERROR: unknown column in WHERE",
        ),
        (
            "SELECT * FROM t WHERE a IS",
            "ERROR: unknown column in WHERE",
        ),
        ("UPDATE t SET zz = 1", "ERROR: unknown column in SET"),
        ("UPDATE t SET a 1", "ERROR: expected '=' in SET"),
        ("COMMIT", "ERROR: no active transaction"),
        ("ROLLBACK", "ERROR: no active transaction"),
        ("BEGIN", "OK"),
    ];
    let mut body = String::from("let mut db = sql_new();\n");
    body.push_str("db = ex(db, \"CREATE TABLE t (a INT)\");\n");
    // One real row, so WHERE conditions are actually evaluated (a table with no
    // rows never reaches the condition).
    body.push_str("db = ex(db, \"INSERT INTO t VALUES (5)\");\n");
    for (sql, _) in cases.iter() {
        body.push_str(&format!("db = ex(db, \"{}\");\n", sql));
    }
    body.push_str("let r = sql_rows(db, \"t\"); say_i(r.0); db = r.1;\n");
    body.push_str("sql_free(db);\n");
    let (code, out) = build_and_run("", &body);
    assert_eq!(code, 0, "exit={code}\n{out}");
    let r = results(&out);
    assert_eq!(r[0], "OK"); // CREATE TABLE t
    assert_eq!(r[1], "OK"); // INSERT INTO t VALUES (5)
    for (i, (sql, want)) in cases.iter().enumerate() {
        assert_eq!(r[2 + i], *want, "case {i}: {sql:?}");
    }
    // No malformed statement changed the row count.
    assert_eq!(r[2 + cases.len()], "1");
}

#[test]
fn s41_missing_parens_and_duplicates() {
    let (code, out) = build_and_run(
        "",
        "let mut db = sql_new();\n\
         db = ex(db, \"CREATE TABLE t (a INT)\");\n\
         db = ex(db, \"CREATE TABLE t (a INT)\");\n\
         db = ex(db, \"CREATE TABLE u (a INT, b STR)\");\n\
         db = ex(db, \"SELECT COUNT(*) FROM u\");\n\
         db = ex(db, \"SELECT * FROM t WHERE a = 1\");\n\
         sql_free(db);\n",
    );
    assert_eq!(code, 0, "exit={code}\n{out}");
    let r = results(&out);
    assert_eq!(
        &r[..],
        &["OK", "ERROR: table already exists", "OK", "0", "",]
    );
}

#[test]
fn s41_control_byte_literal_rejected() {
    let (code, out) = build_and_run(
        "",
        "let mut db = sql_new();\n\
         db = ex(db, \"CREATE TABLE t (a STR)\");\n\
         db = ex(db, \"INSERT INTO t VALUES ('bad\\x01lit')\");\n\
         db = ex(db, \"INSERT INTO t VALUES ('two\\nlines')\");\n\
         db = ex(db, \"SELECT COUNT(*) FROM t\");\n\
         sql_free(db);\n",
    );
    assert_eq!(code, 0, "exit={code}\n{out}");
    let r = results(&out);
    assert_eq!(
        &r[1..],
        &[
            "ERROR: control byte in string literal",
            "ERROR: control byte in string literal",
            "0",
        ]
    );
}

// ============================================================================
// Empty input, accessors, repeated use, ownership
// ============================================================================

#[test]
fn s41_empty_database() {
    let (code, out) = build_and_run(
        "",
        "let mut db = sql_new();\n\
         db = ex(db, \"SELECT COUNT(*) FROM nope\");\n\
         let r0 = sql_rows(db, \"nope\"); say_i(r0.0); db = r0.1;\n\
         let r1 = sql_has_table(db, \"nope\"); say_b(r1.0); db = r1.1;\n\
         let r2 = sql_tables(db); say_s(r2.0); db = r2.1;\n\
         let r3 = sql_in_transaction(db); say_b(r3.0); db = r3.1;\n\
         sql_free(db);\n",
    );
    assert_eq!(code, 0, "exit={code}\n{out}");
    let r = results(&out);
    assert_eq!(&r[..], &["ERROR: no such table", "-1", "F", "", "F",]);
}

#[test]
fn s41_accessors_keep_db_usable() {
    let (code, out) = build_and_run(
        SCHEMA,
        "let mut db = seed();\n\
         let r1 = sql_rows(db, \"users\"); say_i(r1.0); db = r1.1;\n\
         let r2 = sql_has_table(db, \"users\"); say_b(r2.0); db = r2.1;\n\
         let r3 = sql_in_transaction(db); say_b(r3.0); db = r3.1;\n\
         let r4 = sql_tables(db); say_s(r4.0); db = r4.1;\n\
         let r5 = sql_rows(db, \"users\"); say_i(r5.0); db = r5.1;\n\
         say_s(sql_quote(\"a'b\"));\n\
         let r6 = sql_in_transaction(db); say_b(r6.0); db = r6.1;\n\
         sql_free(db);\n",
    );
    assert_eq!(code, 0, "exit={code}\n{out}");
    let r = results(&out);
    assert_eq!(&r[4..], &["3", "T", "F", "users", "3", "'a''b'", "F"]);
}

#[test]
fn s41_repeated_operations_are_leak_free() {
    // 300 insert/select/update/delete cycles inside one program. A clean exit
    // is the ownership/leak proof (a leak makes the runtime exit 106 / E-R06).
    let (code, out) = build_and_run(
        "",
        "let mut db = sql_new();\n\
         db = ex(db, \"CREATE TABLE m (i INT, s STR)\");\n\
         let mut k = 0;\n\
         while k < 300 {\n\
             db = ex(db, \"INSERT INTO m VALUES (1, 'cycle')\");\n\
             db = ex(db, \"SELECT * FROM m WHERE i = 1\");\n\
             db = ex(db, \"UPDATE m SET s = 'done' WHERE i = 1\");\n\
             db = ex(db, \"DELETE FROM m WHERE i = 1\");\n\
             k = k + 1;\n\
         }\n\
         let r = sql_rows(db, \"m\"); say_i(r.0); db = r.1;\n\
         sql_free(db);\n",
    );
    assert_eq!(code, 0, "exit={code}\n{out}");
    let r = results(&out);
    assert_eq!(r.len(), 1 + 300 * 4 + 1);
    assert_eq!(r[r.len() - 1], "0");
}

#[test]
fn s41_realistic_workload() {
    // 200 orders: insert, count, update a status, then delete everything inside
    // a transaction and roll the batch back.
    let (code, out) = build_and_run(
        "",
        "let mut db = sql_new();\n\
         db = ex(db, \"CREATE TABLE orders (id INT, cust STR, amount INT, status STR)\");\n\
         let mut i = 0;\n\
         while i < 200 {\n\
             db = ex(db, \"INSERT INTO orders VALUES (7, 'cust', 100, 'new')\");\n\
             i = i + 1;\n\
         }\n\
         db = ex(db, \"SELECT COUNT(*) FROM orders\");\n\
         let r = sql_rows(db, \"orders\"); say_i(r.0); db = r.1;\n\
         db = ex(db, \"UPDATE orders SET status = 'paid' WHERE status = 'new'\");\n\
         db = ex(db, \"SELECT COUNT(*) FROM orders WHERE status = 'paid'\");\n\
         db = ex(db, \"SELECT * FROM orders WHERE status = 'new'\");\n\
         db = ex(db, \"BEGIN\");\n\
         db = ex(db, \"DELETE FROM orders\");\n\
         db = ex(db, \"SELECT COUNT(*) FROM orders\");\n\
         db = ex(db, \"ROLLBACK\");\n\
         db = ex(db, \"SELECT COUNT(*) FROM orders\");\n\
         db = ex(db, \"SELECT COUNT(*) FROM orders WHERE id = 7\");\n\
         sql_free(db);\n",
    );
    assert_eq!(code, 0, "exit={code}\n{out}");
    let r = results(&out);
    let n = r.len();
    assert_eq!(&r[n - 5..], &["OK 200", "0", "OK", "200", "200"]);
    assert_eq!(r[201], "200"); // COUNT(*)
    assert_eq!(r[202], "200"); // sql_rows
    assert_eq!(r[203], "OK 200"); // UPDATE
    assert_eq!(r[204], "200"); // COUNT(*) WHERE status = 'paid'
    assert_eq!(r[205], ""); // no rows are still 'new'
}

#[test]
fn s41_unicode_values_survive_round_trip() {
    // UTF-8 byte sequences inside string values (the engine is byte-clean).
    let (code, out) = build_and_run(
        "",
        "let mut db = sql_new();\n\
         db = ex(db, \"CREATE TABLE u (k STR, v STR)\");\n\
         db = ex(db, \"INSERT INTO u VALUES ('caf\\xc3\\xa9', 'na\\xc3\\xafve')\");\n\
         db = ex(db, \"INSERT INTO u VALUES ('\\xe6\\x97\\xa5\\xe6\\x9c\\xac', 'x')\");\n\
         db = ex(db, \"SELECT * FROM u WHERE k = 'caf\\xc3\\xa9'\");\n\
         db = ex(db, \"SELECT COUNT(*) FROM u\");\n\
         db = ex(db, \"SELECT * FROM u WHERE k = '\\xe6\\x97\\xa5\\xe6\\x9c\\xac'\");\n\
         sql_free(db);\n",
    );
    assert_eq!(code, 0, "exit={code}\n{out}");
    let r = results(&out);
    assert_eq!(r[3], "caf\u{e9}\tna\u{ef}ve");
    assert_eq!(r[4], "2");
    assert_eq!(r[5], "\u{65e5}\u{672c}\tx");
}
