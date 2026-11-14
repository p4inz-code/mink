//! Compiler pipeline orchestration.
//!
//! Owns the sequence Source → Lexer → Parser → AST → Semantic Analysis →
//! Type Analysis → HIR → MIR → Optimization → Backend (see
//! `docs/compiler/COMPILER_ARCHITECTURE.md` §2). The driver runs source
//! loading plus lexical, syntactic, semantic, type, and HIR analysis, and
//! lowers to MIR when the front end is clean.

use std::collections::HashSet;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use crate::backend::{self, BackendError, Target};
use crate::hir::{self, HirProgram};
use crate::lexer::LexError;
use crate::mir::{self, MirProgram};
use crate::parser::{self, ParseError};
use crate::semantics::{self, SemanticError, SemanticResult};
use crate::source::{SourceId, SourceMap, Span};
use crate::typecheck::{self, TypeError, TypeResult};

/// Options controlling a [`build`] run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuildOptions {
    /// The target to compile for.
    pub target: Target,
}

impl Default for BuildOptions {
    fn default() -> Self {
        Self {
            target: Target::native(),
        }
    }
}

/// The outcome of a successful [`build`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildOutcome {
    /// The source id of the root module.
    pub source_id: SourceId,
    /// The path to the generated executable.
    pub output: PathBuf,
    /// The target the binary was compiled for.
    pub target: Target,
    /// Number of user functions in the program.
    pub functions: usize,
    /// Number of static bindings in the program.
    pub statics: usize,
}

/// Errors produced while running the build pipeline.
#[derive(Debug)]
pub enum BuildError {
    /// A source file could not be read.
    Io {
        /// The path that failed.
        path: PathBuf,
        /// The underlying I/O error.
        source: io::Error,
    },
    /// The path exists but is a directory, not a MINK source file.
    NotAFile {
        /// The path that was expected to be a source file.
        path: PathBuf,
    },
    /// One or more front-end errors (lex / parse / semantic / type / HIR / MIR).
    FrontEnd(Box<CheckReport>),
    /// One or more back-end errors (unsupported constructs, verification).
    Backend(Box<[BackendError]>),
    /// The executable could not be written.
    Output {
        /// The path that failed.
        path: PathBuf,
        /// The underlying I/O error.
        source: io::Error,
    },
}

impl fmt::Display for BuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source: _ } => {
                write!(f, "failed to read '{}'", path.display())
            }
            Self::NotAFile { path } => {
                write!(
                    f,
                    "'{}' is a directory, not a MINK source file",
                    path.display()
                )
            }
            Self::FrontEnd(report) => {
                write!(f, "{} front-end error(s)", report.errors.len())
            }
            Self::Backend(errors) => {
                write!(f, "{} backend error(s)", errors.len())
            }
            Self::Output { path, source: _ } => {
                write!(f, "failed to write '{}'", path.display())
            }
        }
    }
}

impl std::error::Error for BuildError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } | Self::Output { source, .. } => Some(source),
            Self::NotAFile { .. } | Self::FrontEnd(_) | Self::Backend(_) => None,
        }
    }
}

/// A single problem found by `check`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckError {
    /// A lexical error.
    Lex(LexError),
    /// A syntax/parse error.
    Parse(ParseError),
    /// A semantic error.
    Semantic(SemanticError),
    /// A type error.
    Type(TypeError),
    /// A HIR lowering error.
    Hir(hir::HirError),
    /// A MIR lowering error.
    Mir(mir::MirError),
}

impl CheckError {
    /// The diagnostic code (e.g. `"E-L01"`, `"E-T05"`).
    pub fn code(&self) -> &'static str {
        match self {
            Self::Lex(error) => error.kind().code(),
            Self::Parse(error) => error.kind().code(),
            Self::Semantic(error) => error.kind().code(),
            Self::Type(error) => error.kind().code(),
            Self::Hir(error) => error.kind().code(),
            Self::Mir(error) => error.kind().code(),
        }
    }

    /// The primary span of the error.
    pub fn span(&self) -> Span {
        match self {
            Self::Lex(error) => error.span(),
            Self::Parse(error) => error.span(),
            Self::Semantic(error) => error.span(),
            Self::Type(error) => error.span(),
            Self::Hir(error) => error.span(),
            Self::Mir(error) => error.span(),
        }
    }

    /// An optional related span (the original declaration for duplicates,
    /// the expected type for mismatches, etc.).
    pub fn related_span(&self) -> Option<Span> {
        match self {
            Self::Semantic(error) => error.original(),
            Self::Type(error) => error.related(),
            Self::Lex(_) | Self::Parse(_) | Self::Hir(_) | Self::Mir(_) => None,
        }
    }
}

impl fmt::Display for CheckError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Lex(error) => error.fmt(f),
            Self::Parse(error) => error.fmt(f),
            Self::Semantic(error) => error.fmt(f),
            Self::Type(error) => error.fmt(f),
            Self::Hir(error) => error.fmt(f),
            Self::Mir(error) => error.fmt(f),
        }
    }
}

/// The result of running the full pipeline on one source file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckReport {
    /// The root source file id.
    pub source_id: SourceId,
    /// Total token count across all processed source files.
    pub token_count: usize,
    /// All diagnostic errors, in source order.
    pub errors: Vec<CheckError>,
    /// The semantic analysis result, if the front end got that far.
    pub semantic: Option<SemanticResult>,
    /// The type analysis result, if the front end got that far.
    pub types: Option<TypeResult>,
    /// The ownership analysis result, if the front end got that far.
    pub ownership: Option<crate::ownership::OwnershipResult>,
    /// The HIR program, if the front end got that far.
    pub hir: Option<HirProgram>,
    /// The optimized MIR program, if the front end got that far.
    pub mir: Option<MirProgram>,
}

// ======================================================================
// Public entry points
// ======================================================================

/// Loads `path` and runs the full pipeline. When the source contains `mod`
/// declarations, all reachable modules are loaded and compiled together.
pub fn check(sources: &mut SourceMap, path: &Path) -> Result<CheckReport, BuildError> {
    if path.is_dir() {
        return Err(BuildError::NotAFile {
            path: path.to_path_buf(),
        });
    }
    // Discover all modules reachable from the root file.
    let modules = discover_modules(sources, path).map_err(|errors| match errors {
        // Root file I/O error: report as BuildError::Io.
        None => BuildError::Io {
            path: path.to_path_buf(),
            source: std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("file '{}' not found", path.display()),
            ),
        },
        // Child-module errors: report as FrontEnd.
        Some(errors) => {
            let report = CheckReport {
                source_id: SourceId::new(0),
                token_count: 0,
                errors,
                semantic: None,
                types: None,
                ownership: None,
                hir: None,
                mir: None,
            };
            BuildError::FrontEnd(Box::new(report))
        }
    })?;
    // Single-module fast path (no mod declarations discovered).
    if modules.len() == 1 {
        return check_single_module(sources, path);
    }
    // Multi-module pipeline: flatten all modules into a single AST.
    check_multi_module(sources, &modules)
}

/// Runs the full compiler pipeline for a single MINK source file and writes
/// a native executable.
pub fn build(
    sources: &mut SourceMap,
    path: &Path,
    options: BuildOptions,
) -> Result<BuildOutcome, BuildError> {
    let report = check(sources, path)?;
    if !report.errors.is_empty() {
        return Err(BuildError::FrontEnd(Box::new(report)));
    }
    let mir = report
        .mir
        .expect("a clean front end always lowers, validates, and optimizes to MIR");
    let image = backend::compile(&mir, sources, options.target)
        .map_err(|errors| BuildError::Backend(errors.into_boxed_slice()))?;
    let output = executable_path(path);
    std::fs::write(&output, &image.bytes).map_err(|source| BuildError::Output {
        path: output.clone(),
        source,
    })?;
    Ok(BuildOutcome {
        source_id: report.source_id,
        output,
        target: options.target,
        functions: image.functions,
        statics: image.statics,
    })
}

// ======================================================================
// Single-module pipeline
// ======================================================================

fn check_single_module(sources: &mut SourceMap, path: &Path) -> Result<CheckReport, BuildError> {
    let source_id = sources.load(path).map_err(|source| BuildError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let file = sources
        .get(source_id)
        .expect("the file id returned by load is always registered");
    let parsed = parser::parse(file);
    let mut errors: Vec<CheckError> = parsed
        .lex_errors()
        .iter()
        .copied()
        .map(CheckError::Lex)
        .collect();
    errors.extend(parsed.parse_errors().iter().copied().map(CheckError::Parse));
    let (semantic, types, ownership, hir, mir) = if parsed.is_valid() {
        // Run monomorphization: create concrete copies of generic functions.
        let mut mono_ast = parsed.ast().clone();
        let uninferable = crate::monomorphize::monomorphize_reported(&mut mono_ast);
        let semantic = semantics::analyze(&mono_ast);
        let types = typecheck::check(&mono_ast, &semantic, sources);
        errors.extend(
            semantic
                .errors()
                .iter()
                .filter(|e| !explained_by_inference(e, &uninferable))
                .cloned()
                .map(CheckError::Semantic),
        );
        errors.extend(uninferable.iter().map(|(name, span)| {
            CheckError::Type(crate::typecheck::TypeError::cannot_infer_type_arguments(
                *span, name,
            ))
        }));
        errors.extend(types.errors().iter().cloned().map(CheckError::Type));
        let ownership = if errors.is_empty() {
            let result = crate::ownership::check(&mono_ast, &semantic, &types);
            errors.extend(result.errors().iter().cloned().map(CheckError::Semantic));
            Some(result)
        } else {
            None
        };
        let (hir, mir) = if errors.is_empty() {
            match hir::lower(&mono_ast, &semantic, &types) {
                Ok(program) => {
                    let mir = match mir::lower(&program) {
                        Ok(mir_program) => match mir::optimize(&mir_program) {
                            Ok(optimized) => Some(optimized),
                            Err(optimization_errors) => {
                                errors.extend(optimization_errors.into_iter().map(CheckError::Mir));
                                None
                            }
                        },
                        Err(lowering_errors) => {
                            errors.extend(lowering_errors.into_iter().map(CheckError::Mir));
                            None
                        }
                    };
                    (Some(program), mir)
                }
                Err(lowering_errors) => {
                    errors.extend(lowering_errors.into_iter().map(CheckError::Hir));
                    (None, None)
                }
            }
        } else {
            (None, None)
        };
        (Some(semantic), Some(types), ownership, hir, mir)
    } else {
        (None, None, None, None, None)
    };
    sort_errors(&mut errors);
    Ok(CheckReport {
        source_id,
        token_count: parsed.token_count(),
        errors,
        semantic,
        types,
        ownership,
        hir,
        mir,
    })
}

// ======================================================================
// Multi-module pipeline (Session 34)
//
// Flattens all discovered modules into a single AST and runs the standard
// single-module pipeline. Child module items are injected into the root
// module's item list, `mod`/`use` declarations are stripped, and the
// combined AST is analyzed as one compilation unit. This avoids the
// complexity of cross-module SymbolId and TypeId merging.
// ======================================================================

fn check_multi_module(
    sources: &mut SourceMap,
    modules: &[ModuleSource],
) -> Result<CheckReport, BuildError> {
    let root_source_id = modules[0].source_id;
    let mut all_errors: Vec<CheckError> = Vec::new();
    let mut total_tokens = 0;

    // Build a map: module_name → its parsed AST items (excluding mod/use).
    let mut child_items: std::collections::HashMap<String, Vec<crate::ast::Item>> =
        std::collections::HashMap::new();
    let mut root_items: Vec<crate::ast::Item> = Vec::new();
    let mut root_valid = false;

    for module in modules {
        if !module.parsed.is_valid() {
            // Collect parse errors from non-root modules.
            if module.source_id != root_source_id {
                for e in module.parsed.lex_errors() {
                    all_errors.push(CheckError::Lex(*e));
                }
                for e in module.parsed.parse_errors() {
                    all_errors.push(CheckError::Parse(*e));
                }
            }
            continue;
        }
        total_tokens += module.parsed.token_count();

        if module.source_id == root_source_id {
            root_valid = true;
            // Collect the root module's items, stripping mod/use (including
            // the `pub`-qualified forms, which declare nothing themselves).
            for item in module.parsed.ast().items() {
                if let Some(item) = flattened_item(item) {
                    root_items.push(item);
                }
            }
        } else {
            // Child modules: every remaining item is included. `pub` is
            // recorded but not enforced (CORE_GRAMMAR.md §30), so private
            // items are included too — public items may call them.
            let mut items = Vec::new();
            for item in module.parsed.ast().items() {
                if let Some(item) = flattened_item(item) {
                    items.push(item);
                }
            }
            child_items.insert(module.name.clone(), items);
        }
    }

    // Validate `use` declarations against the modules this compilation
    // loaded. Flattening makes every item reachable by name, which is why a
    // misspelled import used to go unreported until (and unless) the
    // imported name was used.
    all_errors.extend(validate_imports(modules));

    if !root_valid {
        // Root module had no valid content. Return whatever errors we have.
        sort_errors(&mut all_errors);
        return Ok(CheckReport {
            source_id: root_source_id,
            token_count: total_tokens,
            errors: all_errors,
            semantic: None,
            types: None,
            ownership: None,
            hir: None,
            mir: None,
        });
    }

    // Append child module items to the root module's items.
    // Process children in a deterministic order (sorted by name).
    let mut child_names: Vec<String> = child_items.keys().cloned().collect();
    child_names.sort();
    for name in &child_names {
        if let Some(items) = child_items.remove(name) {
            root_items.extend(items);
        }
    }

    // Build a combined AST and run the single-module pipeline.
    let mut combined_ast = crate::ast::Ast::new(root_items);

    // Run monomorphization: create concrete copies of generic functions.
    let uninferable = crate::monomorphize::monomorphize_reported(&mut combined_ast);

    // Run semantic analysis.
    let semantic = semantics::analyze(&combined_ast);
    all_errors.extend(
        semantic
            .errors()
            .iter()
            .filter(|e| !explained_by_inference(e, &uninferable))
            .cloned()
            .map(CheckError::Semantic),
    );
    all_errors.extend(uninferable.iter().map(|(name, span)| {
        CheckError::Type(crate::typecheck::TypeError::cannot_infer_type_arguments(
            *span, name,
        ))
    }));

    // Run type checking.
    let types = typecheck::check(&combined_ast, &semantic, sources);
    all_errors.extend(types.errors().iter().cloned().map(CheckError::Type));

    // Run ownership analysis if clean so far.
    let ownership = if all_errors.is_empty() {
        let result = crate::ownership::check(&combined_ast, &semantic, &types);
        all_errors.extend(result.errors().iter().cloned().map(CheckError::Semantic));
        Some(result)
    } else {
        None
    };

    // HIR + MIR lowering.
    let (hir, mir) = if all_errors.is_empty() {
        match hir::lower(&combined_ast, &semantic, &types) {
            Ok(hir_program) => match mir::lower(&hir_program) {
                Ok(mir_program) => match mir::optimize(&mir_program) {
                    Ok(optimized) => (Some(hir_program), Some(optimized)),
                    Err(errs) => {
                        all_errors.extend(errs.into_iter().map(CheckError::Mir));
                        (Some(hir_program), None)
                    }
                },
                Err(errs) => {
                    all_errors.extend(errs.into_iter().map(CheckError::Mir));
                    (Some(hir_program), None)
                }
            },
            Err(errs) => {
                all_errors.extend(errs.into_iter().map(CheckError::Hir));
                (None, None)
            }
        }
    } else {
        (None, None)
    };

    sort_errors(&mut all_errors);
    Ok(CheckReport {
        source_id: root_source_id,
        token_count: total_tokens,
        errors: all_errors,
        semantic: Some(semantic),
        types: Some(types),
        ownership,
        hir,
        mir,
    })
}

/// Orders diagnostics by source file, then by byte offset, so output is
/// deterministic regardless of which pipeline stage produced an error.
///
/// Spans carry their own file id, and a compilation can cover several files;
/// sorting by offset alone would interleave the files' diagnostics depending
/// on the order the stages ran in.
fn sort_errors(errors: &mut [CheckError]) {
    errors.sort_by_key(|error| {
        let span = error.span();
        (span.file().raw(), span.start(), error.code())
    });
}

/// Validates every `use` declaration against the modules loaded for this
/// compilation.
///
/// `use module::item;` must name a module loaded by a `mod` declaration and
/// an item that module declares. Nothing else is checked: items share one
/// flattened namespace and `pub` is not enforced (see `CORE_GRAMMAR.md`
/// §30), so importing a non-public item stays valid.
fn validate_imports(modules: &[ModuleSource]) -> Vec<CheckError> {
    use std::collections::{HashMap, HashSet};

    // Module name → the names it declares at module scope.
    let mut declared: HashMap<&str, HashSet<&str>> = HashMap::new();
    for module in modules {
        if !module.parsed.is_valid() {
            continue;
        }
        let names = declared.entry(module.name.as_str()).or_default();
        for item in module.parsed.ast().items() {
            if let Some(name) = module_level_name(&item.kind) {
                names.insert(name);
            }
        }
    }

    let mut errors = Vec::new();
    for module in modules {
        if !module.parsed.is_valid() {
            continue;
        }
        for item in module.parsed.ast().items() {
            let crate::ast::ItemKind::Use(use_decl) = item_kind_through_pub(&item.kind) else {
                continue;
            };
            // `pub use module::item;` is validated like a plain import: MINK
            // has no re-exports (CORE_GRAMMAR.md §30), and the qualifier is
            // ignored, so an unresolvable path is still a mistake.
            // `use module;` is a documented no-op.
            if use_decl.path.len() < 2 {
                continue;
            }
            if use_decl.path.len() > 2 {
                errors.push(CheckError::Semantic(SemanticError::module_not_found(
                    format!(
                        "`use` takes a module and one item (`use module::item;`), but this path has {} segments",
                        use_decl.path.len()
                    ),
                    use_decl.span,
                )));
                continue;
            }
            let module_name = use_decl.path[0].name.as_str();
            let item_name = use_decl.path[1].name.as_str();
            // Importing from the declaring module itself is a no-op: the
            // item is already in scope.
            if module_name == module.name {
                continue;
            }
            let Some(items) = declared.get(module_name) else {
                errors.push(CheckError::Semantic(SemanticError::module_not_found(
                    format!(
                        "cannot find module `{module_name}`; declare it with `mod {module_name};`"
                    ),
                    use_decl.path[0].span,
                )));
                continue;
            };
            if !items.contains(item_name) {
                errors.push(CheckError::Semantic(SemanticError::module_not_found(
                    format!("cannot find `{item_name}` in module `{module_name}`"),
                    use_decl.path[1].span,
                )));
            }
        }
    }
    errors
}

/// Whether a semantic error is already explained by a failed generic-call
/// inference.
///
/// Monomorphization drops generic declarations from the AST, so an
/// uninstantiated call would also surface as "cannot find name". Reporting
/// both would tell the user two contradictory things about one call, so the
/// unresolved-name error at that exact callee is suppressed and the `E-T44`
/// (which names the real fix) is the only diagnostic.
fn explained_by_inference(
    error: &crate::semantics::SemanticError,
    uninferable: &[(String, crate::source::Span)],
) -> bool {
    if error.kind() != crate::semantics::SemanticErrorKind::UnresolvedName {
        return false;
    }
    uninferable
        .iter()
        .any(|(name, span)| name == error.name() && *span == error.span())
}

/// The kind of an item, with a `pub` qualifier stripped.
///
/// `pub` neither declares nor loads anything on its own; the pipeline treats
/// `pub fn`/`pub struct`/... as the declaration it wraps, and `pub mod`/
/// `pub use` as the module or import it wraps.
fn item_kind_through_pub(kind: &crate::ast::ItemKind) -> &crate::ast::ItemKind {
    match kind {
        crate::ast::ItemKind::Pub(pub_item) => item_kind_through_pub(&pub_item.item.kind),
        other => other,
    }
}

/// The item a module contributes to the flattened compilation unit.
///
/// Returns `None` for `mod`/`use` declarations (and their `pub` forms): they
/// have been acted on during discovery, and the flattened unit sees a single
/// namespace rather than per-module ones. Everything else is returned with
/// any `pub` qualifier stripped, matching how the pipeline treats visibility.
fn flattened_item(item: &crate::ast::Item) -> Option<crate::ast::Item> {
    match item_kind_through_pub(&item.kind) {
        crate::ast::ItemKind::Module(_) | crate::ast::ItemKind::Use(_) => None,
        _ => match &item.kind {
            crate::ast::ItemKind::Pub(pub_item) => Some(pub_item.item.as_ref().clone()),
            _ => Some(item.clone()),
        },
    }
}

/// The name a module-level item is declared under, if it declares one.
fn module_level_name(kind: &crate::ast::ItemKind) -> Option<&str> {
    match kind {
        crate::ast::ItemKind::Fn(item) => Some(item.name.name.as_str()),
        crate::ast::ItemKind::Struct(item) => Some(item.name.name.as_str()),
        crate::ast::ItemKind::Enum(item) => Some(item.name.name.as_str()),
        crate::ast::ItemKind::Let(item) => Some(item.name.name.as_str()),
        crate::ast::ItemKind::Const(item) => Some(item.name.name.as_str()),
        crate::ast::ItemKind::Pub(item) => module_level_name(&item.item.kind),
        crate::ast::ItemKind::Module(_) | crate::ast::ItemKind::Use(_) => None,
    }
}

// ======================================================================
// Module discovery (Session 34)
// ======================================================================

/// A discovered source module: its AST, file path, and source id.
#[allow(dead_code)]
pub(crate) struct ModuleSource {
    /// The module's name (file stem).
    pub name: String,
    /// The file path on disk.
    pub path: PathBuf,
    /// The source id assigned to this file.
    pub source_id: SourceId,
    /// The parsed output for this module.
    pub parsed: parser::ParseOutput,
    /// The parent module name, if any (None for root).
    pub parent: Option<String>,
}

/// Discovers all modules reachable from `root_path` by following `mod`
/// declarations recursively.
///
/// Returns either the module list, a list of semantic/syntax errors from
/// child modules, or `Err(None)` when the root file itself cannot be
/// loaded (I/O error — the caller should report it as `BuildError::Io`).
pub(crate) fn discover_modules(
    sources: &mut SourceMap,
    root_path: &Path,
) -> Result<Vec<ModuleSource>, Option<Vec<CheckError>>> {
    // Try loading the root file first. If it fails, return None so the
    // caller can report the proper I/O error.
    if !root_path.exists() {
        return Err(None);
    }
    let mut modules = Vec::new();
    let mut visited = HashSet::new();
    let mut errors = Vec::new();
    // Installed packages and path dependencies are additional module roots
    // (the site-packages search path). A file outside any project simply has
    // none, so single-file builds are unaffected.
    let external = crate::package::module_roots(root_path);
    discover_modules_recursive(
        sources,
        root_path,
        None,
        None,
        &external,
        &mut modules,
        &mut visited,
        &mut errors,
    );
    if errors.is_empty() {
        Ok(modules)
    } else {
        errors.sort_by_key(|e| e.span().start());
        Err(Some(errors))
    }
}

#[allow(clippy::too_many_arguments)]
fn discover_modules_recursive(
    sources: &mut SourceMap,
    path: &Path,
    parent: Option<String>,
    declared: Option<(String, Span)>,
    external_roots: &[PathBuf],
    modules: &mut Vec<ModuleSource>,
    visited: &mut HashSet<PathBuf>,
    errors: &mut Vec<CheckError>,
) {
    // The `mod name;` declaration that requested this file, when there is
    // one. A missing or unreadable module file is reported at that
    // declaration, which is the only location the source actually contains;
    // the synthetic empty span is the fallback when nothing points at it.
    let declared_span = declared
        .as_ref()
        .map(|(_, span)| *span)
        .unwrap_or_else(|| Span::new(SourceId::new(0), 0..0));
    // Canonicalize to detect cycles and missing files.
    let canon = match path.canonicalize() {
        Ok(p) => p,
        Err(_) => {
            errors.push(CheckError::Semantic(SemanticError::module_not_found(
                format!("module file '{}' not found", path.display()),
                declared_span,
            )));
            return;
        }
    };
    if !visited.insert(canon) {
        return;
    }
    let source_id = match sources.load(path) {
        Ok(id) => id,
        Err(_) => {
            errors.push(CheckError::Semantic(SemanticError::module_not_found(
                format!("module file '{}' not found", path.display()),
                declared_span,
            )));
            return;
        }
    };
    let file = sources
        .get(source_id)
        .expect("loaded file always registered");
    // Child modules take the name from their `mod` declaration, so a
    // directory package resolved to `<name>/mod.mink` is still known as
    // `<name>` (matching `use <name>::item`). The root uses its file stem.
    let name = declared.map(|(name, _)| name).unwrap_or_else(|| {
        path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string()
    });
    let parsed = parser::parse(file);

    // Only collect lex/parse errors from child modules; the root file's
    // errors are handled by check_single_module or check_multi_module.
    if parent.is_some() {
        for e in parsed.lex_errors() {
            errors.push(CheckError::Lex(*e));
        }
        for e in parsed.parse_errors() {
            errors.push(CheckError::Parse(*e));
        }
    }

    // Collect mod declarations before moving `parsed` into ModuleSource.
    let mod_names: Vec<(String, Span)> = if parsed.is_valid() {
        parsed
            .ast()
            .items()
            .iter()
            .filter_map(|item| match item_kind_through_pub(&item.kind) {
                crate::ast::ItemKind::Module(mod_decl) => {
                    Some((mod_decl.name.name.clone(), mod_decl.name.span))
                }
                _ => None,
            })
            .collect()
    } else {
        Vec::new()
    };

    let module_name = name.clone();
    modules.push(ModuleSource {
        name,
        path: path.to_path_buf(),
        source_id,
        parsed,
        parent,
    });

    // Recurse into child modules.
    if !mod_names.is_empty() {
        let parent_dir = path.parent().unwrap_or_else(|| Path::new("."));
        for (child_name, decl_span) in mod_names {
            let child_path = resolve_module_path(&child_name, parent_dir, external_roots);
            discover_modules_recursive(
                sources,
                &child_path,
                Some(module_name.clone()),
                Some((child_name, decl_span)),
                external_roots,
                modules,
                visited,
                errors,
            );
        }
    }
}

/// Resolves a `mod name;` declaration to a `.mink` file.
///
/// Search order:
/// 1. the declaring file's directory (classic sibling-module behavior);
/// 2. the declaring file's directory as a *directory package*: the package
///    root file is `<dir>/<name>/mod.mink` (MINK's equivalent of Python's
///    `<name>/__init__.py`); submodules declared by that root resolve
///    relative to the package directory (`<dir>/<name>/<sub>.mink`);
/// 3. the bundled standard library next to the installed `mink`
///    executable (`<exe>/../stdlib`, the npm package layout);
/// 4. a `stdlib` directory in the current working directory (dev
///    checkout layout: `target/debug/mink.exe` run from the repo root);
/// 5. every project package root: the active environment's packages
///    directory (or the project's own `.mink/packages`) and each path
///    dependency's directory.
///
/// The first candidate that exists wins; when none exist the returned
/// path is the sibling path so the existing "module file not found"
/// error still names the most intuitive location.
fn resolve_module_path(child_name: &str, parent_dir: &Path, external_roots: &[PathBuf]) -> PathBuf {
    let sibling = parent_dir.join(format!("{child_name}.mink"));
    if sibling.exists() {
        return sibling;
    }
    let sibling_package = parent_dir.join(child_name).join("mod.mink");
    if sibling_package.exists() {
        return sibling_package;
    }
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            roots.push(exe_dir.join("..").join("stdlib"));
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        roots.push(cwd.join("stdlib"));
    }
    for root in roots.into_iter().chain(external_roots.iter().cloned()) {
        let candidate = root.join(format!("{child_name}.mink"));
        if candidate.exists() {
            return candidate;
        }
        // A directory package: `<dir>/<name>/mod.mink` (the MINK equivalent
        // of Python's `<name>/__init__.py`).
        let package = root.join(child_name).join("mod.mink");
        if package.exists() {
            return package;
        }
        // An installed package: `<packages>/<name>/<name>.mink`. `mink
        // install` copies a package into `<packages>/<name>/`, keeping the
        // package's own entry file beside its manifest, so the entry module
        // is the file named after the package.
        let installed = root.join(child_name).join(format!("{child_name}.mink"));
        if installed.exists() {
            return installed;
        }
    }
    sibling
}

// ======================================================================
// Helpers
// ======================================================================

fn executable_path(path: &Path) -> PathBuf {
    let exe = if cfg!(target_os = "windows") {
        path.with_extension("exe")
    } else {
        path.with_extension("")
    };
    // On Windows, std::process::Command does not search the current
    // directory for relative paths.  Resolve to an absolute path so
    // `mink run hello.mink` works without requiring `./hello.mink`.
    if exe.is_relative() {
        std::env::current_dir()
            .map(|cwd| cwd.join(&exe))
            .unwrap_or(exe)
    } else {
        exe
    }
}
