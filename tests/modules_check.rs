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
