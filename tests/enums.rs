//! Integration tests for the enum foundation (session 17): enum
//! declarations, variant paths (`E::V`), nominal enum typing, discriminant
//! layout (single word), enum equality, copying, composition with structs
//! and arrays, diagnostics (parser, semantic, type), MIR/backend lowering,
//! and native execution.
//!
//! The rules under test are documented in
//! `docs/implementation/ENUM_TYPES_IMPLEMENTATION.md` and specified in
//! `docs/language/CORE_LANGUAGE.md`.

use std::path::{Path, PathBuf};
use std::process::Command;

use mink::ast::{Ast, ExprKind, ItemKind};
use mink::backend::{self, BType};
use mink::mir::{
    self, MirConstantKind, MirFn, MirItemKind, MirOperandKind, MirProgram, MirRvalueKind,
    MirStmtKind,
};
use mink::parser::{ParseErrorKind, ParseOutput, parse};
use mink::runtime::layout::scalar_size_align;
use mink::semantics::{SemanticErrorKind, SemanticResult};
use mink::source::SourceMap;
use mink::typecheck::{TypeErrorKind, TypeId, TypeKind, TypeResult};

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
fn check_src(src: &str) -> (SourceMap, Ast, SemanticResult, TypeResult) {
    let mut sources = SourceMap::new();
    let id = sources.add(Path::new("test.mink"), src);
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

/// All semantic errors of `kind`.
fn semantic_errors(
    semantic: &SemanticResult,
    kind: SemanticErrorKind,
) -> Vec<&mink::semantics::SemanticError> {
    semantic
        .errors()
        .iter()
        .filter(|error| error.kind() == kind)
        .collect()
}

/// The first enum item in `ast`, panicking if there is none.
fn first_enum(ast: &Ast) -> &mink::ast::EnumItem {
    ast.items()
        .iter()
        .find_map(|item| match &item.kind {
            ItemKind::Enum(e) => Some(e),
            _ => None,
        })
        .expect("program declares an enum")
}

/// The inferred type of the first symbol named `name`.
fn symbol_type(types: &TypeResult, semantic: &SemanticResult, name: &str) -> TypeId {
    let symbol = semantic
        .symbols()
        .iter()
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("no symbol named `{name}`"));
    types
        .symbol_type(symbol.id)
        .unwrap_or_else(|| panic!("symbol `{name}` has no type"))
}

// ---------------------------------------------------------------------------
// Parser: declarations and variant paths
// ---------------------------------------------------------------------------

#[test]
fn enum_declaration_parses() {
    let src = "enum Direction { North, South, East, West } fn main() {}";
    let parsed = parse_src(src);
    assert!(parsed.is_valid(), "{:?}", parsed.parse_errors());
    let (ast, _, _) = parsed.into_parts();
    let e = first_enum(&ast);
    assert_eq!(e.name.name, "Direction");
    assert_eq!(
        e.variants
            .iter()
            .map(|v| v.name.name.as_str())
            .collect::<Vec<_>>(),
        vec!["North", "South", "East", "West"]
    );
}

#[test]
fn enum_declaration_with_trailing_comma_parses() {
    let src = "enum E { A, B, } fn main() {}";
    let parsed = parse_src(src);
    assert!(parsed.is_valid(), "{:?}", parsed.parse_errors());
    let (ast, _, _) = parsed.into_parts();
    let e = first_enum(&ast);
    assert_eq!(e.variants.len(), 2);
}

#[test]
fn empty_enum_declaration_parses() {
    let src = "enum E {} fn main() {}";
    let parsed = parse_src(src);
    assert!(parsed.is_valid(), "{:?}", parsed.parse_errors());
    let (ast, _, _) = parsed.into_parts();
    assert!(first_enum(&ast).variants.is_empty());
}

#[test]
fn variant_path_expression_parses() {
    let src = "enum E { A, B } fn main() { let x = E::A; let y = E::B; }";
    let parsed = parse_src(src);
    assert!(parsed.is_valid(), "{:?}", parsed.parse_errors());
    let (ast, _, _) = parsed.into_parts();
    let main = ast
        .items()
        .iter()
        .find_map(|item| match &item.kind {
            ItemKind::Fn(f) if f.name.name == "main" => Some(f),
            _ => None,
        })
        .unwrap();
    let mut variants = Vec::new();
    for stmt in &main.body.stmts {
        if let mink::ast::StmtKind::Let(let_item) = &stmt.kind {
            if let ExprKind::EnumVariant {
                name,
                variant,
                payload: _,
            } = &let_item.init.kind
            {
                variants.push((name.name.clone(), variant.name.clone()));
            }
        }
    }
    assert_eq!(
        variants,
        vec![
            ("E".to_string(), "A".to_string()),
            ("E".to_string(), "B".to_string())
        ]
    );
}

#[test]
fn malformed_enum_declarations_report_structured_errors() {
    // Missing closing brace at end of input: unclosed-brace error.
    let errors = parse_errors("enum E { A, B");
    assert!(
        errors.contains(&ParseErrorKind::UnclosedBrace),
        "{errors:?}"
    );

    // Variant separator missing: expected-comma error, and the parser
    // recovers to the enum's closing brace.
    let errors = parse_errors("enum E { A B } fn main() {}");
    assert!(
        errors.contains(&ParseErrorKind::ExpectedComma),
        "{errors:?}"
    );

    // `E::` with no variant name: expected-variant error.
    let errors = parse_errors("enum E { A } fn main() { let x = E::; }");
    assert!(
        errors.contains(&ParseErrorKind::ExpectedVariant),
        "{errors:?}"
    );
}

#[test]
fn bad_enum_then_valid_item_recovers() {
    let src = "enum E { A B } fn main() { return; }";
    let parsed = parse_src(src);
    let (ast, _, errors) = parsed.into_parts();
    assert!(!errors.is_empty());
    // The recovery must still find the function declaration.
    assert!(
        ast.items()
            .iter()
            .any(|item| matches!(&item.kind, ItemKind::Fn(f) if f.name.name == "main")),
        "parser must recover to the next declaration"
    );
}

// ---------------------------------------------------------------------------
// Semantics: type namespace and duplicates
// ---------------------------------------------------------------------------

#[test]
fn duplicate_enum_is_rejected() {
    let src = "enum E { A } enum E { B } fn main() {}";
    let (_s, _a, semantic, _types) = check_src(src);
    assert_eq!(
        semantic_errors(&semantic, SemanticErrorKind::DuplicateEnum).len(),
        1
    );
}

#[test]
fn enum_and_struct_share_the_type_namespace() {
    // Struct and enum names share one type namespace: a struct after an
    // enum with the same name is a duplicate (reported as the struct's
    // duplicate kind, E-S08), and an enum after a struct with the same
    // name is E-S15.
    let src = "enum E { A } struct E { x: Int } fn main() {}";
    let (_s, _a, semantic, _types) = check_src(src);
    assert_eq!(
        semantic_errors(&semantic, SemanticErrorKind::DuplicateStruct).len(),
        1,
        "struct after same-named enum must be rejected"
    );

    let src = "struct E { x: Int } enum E { A } fn main() {}";
    let (_s, _a, semantic, _types) = check_src(src);
    assert_eq!(
        semantic_errors(&semantic, SemanticErrorKind::DuplicateEnum).len(),
        1,
        "enum after same-named struct must be rejected"
    );
}

#[test]
fn duplicate_variant_is_rejected() {
    let src = "enum E { A, A } fn main() {}";
    let (_s, _a, semantic, _types) = check_src(src);
    assert_eq!(
        semantic_errors(&semantic, SemanticErrorKind::DuplicateVariant).len(),
        1
    );
}

#[test]
fn duplicate_variant_across_enums_is_allowed() {
    let src = "enum E { A } enum F { A } fn main() {}";
    let (_s, _a, semantic, _types) = check_src(src);
    assert!(
        semantic_errors(&semantic, SemanticErrorKind::DuplicateVariant).is_empty(),
        "variants are scoped to their enum"
    );
}

// ---------------------------------------------------------------------------
// Type system: nominal enums, variant typing, equality
// ---------------------------------------------------------------------------

#[test]
fn variant_expression_has_enum_type() {
    let src = "enum E { A, B } fn main() { let x = E::A; }";
    let (_s, _a, semantic, types) = check_src(src);
    assert!(types.errors().is_empty(), "{:?}", types.errors());
    let ty = symbol_type(&types, &semantic, "x");
    assert!(
        matches!(types.types().kind(ty), Some(TypeKind::Enum(_))),
        "x must have the enum type"
    );
}

#[test]
fn variant_equality_requires_same_enum() {
    // Same enum, same variant: fine.
    let src = "enum E { A } fn main() { let b = E::A == E::A; }";
    let (_s, _a, _semantic, types) = check_src(src);
    assert!(types.errors().is_empty(), "{:?}", types.errors());

    // Different enums with the same variant name: type error.
    let src = "enum E { A } enum F { A } fn main() { let b = E::A == F::A; }";
    let (_s, _a, _semantic, types) = check_src(src);
    assert_eq!(type_errors(&types, TypeErrorKind::InvalidOperator).len(), 1);
}

#[test]
fn unknown_variant_is_rejected() {
    let src = "enum E { A } fn main() { let x = E::B; }";
    let (_s, _a, _semantic, types) = check_src(src);
    assert_eq!(type_errors(&types, TypeErrorKind::UnknownVariant).len(), 1);
}

#[test]
fn variant_access_on_non_enum_is_rejected() {
    let src = "struct S { x: Int } fn main() { let y = S::Q; }";
    let (_s, _a, _semantic, types) = check_src(src);
    assert_eq!(type_errors(&types, TypeErrorKind::NotAnEnum).len(), 1);
}

#[test]
fn enum_mismatch_assignment_is_rejected() {
    let src = "enum E { A } enum F { A } fn main() { let mut e = E::A; e = F::A; }";
    let (_s, _a, _semantic, types) = check_src(src);
    assert!(
        !type_errors(&types, TypeErrorKind::TypeMismatch).is_empty(),
        "assigning a different enum must be rejected"
    );
}

#[test]
fn enum_composes_with_structs_and_arrays() {
    let src = "
        enum Color { Red, Green, Blue }
        struct Tag { c: Color, id: Int }
        fn main() {
            let mut t = Tag { c: Color::Green, id: 5 };
            t.c = Color::Red;
            let colors = [Color::Red, Color::Blue];
            let first = colors[0];
        }
    ";
    let (_s, _a, _semantic, types) = check_src(src);
    assert!(types.errors().is_empty(), "{:?}", types.errors());
}

#[test]
fn enum_registration_records_discriminants_in_order() {
    let src = "enum E { A, B, C } fn main() {}";
    let (_s, _a, _semantic, types) = check_src(src);
    let e = types
        .types()
        .enums()
        .iter()
        .find(|i| i.name == "E")
        .unwrap();
    let disc: Vec<i64> = e.variants.iter().map(|v| v.discriminant).collect();
    assert_eq!(disc, vec![0, 1, 2]);
}

// ---------------------------------------------------------------------------
// Layout: enums are single-word discriminants
// ---------------------------------------------------------------------------

#[test]
fn enum_layout_is_a_single_word() {
    let src = "enum E { A, B } fn main() { let x = E::A; }";
    let (_s, _a, semantic, types) = check_src(src);
    let ty = symbol_type(&types, &semantic, "x");
    let (size, align) = scalar_size_align(types.types(), ty).expect("enum is scalar");
    assert_eq!((size, align), (8, 8), "enum values occupy one 8-byte word");
}

// ---------------------------------------------------------------------------
// MIR lowering
// ---------------------------------------------------------------------------

/// Parses, analyzes, type-checks, and lowers through HIR to MIR.
fn lower_mir(src: &str) -> (mink::hir::HirProgram, mir::MirProgram) {
    let mut sources = SourceMap::new();
    let id = sources.add(Path::new("test.mink"), src);
    let file = sources.get(id).unwrap();
    let parsed = parse(file);
    assert!(parsed.is_valid(), "{:?}", parsed.parse_errors());
    let (ast, _, _) = parsed.into_parts();
    let semantic = mink::semantics::analyze(&ast);
    assert!(semantic.errors().is_empty(), "{:?}", semantic.errors());
    let types = mink::typecheck::check(&ast, &semantic, &sources);
    assert!(types.errors().is_empty(), "{:?}", types.errors());
    let hir = mink::hir::lower(&ast, &semantic, &types)
        .unwrap_or_else(|errors| panic!("clean front end must lower: {errors:?}"));
    let mir = mir::lower(&hir).unwrap_or_else(|errors| panic!("clean HIR must lower: {errors:?}"));
    if let Err(errors) = mir::validate(&mir) {
        panic!("lowering a clean program must produce valid MIR: {errors:?}");
    }
    (hir, mir)
}

/// The MIR function named `name`.
fn mir_fn<'p>(mir: &'p MirProgram, name: &str) -> &'p MirFn {
    mir.items
        .iter()
        .find_map(|item| match &item.kind {
            MirItemKind::Fn(f) if f.name.name == name => Some(f),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no MIR function named `{name}`"))
}

/// Whether any constant in `f` is an enum-variant constant.
fn has_enum_constant(f: &MirFn) -> bool {
    f.blocks.iter().any(|block| {
        block.stmts.iter().any(|stmt| {
            let MirStmtKind::Assign { rvalue, .. } = &stmt.kind;
            let MirRvalueKind::Use(operand) = &rvalue.kind else {
                return false;
            };
            matches!(&operand.kind, MirOperandKind::Constant(c) if matches!(c.kind, MirConstantKind::Enum { .. }))
        })
    })
}

#[test]
fn enum_variant_lowers_to_a_constant() {
    let src = "enum E { A, B } fn main() { let x = E::B; }";
    let (_hir, mir) = lower_mir(src);
    let main = mir_fn(&mir, "main");
    assert!(
        has_enum_constant(main),
        "enum variant must lower to an enum constant"
    );
}

// ---------------------------------------------------------------------------
// Backend lowering and verification
// ---------------------------------------------------------------------------

/// Runs the front end on `src` and lowers the optimized MIR into backend
/// instructions, asserting every stage is clean.
fn lower_backend(src: &str) -> (mir::MirProgram, mink::backend::BProgram) {
    let mut sources = SourceMap::new();
    let path = std::env::temp_dir().join(format!("mink_enum_test_{}.mink", std::process::id()));
    std::fs::write(&path, src).unwrap();
    let report = mink::driver::check(&mut sources, &path).unwrap();
    let _ = std::fs::remove_file(&path);
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    let mir = report.mir.as_ref().expect("clean program lowers to MIR");
    let program = backend::lower(mir, &sources)
        .unwrap_or_else(|errors| panic!("clean MIR must lower: {errors:?}"));
    if let Err(errors) = backend::verify(&program) {
        panic!("lowering must produce valid instructions: {errors:?}");
    }
    (mir.clone(), program)
}

#[test]
fn enum_locals_are_word_sized_in_backend() {
    let src = "enum E { A, B } fn main() { let x = E::A; }";
    let (_mir, program) = lower_backend(src);
    let main = program
        .functions
        .iter()
        .find(|f| f.name == "main")
        .expect("main lowered");
    assert!(
        main.locals.iter().any(|l| l.ty == BType::Enum),
        "enum local must lower to BType::Enum"
    );
}

#[test]
fn enum_discriminant_survives_optimization() {
    // The enum discriminant is a compiler-computed value (variant order),
    // not source text, so constant folding must preserve it. `let x = E::B`
    // with `B` the second variant must keep discriminant 1.
    let src = "enum E { A, B } fn main() { let x = E::B; }";
    let (_hir, mir) = lower_mir(src);
    let main = mir_fn(&mir, "main");
    let found = main.blocks.iter().any(|block| {
        block.stmts.iter().any(|stmt| {
            let MirStmtKind::Assign { rvalue, .. } = &stmt.kind;
            let MirRvalueKind::Use(operand) = &rvalue.kind else {
                return false;
            };
            matches!(&operand.kind, MirOperandKind::Constant(c) if matches!(c.kind, MirConstantKind::Enum { variant: 1 }))
        })
    });
    assert!(found, "second variant must carry discriminant 1");
}

// ---------------------------------------------------------------------------
// Native execution
// ---------------------------------------------------------------------------

fn mink() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mink"))
}

fn temp_source(name: &str, content: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("mink_enum_test_{}_{name}", std::process::id()));
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
fn native_enum_equality_and_copy() {
    let exe = build(
        "enum D { A, B }
         fn main() {
             let mut e = D::A;
             e = D::B;
             if e == D::B { rt_print_int(7); } else { rt_print_int(9); }
             let copy = e;
             if copy == D::B { rt_print_int(3); }
             return;
         }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "7\n3");
}

#[test]
fn native_enum_through_functions() {
    let exe = build(
        "enum D { A, B }
         fn id(x) {
             return x;
         }
         fn main() {
             let e = id(D::B);
             if e == D::B { rt_print_int(7); } else { rt_print_int(9); }
             return;
         }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "7");
}

#[test]
fn native_enum_in_struct_and_array() {
    let exe = build(
        "enum Color { Red, Green, Blue }
         struct Tag { c: Color, id: Int }
         fn label(t) {
             if t.c == Color::Red { return 10; }
             if t.c == Color::Green { return 20; }
             return 30;
         }
         fn main() {
             let mut t = Tag { c: Color::Green, id: 5 };
             rt_print_int(label(t));
             let colors = [Color::Red, Color::Blue];
             if colors[1] == Color::Blue { rt_print_int(99); }
             t.c = Color::Red;
             rt_print_int(label(t));
             return;
         }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "20\n99\n10");
}

#[test]
fn native_many_variants_stay_word_sized() {
    // 300 variants: discriminants still fit a single word, so the enum
    // value (and every copy) is one word. This exercises the discriminant
    // assignment past small-cache sizes.
    let variants = (0..300)
        .map(|i| format!("V{i}"))
        .collect::<Vec<_>>()
        .join(", ");
    let src = format!(
        "enum Big {{ {variants} }}
         fn main() {{
             let mut e = Big::V0;
             e = Big::V299;
             if e == Big::V299 {{ rt_print_int(1); }} else {{ rt_print_int(9); }}
             return;
         }}"
    );
    let exe = build(&src);
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "1");
}
