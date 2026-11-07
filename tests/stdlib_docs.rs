//! Session 201 — the standard-library reference must cover the exported surface.
//!
//! Scans every top-level `fn` in `stdlib/*.mink`, skips `_`-prefixed private
//! helpers, and fails if a public function is not mentioned in
//! `docs/language/STANDARD_LIBRARY_REFERENCE.md`. Session 201 found 63 public
//! functions missing; the section 26 appendix and this guard keep the count at
//! zero as the surface grows.

use std::path::PathBuf;

fn reference() -> String {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(root.join("docs/language/STANDARD_LIBRARY_REFERENCE.md"))
        .expect("failed to read the standard library reference")
}

fn is_word(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// Whole-word containment: `log_log` must not satisfy a search for `log`.
fn mentions(haystack: &str, name: &str) -> bool {
    let bytes = haystack.as_bytes();
    let mut from = 0;
    while let Some(offset) = haystack[from..].find(name) {
        let start = from + offset;
        let end = start + name.len();
        let before = start == 0 || !is_word(bytes[start - 1]);
        let after = end == bytes.len() || !is_word(bytes[end]);
        if before && after {
            return true;
        }
        from = start + 1;
    }
    false
}

#[test]
fn every_public_stdlib_function_is_documented() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let text = reference();
    let mut entries: Vec<PathBuf> = std::fs::read_dir(root.join("stdlib"))
        .expect("failed to read stdlib/")
        .map(|entry| entry.expect("failed to read a stdlib entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "mink"))
        .collect();
    entries.sort();

    let mut checked = 0usize;
    let mut missing = Vec::new();
    for path in entries {
        let source = std::fs::read_to_string(&path).expect("failed to read a stdlib source");
        for line in source.lines() {
            let Some(rest) = line.strip_prefix("fn ") else {
                continue;
            };
            let name: String = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            if name.is_empty() || name.starts_with('_') {
                continue;
            }
            checked += 1;
            if !mentions(&text, &name) {
                missing.push(name);
            }
        }
    }

    assert!(
        checked > 400,
        "the scan saw only {checked} public functions; did the stdlib layout change?"
    );
    missing.sort();
    missing.dedup();
    assert!(
        missing.is_empty(),
        "{} public stdlib function(s) are missing from \
         docs/language/STANDARD_LIBRARY_REFERENCE.md: {missing:?}",
        missing.len()
    );
}
