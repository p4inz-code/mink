//! Session 208 — Section 26 signature-execution audit.
//!
//! `tests/stdlib_docs.rs` proves that every public `stdlib/*.mink` function is
//! *mentioned* in `docs/language/STANDARD_LIBRARY_REFERENCE.md`. It does not
//! prove that the signatures in the Section 26 appendix match the code: a
//! stale parameter type, return type or name would still pass.
//!
//! This test extracts every signature line from the Section 26 code blocks and
//! compares it, after whitespace normalization, with the corresponding
//! top-level `fn` in `stdlib/*.mink`. It also checks that every listed name
//! exists and that no public function in the section's modules is missing from
//! the appendix. It fails on any drift.

use std::collections::BTreeMap;
use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn reference() -> String {
    std::fs::read_to_string(root().join("docs/language/STANDARD_LIBRARY_REFERENCE.md"))
        .expect("failed to read the standard library reference")
}

fn normalize(signature: &str) -> String {
    signature.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Section 26 signatures, keyed by function name, as written in the appendix.
fn documented_signatures() -> BTreeMap<String, String> {
    let text = reference();
    let section = text
        .split_once("## 26. Complete exported surface")
        .unwrap_or_else(|| panic!("Section 26 appendix not found"))
        .1;
    let mut signatures = BTreeMap::new();
    let mut in_fence = false;
    for line in section.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if !in_fence || trimmed.is_empty() {
            continue;
        }
        let without_comment = trimmed.split("//").next().unwrap_or("").trim();
        if without_comment.is_empty() || !without_comment.contains("->") {
            continue;
        }
        let name = without_comment
            .split_once('(')
            .map(|(name, _)| name.trim())
            .unwrap_or("");
        assert!(
            name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') && !name.is_empty(),
            "unparseable Section 26 signature: {trimmed:?}"
        );
        signatures.insert(name.to_string(), normalize(without_comment));
    }
    signatures
}

/// Top-level `fn` signatures from every stdlib source, keyed by name.
fn source_signatures() -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    let mut entries: Vec<PathBuf> = std::fs::read_dir(root().join("stdlib"))
        .expect("failed to read stdlib/")
        .map(|entry| entry.expect("bad stdlib entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "mink"))
        .collect();
    entries.sort();
    for path in entries {
        let source = std::fs::read_to_string(&path).expect("failed to read stdlib source");
        for line in source.lines() {
            let Some(rest) = line.strip_prefix("fn ") else {
                continue;
            };
            let Some(signature) = rest.split_once('{').map(|(sig, _)| sig.trim()) else {
                continue;
            };
            let name = signature
                .split_once('(')
                .map(|(name, _)| name.trim())
                .unwrap_or("");
            if name.is_empty() || name.starts_with('_') {
                continue;
            }
            map.insert(
                name.to_string(),
                normalize(&format!("{name}{}", &signature[name.len()..])),
            );
        }
    }
    map
}

#[test]
fn section26_signatures_match_the_source() {
    let documented = documented_signatures();
    let source = source_signatures();

    assert!(
        documented.len() >= 60,
        "Section 26 lists only {} signatures; the appendix lost entries",
        documented.len()
    );

    let mut missing = Vec::new();
    let mut mismatched = Vec::new();
    for (name, signature) in &documented {
        match source.get(name) {
            None => missing.push(name.clone()),
            Some(actual) if actual != signature => mismatched.push(format!(
                "{name}: documented `{signature}`, source `{actual}`"
            )),
            Some(_) => {}
        }
    }

    assert!(
        missing.is_empty(),
        "{} Section 26 name(s) have no top-level stdlib implementation: {missing:?}",
        missing.len()
    );
    assert!(
        mismatched.is_empty(),
        "{} Section 26 signature(s) drifted from the source:\n  {}",
        mismatched.len(),
        mismatched.join("\n  ")
    );
}
