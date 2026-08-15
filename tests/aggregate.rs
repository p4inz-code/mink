//! Integration tests for the aggregate memory foundation: struct and array
//! declarations, struct/array literals, member/index access, deterministic
//! layout (offsets, alignment, sizes, strides), type diagnostics, and
//! native execution.
//!
//! The rules under test are documented in
//! `docs/implementation/AGGREGATE_TYPES_IMPLEMENTATION.md` and specified in
//! `docs/language/CORE_LANGUAGE.md`.

use std::path::PathBuf;
use std::process::Command;

use mink::parser::{ParseErrorKind, ParseOutput, parse};
use mink::runtime::layout::{LayoutError, array_layout, struct_layout};
use mink::semantics::{SemanticErrorKind, SemanticResult};
use mink::source::{SourceId, SourceMap, Span};
use mink::typecheck::{TypeErrorKind, TypeId, TypeResult};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn parse_src(src: &str) -> ParseOutput {
    let mut map = SourceMap::new();
    let id = map.add("test.mink", src);
    let file = map.get(id).expect("added file is present");
    parse(file)
}

fn parse_errors(src: &str) -> Vec<ParseErrorKind> {
    parse_src(src)
        .parse_errors()
        .iter()
        .map(|e| e.kind())
        .collect()
}

/// Parses, semantically analyzes, and type-checks `src`, asserting that it
/// lexes and parses cleanly (type tests start from valid syntax).
fn check_src(src: &str) -> (SourceMap, mink::ast::Ast, SemanticResult, TypeResult) {
    let mut sources = SourceMap::new();
    let id = sources.add(std::path::Path::new("test.mink"), src);
    let file = sources.get(id).expect("the file just added");
    let parsed = parse(file);
    assert!(
        parsed.is_valid(),
        "test source must lex and parse cleanly\nlex errors: {:?}\nparse errors: {:?}",
        parsed.lex_errors(),
        parsed.parse_errors()
    );
    let (ast, lex_errors, parse_errors) = parsed.into_parts();
    assert!(lex_errors.is_empty() && parse_errors.is_empty());
    let semantic = mink::semantics::analyze(&ast);
    let types = mink::typecheck::check(&ast, &semantic, &sources);
    (sources, ast, semantic, types)
}

/// All type errors of `kind`.
fn type_errors(types: &TypeResult, kind: TypeErrorKind) -> Vec<&mink::typecheck::TypeError> {
    types
        .errors()
        .iter()
        .filter(|error| error.kind() == kind)
        .collect()
}

/// The rendered type name of the symbol `name`.
fn symbol_type_name(types: &TypeResult, semantic: &SemanticResult, name: &str) -> String {
    let symbol = semantic
        .symbols()
        .iter()
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("no symbol named `{name}`"));
    let ty = types
        .symbol_type(symbol.id)
        .unwrap_or_else(|| panic!("no type recorded for `{name}`"));
    types.types().display(ty)
}

/// The type id of the symbol `name`.
fn symbol_type_id(types: &TypeResult, semantic: &SemanticResult, name: &str) -> TypeId {
    let symbol = semantic
        .symbols()
        .iter()
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("no symbol named `{name}`"));
    types
        .symbol_type(symbol.id)
        .unwrap_or_else(|| panic!("no type recorded for `{name}`"))
}

/// Asserts the expression `needle` (appearing exactly once) types as `expected`.
fn assert_expr_type(src: &str, types: &TypeResult, needle: &str, expected: &str) {
    let start = src
        .find(needle)
        .unwrap_or_else(|| panic!("`{needle}` not found"));
    let span = Span::new(
        SourceId::new(0),
        start as u32..start as u32 + needle.len() as u32,
    );
    let ty = types
        .expr_type_exact(span)
        .unwrap_or_else(|| panic!("no type recorded for `{needle}`"));
    assert_eq!(
        types.types().display(ty),
        expected,
        "type of `{needle}` in {src:?}"
    );
}

/// The first error of `kind` in the semantic result.
fn first_semantic(
    semantic: &SemanticResult,
    kind: SemanticErrorKind,
) -> &mink::semantics::SemanticError {
    semantic
        .errors()
        .iter()
        .find(|e| e.kind() == kind)
        .unwrap_or_else(|| panic!("no semantic error of kind {kind:?}"))
}

// ---------------------------------------------------------------------------
// Parsing: struct declarations and literals
// ---------------------------------------------------------------------------

#[test]
fn struct_declaration_parses() {
    let output =
        parse_src("struct Point { x: Int, y: Int }\nfn main() { let p = Point { x: 1, y: 2 }; }");
    assert!(
        output.parse_errors().is_empty(),
        "{:?}",
        output.parse_errors()
    );
    let ast = output.ast();
    let items = ast.items();
    assert_eq!(items.len(), 2);
    let mink::ast::ItemKind::Struct(s) = &items[0].kind else {
        panic!("first item is not a struct");
    };
    assert_eq!(s.name.name, "Point");
    assert_eq!(s.fields.len(), 2);
    assert_eq!(s.fields[0].name.name, "x");
    assert_eq!(s.fields[1].name.name, "y");
}

#[test]
fn struct_with_array_field_and_trailing_comma_parses() {
    let output = parse_src("struct Grid { rows: [Int; 3], name: Str, }");
    assert!(
        output.parse_errors().is_empty(),
        "{:?}",
        output.parse_errors()
    );
    let ast = output.ast();
    let mink::ast::ItemKind::Struct(s) = &ast.items()[0].kind else {
        panic!("not a struct");
    };
    assert_eq!(s.fields.len(), 2);
    let mink::ast::TyKind::Array { len, .. } = &s.fields[0].ty.kind else {
        panic!("first field is not an array type");
    };
    let mink::ast::ExprKind::Int = len.kind else {
        panic!("array length is not an integer literal");
    };
}

#[test]
fn struct_literal_expression_parses() {
    let src = "struct P { x: Int } fn f() { let p = P { x: 7 }; }";
    let output = parse_src(src);
    assert!(
        output.parse_errors().is_empty(),
        "{:?}",
        output.parse_errors()
    );
}

#[test]
fn struct_literal_inside_if_condition_is_a_block_not_a_literal() {
    // `if Name { ... }` opens the block; a struct literal there must be
    // parenthesized.
    let src = "struct P { x: Int } fn f() { if P { x: 1 } { } }";
    let errors = parse_errors(src);
    assert!(
        !errors.is_empty(),
        "an unparenthesized struct literal in a condition must not parse as a literal"
    );
}

#[test]
fn parenthesized_struct_literal_in_condition_parses() {
    let src = "struct P { x: Int } fn f() { if (P { x: 1 }) { } }";
    let output = parse_src(src);
    assert!(
        output.parse_errors().is_empty(),
        "{:?}",
        output.parse_errors()
    );
}

#[test]
fn array_literal_parses_with_trailing_comma() {
    let src = "fn f() { let a = [1, 2, 3,]; }";
    let output = parse_src(src);
    assert!(
        output.parse_errors().is_empty(),
        "{:?}",
        output.parse_errors()
    );
}

#[test]
fn malformed_struct_declarations_report_structured_errors() {
    // Missing '{'.
    assert_eq!(
        parse_errors("struct P x: Int }"),
        vec![ParseErrorKind::ExpectedBlock]
    );
    // Missing field type.
    assert_eq!(
        parse_errors("struct P { x: }"),
        vec![ParseErrorKind::ExpectedType]
    );
    // Missing ':'.
    assert_eq!(
        parse_errors("struct P { x Int }"),
        vec![ParseErrorKind::ExpectedColon]
    );
    // Unclosed brace.
    assert!(parse_errors("struct P { x: Int").contains(&ParseErrorKind::UnclosedBrace));
}

#[test]
fn bad_struct_then_valid_item_recovers() {
    // Recovery must not swallow the following declaration.
    let src = "struct P { x }\nfn f() { return 1; }";
    let output = parse_src(src);
    assert!(!output.parse_errors().is_empty());
    let ast = output.ast();
    assert!(
        ast.items()
            .iter()
            .any(|i| matches!(&i.kind, mink::ast::ItemKind::Fn(_))),
        "the function after the malformed struct must still parse"
    );
}

#[test]
fn struct_spans_cover_the_whole_declaration() {
    let src = "struct P { x: Int, y: Int }";
    let output = parse_src(src);
    let ast = output.ast();
    let mink::ast::ItemKind::Struct(s) = &ast.items()[0].kind else {
        panic!("not a struct");
    };
    assert_eq!(s.span.start(), 0);
    assert_eq!(s.span.end(), src.len() as u32);
    assert_eq!(s.fields[0].name.span.start(), 11);
}

// ---------------------------------------------------------------------------
// Semantics: duplicate structs and fields
// ---------------------------------------------------------------------------

#[test]
fn duplicate_struct_names_are_e_s08() {
    let (_s, _a, semantic, _t) = check_src("struct P { x: Int } struct P { y: Int } fn main() {}");
    let error = first_semantic(&semantic, SemanticErrorKind::DuplicateStruct);
    assert_eq!(error.kind().code(), "E-S08");
}

#[test]
fn duplicate_fields_are_e_s09() {
    let (_s, _a, semantic, _t) = check_src("struct P { x: Int, x: Int } fn main() {}");
    let error = first_semantic(&semantic, SemanticErrorKind::DuplicateField);
    assert_eq!(error.kind().code(), "E-S09");
}

#[test]
fn duplicate_fields_in_distinct_structs_are_fine() {
    let (_s, _a, semantic, _t) = check_src("struct P { x: Int } struct Q { x: Int } fn main() {}");
    assert!(semantic.errors().is_empty(), "{:?}", semantic.errors());
}

// ---------------------------------------------------------------------------
// Typing: member access, indexing, literals
// ---------------------------------------------------------------------------

#[test]
fn member_access_types_as_the_field_type() {
    let src =
        "struct P { x: Int, name: Str } fn f() { let p = P { x: 1, name: \"a\" }; p.x; p.name; }";
    let (_s, _a, semantic, types) = check_src(src);
    assert!(types.errors().is_empty(), "{:?}", types.errors());
    assert_expr_type(src, &types, "p.x", "Int");
    assert_expr_type(src, &types, "p.name", "Str");
    assert_eq!(symbol_type_name(&types, &semantic, "p"), "P");
}

#[test]
fn index_access_types_as_the_element_type() {
    let src = "fn f() { let a = [1, 2, 3]; a[0]; }";
    let (_s, _a, semantic, types) = check_src(src);
    assert!(types.errors().is_empty(), "{:?}", types.errors());
    assert_expr_type(src, &types, "a[0]", "Int");
    assert_eq!(symbol_type_name(&types, &semantic, "a"), "Array<Int, 3>");
}

#[test]
fn nested_member_and_index_types_resolve() {
    let src = "struct Inner { v: Int } struct Outer { inner: Inner } fn f() { let o = Outer { inner: Inner { v: 5 } }; o.inner.v; }";
    let (_s, _a, _semantic, types) = check_src(src);
    assert!(types.errors().is_empty(), "{:?}", types.errors());
    assert_expr_type(src, &types, "o.inner.v", "Int");
    assert_expr_type(src, &types, "o.inner", "Inner");
}

#[test]
fn array_of_structs_indexing_types_as_the_struct() {
    let src = "struct P { x: Int } fn f() { let ps = [P { x: 1 }, P { x: 2 }]; ps[0]; ps[1].x; }";
    let (_s, _a, _semantic, types) = check_src(src);
    assert!(types.errors().is_empty(), "{:?}", types.errors());
    assert_expr_type(src, &types, "ps[0]", "P");
    assert_expr_type(src, &types, "ps[1].x", "Int");
}

#[test]
fn struct_literal_requires_all_fields_e_t13() {
    let src = "struct P { x: Int, y: Int } fn f() { let p = P { x: 1 }; }";
    let (_s, _a, _semantic, types) = check_src(src);
    let errors = type_errors(&types, TypeErrorKind::MissingStructField);
    assert!(
        !errors.is_empty(),
        "a literal missing a field must be E-T13"
    );
    assert_eq!(errors[0].kind().code(), "E-T13");
}

#[test]
fn struct_literal_unknown_field_is_e_t12() {
    let src = "struct P { x: Int } fn f() { let p = P { z: 1 }; }";
    let (_s, _a, _semantic, types) = check_src(src);
    let errors = type_errors(&types, TypeErrorKind::UnknownStructField);
    assert!(
        !errors.is_empty(),
        "a literal with an unknown field must be E-T12"
    );
    assert_eq!(errors[0].kind().code(), "E-T12");
}

#[test]
fn struct_literal_duplicate_field_is_e_t14() {
    let src = "struct P { x: Int } fn f() { let p = P { x: 1, x: 2 }; }";
    let (_s, _a, _semantic, types) = check_src(src);
    let errors = type_errors(&types, TypeErrorKind::DuplicateFieldInit);
    assert!(
        !errors.is_empty(),
        "a literal with a duplicate field must be E-T14"
    );
    assert_eq!(errors[0].kind().code(), "E-T14");
}

#[test]
fn member_on_non_struct_is_e_t07() {
    let src = "fn f() { let a = 1; a.x; }";
    let (_s, _a, _semantic, types) = check_src(src);
    let errors = type_errors(&types, TypeErrorKind::MemberAccessOnNonStruct);
    assert!(!errors.is_empty(), "member access on an Int must be E-T07");
    assert_eq!(errors[0].kind().code(), "E-T07");
}

#[test]
fn unknown_member_is_e_t08() {
    let src = "struct P { x: Int } fn f() { let p = P { x: 1 }; p.y; }";
    let (_s, _a, _semantic, types) = check_src(src);
    let errors = type_errors(&types, TypeErrorKind::UnknownMember);
    assert!(!errors.is_empty(), "an unknown member must be E-T08");
    assert_eq!(errors[0].kind().code(), "E-T08");
}

#[test]
fn index_on_non_array_is_e_t09() {
    let src = "fn f() { let a = 1; a[0]; }";
    let (_s, _a, _semantic, types) = check_src(src);
    let errors = type_errors(&types, TypeErrorKind::IndexOnNonArray);
    assert!(!errors.is_empty(), "indexing an Int must be E-T09");
    assert_eq!(errors[0].kind().code(), "E-T09");
}

#[test]
fn non_integer_index_is_e_t10() {
    let src = "fn f() { let a = [1, 2]; let s = \"x\"; a[s]; }";
    let (_s, _a, _semantic, types) = check_src(src);
    let errors = type_errors(&types, TypeErrorKind::InvalidIndexType);
    assert!(!errors.is_empty(), "a Str index must be E-T10");
    assert_eq!(errors[0].kind().code(), "E-T10");
}

#[test]
fn constant_out_of_range_index_is_e_t11() {
    let src = "fn f() { let a = [1, 2]; a[5]; }";
    let (_s, _a, _semantic, types) = check_src(src);
    let errors = type_errors(&types, TypeErrorKind::IndexOutOfRange);
    assert!(!errors.is_empty(), "a constant index >= len must be E-T11");
    assert_eq!(errors[0].kind().code(), "E-T11");
}

#[test]
fn in_range_constant_index_is_accepted() {
    let src = "fn f() { let a = [1, 2]; a[1]; }";
    let (_s, _a, _semantic, types) = check_src(src);
    assert!(types.errors().is_empty(), "{:?}", types.errors());
}

#[test]
fn unknown_struct_type_is_e_t15() {
    let src = "fn f() { let p = Missing { x: 1 }; }";
    let (_s, _a, _semantic, types) = check_src(src);
    let errors = type_errors(&types, TypeErrorKind::UnknownType);
    assert!(!errors.is_empty(), "an undeclared struct must be E-T15");
    assert_eq!(errors[0].kind().code(), "E-T15");
}

#[test]
fn invalid_array_length_is_e_t16() {
    let src = "struct P { a: [Int; 0] } fn main() {}";
    let (_s, _a, _semantic, types) = check_src(src);
    let errors = type_errors(&types, TypeErrorKind::InvalidArrayLength);
    assert!(!errors.is_empty(), "a zero-length array type must be E-T16");
    assert_eq!(errors[0].kind().code(), "E-T16");
}

#[test]
fn empty_array_literal_is_e_t17() {
    let src = "fn f() { let a = []; }";
    let (_s, _a, _semantic, types) = check_src(src);
    let errors = type_errors(&types, TypeErrorKind::EmptyArrayLiteral);
    assert!(!errors.is_empty(), "an empty array literal must be E-T17");
    assert_eq!(errors[0].kind().code(), "E-T17");
}

#[test]
fn recursive_struct_layout_is_e_t18() {
    let src = "struct Node { next: Node } fn main() {}";
    let (_s, _a, _semantic, types) = check_src(src);
    let errors = type_errors(&types, TypeErrorKind::InvalidAggregateLayout);
    assert!(!errors.is_empty(), "a recursive struct must be E-T18");
    assert_eq!(errors[0].kind().code(), "E-T18");
}

#[test]
fn empty_struct_layout_is_e_t18() {
    let src = "struct P { } fn main() {}";
    let (_s, _a, _semantic, types) = check_src(src);
    let errors = type_errors(&types, TypeErrorKind::InvalidAggregateLayout);
    assert!(!errors.is_empty(), "an empty struct must be E-T18");
    assert_eq!(errors[0].kind().code(), "E-T18");
}

#[test]
fn member_assignment_requires_mutable_binding() {
    let src = "struct P { x: Int } fn f() { let p = P { x: 1 }; p.x = 2; }";
    let (_s, _a, semantic, _t) = check_src(src);
    assert!(
        semantic
            .errors()
            .iter()
            .any(|e| e.kind() == SemanticErrorKind::AssignmentToImmutable),
        "assigning through a non-mutable binding must be rejected"
    );
}

#[test]
fn member_assignment_types_the_field() {
    let src = "struct P { x: Int } fn f() { let mut p = P { x: 1 }; p.x = 2; }";
    let (_s, _a, _semantic, types) = check_src(src);
    assert!(types.errors().is_empty(), "{:?}", types.errors());
    let _ = _semantic;
}

#[test]
fn member_on_parameter_resolves_at_call_sites() {
    // `p` is only pinned to `Point` by the call in `main`; the member
    // expression inside `f` must still resolve to the field type.
    let src = "struct P { x: Int } fn f(p) { return p.x; } fn main() { let p = P { x: 7 }; rt_print_int(f(p)); }";
    let (_s, _a, _semantic, types) = check_src(src);
    assert!(types.errors().is_empty(), "{:?}", types.errors());
    assert_expr_type(src, &types, "p.x", "Int");
}

#[test]
fn deferred_member_mismatch_is_reported_not_compiled() {
    // The forward pass cannot type `g.tag` (Bool) while `g` is still an
    // unresolved parameter; once the call site pins `g` to `Grid`, the
    // `Int + Bool` sum must be diagnosed instead of silently compiling.
    let src = "struct Grid { tag: Bool } fn f(g) { let mut s = 0; s = s + g.tag; return s; } fn main() { let g = Grid { tag: true }; rt_print_int(f(g)); }";
    let (_s, _a, _semantic, types) = check_src(src);
    let errors = type_errors(&types, TypeErrorKind::InvalidOperator);
    assert!(
        !errors.is_empty(),
        "an Int + Bool sum through a deferred member must be E-T02, got {:?}",
        types.errors()
    );
}

#[test]
fn deferred_member_condition_is_checked_against_bool() {
    // `g.tag` is `Bool`, so it is a valid condition; the parameter
    // resolution must not fabricate an error.
    let src = "struct Grid { tag: Bool } fn f(g) { let mut s = 0; if g.tag { s = s + 1; } return s; } fn main() { let g = Grid { tag: true }; rt_print_int(f(g)); }";
    let (_s, _a, _semantic, types) = check_src(src);
    assert!(types.errors().is_empty(), "{:?}", types.errors());
    assert_expr_type(src, &types, "g.tag", "Bool");
}

#[test]
fn member_assignment_type_mismatch_is_reported() {
    let src = "struct P { x: Int } fn f() { let mut p = P { x: 1 }; p.x = \"s\"; }";
    let (_s, _a, _semantic, types) = check_src(src);
    assert!(
        types
            .errors()
            .iter()
            .any(|e| e.kind() == TypeErrorKind::TypeMismatch),
        "assigning a Str to an Int field must be a type mismatch"
    );
}

// ---------------------------------------------------------------------------
// Deterministic layout
// ---------------------------------------------------------------------------

/// The layout of the struct `name` in `src` (which must declare it).
fn struct_layout_of(src: &str, name: &str) -> mink::runtime::layout::StructLayout {
    let (_s, _a, _semantic, types) = check_src(src);
    let ty = symbol_type_id(&types, &_semantic, name);
    let struct_id = types.types().struct_id(ty).expect("`{name}` is a struct");
    struct_layout(struct_id, types.types()).expect("layout computes")
}

/// The layout of the array type bound to symbol `name` in `src`.
fn array_layout_of(src: &str, name: &str) -> mink::runtime::layout::ArrayLayout {
    let (_s, _a, _semantic, types) = check_src(src);
    let ty = symbol_type_id(&types, &_semantic, name);
    array_layout(ty, types.types()).expect("layout computes")
}

#[test]
fn int_only_struct_has_word_layout() {
    let layout = struct_layout_of(
        "struct P { x: Int, y: Int } fn main() { let p = P { x: 1, y: 2 }; }",
        "p",
    );
    assert_eq!(layout.size, 16);
    assert_eq!(layout.align, 8);
    assert_eq!(layout.fields.len(), 2);
    assert_eq!(layout.fields[0].offset, 0);
    assert_eq!(layout.fields[0].size, 8);
    assert_eq!(layout.fields[1].offset, 8);
    assert_eq!(layout.fields[1].size, 8);
}

#[test]
fn bool_fields_are_one_byte_unaligned() {
    let layout = struct_layout_of(
        "struct F { a: Bool, b: Int } fn main() { let f = F { a: true, b: 1 }; }",
        "f",
    );
    assert_eq!(layout.fields[0].offset, 0);
    assert_eq!(layout.fields[0].size, 1);
    // The Int follows at the next 8-byte boundary.
    assert_eq!(layout.fields[1].offset, 8);
    assert_eq!(layout.fields[1].size, 8);
    assert_eq!(layout.size, 16);
    assert_eq!(layout.align, 8);
}

#[test]
fn mixed_bool_int_struct_is_deterministic() {
    let src = "struct F { a: Bool, b: Int, c: Bool, d: Int } fn main() { let f = F { a: true, b: 1, c: false, d: 2 }; }";
    let layout = struct_layout_of(src, "f");
    assert_eq!(layout.size, 32);
    assert_eq!(layout.align, 8);
    assert_eq!(layout.fields[0].offset, 0); // a: Bool
    assert_eq!(layout.fields[1].offset, 8); // b: Int
    assert_eq!(layout.fields[2].offset, 16); // c: Bool
    assert_eq!(layout.fields[3].offset, 24); // d: Int
}

#[test]
fn nested_struct_layout_inlines_fields() {
    let src = "struct Inner { v: Int } struct Outer { name: Str, inner: Inner } fn main() { let o = Outer { name: \"x\", inner: Inner { v: 1 } }; }";
    let layout = struct_layout_of(src, "o");
    assert_eq!(layout.fields.len(), 2);
    assert_eq!(layout.fields[0].size, 8); // name: Str
    assert_eq!(layout.fields[1].size, 8); // inner: Inner (one Int)
    assert_eq!(layout.fields[1].offset, 8);
    assert_eq!(layout.size, 16);
}

#[test]
fn struct_containing_array_has_stride_layout() {
    let src = "struct B { id: Int, vals: [Int; 4] } fn main() { let b = B { id: 1, vals: [1, 2, 3, 4] }; }";
    let layout = struct_layout_of(src, "b");
    assert_eq!(layout.fields[0].offset, 0);
    assert_eq!(layout.fields[1].offset, 8);
    assert_eq!(layout.fields[1].size, 32);
    assert_eq!(layout.size, 40);
}

#[test]
fn array_layout_stride_is_element_size() {
    let layout = array_layout_of("fn main() { let a = [1, 2, 3]; }", "a");
    assert_eq!(layout.len, 3);
    assert_eq!(layout.elem_size, 8);
    assert_eq!(layout.size, 24);
    assert_eq!(layout.align, 8);
}

#[test]
fn array_of_bools_has_byte_stride() {
    let layout = array_layout_of("fn main() { let a = [true, false, true]; }", "a");
    assert_eq!(layout.len, 3);
    assert_eq!(layout.elem_size, 1);
    assert_eq!(layout.size, 3);
    assert_eq!(layout.align, 1);
}

#[test]
fn array_of_structs_stride_is_the_struct_size() {
    let layout = array_layout_of(
        "struct P { x: Int, y: Int } fn main() { let a = [P { x: 1, y: 2 }, P { x: 3, y: 4 }]; }",
        "a",
    );
    assert_eq!(layout.len, 2);
    assert_eq!(layout.elem_size, 16);
    assert_eq!(layout.size, 32);
    assert_eq!(layout.align, 8);
}

#[test]
fn identical_declarations_yield_identical_layouts() {
    let src = "struct F { a: Bool, b: Int, c: Bool, d: Int } fn main() { let f = F { a: true, b: 1, c: false, d: 2 }; }";
    let l1 = struct_layout_of(src, "f");
    let l2 = struct_layout_of(src, "f");
    assert_eq!(l1, l2, "layout must be deterministic");
}

#[test]
fn recursive_layout_returns_structured_error() {
    let src = "struct Node { next: Node } fn main() {}";
    let (_s, _a, semantic, types) = check_src(src);
    // The checker reports E-T18; also verify the raw layout engine agrees.
    let node = types
        .types()
        .structs()
        .iter()
        .find(|s| s.name == "Node")
        .unwrap();
    // The recursive field's type is the struct type itself; recover its
    // id through the public `struct_id` accessor.
    let field_ty = node.fields[0].ty;
    let struct_id = types
        .types()
        .struct_id(field_ty)
        .expect("the `next` field is the Node struct type");
    let result = struct_layout(struct_id, types.types());
    assert!(matches!(result, Err(LayoutError::Recursive { name }) if name == "Node"));
    assert!(
        !semantic.errors().is_empty() || !types.errors().is_empty(),
        "a recursive struct must be diagnosed somewhere"
    );
}

// ---------------------------------------------------------------------------
// Native execution
// ---------------------------------------------------------------------------

fn mink() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mink"))
}

fn temp_source(name: &str, content: &str) -> PathBuf {
    let path =
        std::env::temp_dir().join(format!("mink_aggregate_test_{}_{name}", std::process::id()));
    std::fs::write(&path, content).unwrap();
    path
}

fn build(source: &str) -> PathBuf {
    let name = std::thread::current()
        .name()
        .unwrap_or("program")
        .replace("::", "_");
    let path = temp_source(&format!("{name}.mink"), source);
    let output = mink().arg("build").arg(&path).output().unwrap();
    assert!(
        output.status.success(),
        "build failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let exe = path.with_extension("exe");
    assert!(exe.exists(), "no executable produced");
    exe
}

fn run(exe: &PathBuf) -> (i32, Vec<u8>) {
    let output = Command::new(exe).output().unwrap();
    let stdout = output.stdout;
    // The native runtime writes `\n`; the Windows console layer may hand
    // text-mode CRLF through pipes, so normalize before comparing.
    let stdout = if stdout.contains(&b'\r') {
        stdout
            .iter()
            .copied()
            .filter(|b| *b != b'\r')
            .collect::<Vec<u8>>()
    } else {
        stdout
    };
    (output.status.code().unwrap_or(-1), stdout)
}

#[test]
fn native_struct_field_access_and_mutation() {
    let exe = build(
        "struct P { x: Int, y: Int }
         fn main() {
             let mut p = P { x: 3, y: 4 };
             p.x = p.x + 10;
             rt_print_int(p.x + p.y);
             return;
         }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "17");
}

#[test]
fn native_nested_member_mutation() {
    let exe = build(
        "struct Inner { v: Int }
         struct Outer { inner: Inner, tag: Int }
         fn main() {
             let mut o = Outer { inner: Inner { v: 5 }, tag: 1 };
             o.inner.v = o.inner.v * 3;
             o.tag = o.tag + 9;
             rt_print_int(o.inner.v + o.tag);
             return;
         }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "25");
}

#[test]
fn native_array_indexing_and_mutation() {
    let exe = build(
        "fn main() {
             let mut a = [10, 20, 30];
             a[1] = a[1] + 5;
             let mut i = 0;
             let mut s = 0;
             while i < 3 {
                 s = s + a[i];
                 i = i + 1;
             }
             rt_print_int(s);
             return;
         }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "65");
}

#[test]
fn native_deep_place_chain() {
    let exe = build(
        "struct Point { x: Int, y: Int }
         struct Grid { rows: [Point; 2] }
         fn main() {
             let mut g = Grid { rows: [Point { x: 1, y: 2 }, Point { x: 3, y: 4 }] };
             g.rows[1].y = 40;
             g.rows[0].x = 10;
             rt_print_int(g.rows[0].x + g.rows[0].y + g.rows[1].x + g.rows[1].y);
             return;
         }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    // 10 + 2 + 3 + 40
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "55");
}

#[test]
fn native_bounds_check_is_e_r10() {
    let exe = build(
        "fn main() {
             let a = [1, 2, 3];
             let i = 5;
             rt_print_int(a[i]);
             return;
         }",
    );
    let (code, _stdout) = run(&exe);
    assert_eq!(code, 110, "out-of-range index is E-R10");
}

#[test]
fn native_negative_index_is_e_r10() {
    let exe = build(
        "fn main() {
             let a = [1, 2, 3];
             let i = 0 - 1;
             rt_print_int(a[i]);
             return;
         }",
    );
    let (code, _stdout) = run(&exe);
    assert_eq!(code, 110);
}

#[test]
fn native_bounds_check_in_place_chain_is_e_r10() {
    let exe = build(
        "struct P { x: Int }
         fn main() {
             let mut ps = [P { x: 1 }, P { x: 2 }];
             let i = 9;
             ps[i].x = 5;
             return;
         }",
    );
    let (code, _stdout) = run(&exe);
    assert_eq!(code, 110);
}

#[test]
fn native_struct_with_bool_fields() {
    let exe = build(
        "struct F { a: Bool, b: Int, c: Bool, d: Int }
         fn main() {
             let f = F { a: true, b: 10, c: false, d: 20 };
             let mut acc = 0;
             if f.a { acc = acc + 1; }
             if f.c { acc = acc + 2; }
             acc = acc + f.b + f.d;
             rt_print_int(acc);
             return;
         }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "31");
}

#[test]
fn native_struct_copy_semantics() {
    let exe = build(
        "struct P { x: Int, vals: [Int; 2] }
         fn main() {
             let mut p = P { x: 1, vals: [10, 20] };
             let mut q = p;
             q.x = 99;
             q.vals[0] = 999;
             rt_print_int(p.x);
             rt_print_int(p.vals[0]);
             rt_print_int(q.x);
             rt_print_int(q.vals[0]);
             return;
         }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "1\n10\n99\n999");
}

#[test]
fn native_struct_param_with_bool_field_and_place_chain() {
    // Exercises the deferred member/index re-typing end to end: `g` is a
    // parameter resolved only by the call in `main`, with a bool field
    // and a deep place chain in the body.
    let exe = build(
        "struct Point { x: Int, y: Int }\n\
         struct Grid { rows: [Point; 2], tag: Bool }\n\
         fn total(g) {\n\
             let mut s = 0;\n\
             s = s + g.rows[0].x;\n\
             s = s + g.rows[1].y;\n\
             if g.tag { s = s + 100; }\n\
             return s;\n\
         }\n\
         fn main() {\n\
             let g = Grid { rows: [Point { x: 1, y: 2 }, Point { x: 3, y: 4 }], tag: true };\n\
             rt_print_int(total(g));\n\
             return;\n\
         }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    // 1 + 4 + 100
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "105");
}

#[test]
fn native_struct_passed_to_function() {
    let exe = build(
        "struct P { x: Int, y: Int }
         fn area(p) { return p.x * p.y; }
         fn main() {
             let p = P { x: 6, y: 7 };
             rt_print_int(area(p));
             return;
         }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "42");
}

#[test]
fn native_aggregate_programs_are_deterministic() {
    let src = "struct P { x: Int, vals: [Int; 3] }
               fn main() {
                   let mut p = P { x: 1, vals: [1, 2, 3] };
                   p.vals[1] = 20;
                   rt_print_int(p.x + p.vals[0] + p.vals[1] + p.vals[2]);
                   return;
               }";
    let exe1 = build(src);
    let exe2 = build(src);
    assert_eq!(
        std::fs::read(&exe1).unwrap(),
        std::fs::read(&exe2).unwrap(),
        "identical sources produce identical images"
    );
    let (code1, out1) = run(&exe1);
    let (code2, out2) = run(&exe2);
    assert_eq!(code1, code2);
    assert_eq!(out1, out2);
    assert_eq!(code1, 0);
    // 1 + 1 + 20 + 3
    assert_eq!(String::from_utf8_lossy(&out1).trim(), "25");
}

#[test]
fn native_struct_containing_string() {
    let exe = build(
        "struct Person { name: Str, age: Int }
         fn main() {
             let p = Person { name: \"alice\", age: 30 };
             rt_print_str(p.name);
             rt_print_int(p.age);
             return;
         }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "alice\n30");
}
