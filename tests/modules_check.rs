//! Tests for the MINK module system / multi-file compilation (Session 34).

use std::path::Path;

// ======================================================================
// Module discovery
// ======================================================================

#[test]
fn single_file_has_no_mod_declarations() {
    let mut sources = mink::source::SourceMap::new();
    let path = Path::new("tests/modules/single.mink");
    let report = mink::driver::check(&mut sources, path).expect("check should succeed");
    assert!(
        report.errors.is_empty(),
        "no errors expected for single file"
    );
}

#[test]
fn multi_file_discovery_finds_children() {
    let mut sources = mink::source::SourceMap::new();
    let path = Path::new("tests/modules/main.mink");
    let report = mink::driver::check(&mut sources, path).expect("check should succeed");
    assert!(
        report.errors.is_empty(),
        "no errors expected for multi-file: {:?}",
        report.errors
    );
}

// ======================================================================
// Cross-module function calls
// ======================================================================

#[test]
fn cross_module_function_call() {
    let mut sources = mink::source::SourceMap::new();
    let path = Path::new("tests/modules/main.mink");
    let report = mink::driver::build(&mut sources, path, mink::driver::BuildOptions::default())
        .expect("build should succeed");
    assert_eq!(report.functions, 4); // add, multiply, main + private_helper
}

// ======================================================================
// Error: missing module file
// ======================================================================

#[test]
fn missing_module_file_is_reported() {
    let mut sources = mink::source::SourceMap::new();
    let path = Path::new("tests/modules/bad_import.mink");
    let result = mink::driver::check(&mut sources, path);
    assert!(result.is_err(), "should fail for missing module file");
}

#[test]
fn missing_module_file_message_names_the_file() {
    let mut sources = mink::source::SourceMap::new();
    let path = Path::new("tests/modules/bad_import.mink");
    let error = mink::driver::check(&mut sources, path).expect_err("should fail");
    let mink::driver::BuildError::FrontEnd(report) = error else {
        panic!("expected a front-end error, got {error}");
    };
    let rendered = report
        .errors
        .iter()
        .map(|error| error.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    // The message names the missing file rather than wrapping that sentence in
    // the unresolved-*identifier* template (which read
    // "cannot find name `module file '...' not found` in this scope").
    assert!(
        rendered.contains("module file '") && rendered.contains("nonexistent.mink' not found"),
        "the message must name the missing module file: {rendered}"
    );
    assert!(
        !rendered.contains("cannot find name"),
        "a missing `mod` file is not an unresolved identifier: {rendered}"
    );
    // It stays in the unresolved-name category, whose documented causes
    // include a failed module import.
    assert_eq!(report.errors[0].code(), "E-S01", "{rendered}");
}

// ======================================================================
// Single file backward compatibility
// ======================================================================

#[test]
fn single_file_programs_still_work() {
    let mut sources = mink::source::SourceMap::new();
    let path = Path::new("tests/modules/single.mink");
    let report = mink::driver::build(&mut sources, path, mink::driver::BuildOptions::default())
        .expect("build should succeed");
    assert_eq!(report.functions, 1); // just main
}

// ======================================================================
// Multi-module check with multiple public functions
// ======================================================================

#[test]
fn multiple_imported_functions() {
    let mut sources = mink::source::SourceMap::new();
    let path = Path::new("tests/modules/main.mink");
    let report = mink::driver::check(&mut sources, path).expect("check should succeed");
    assert!(report.errors.is_empty(), "errors: {:?}", report.errors);
    assert!(report.mir.is_some(), "MIR should be produced");
}
