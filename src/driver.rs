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
            Self::Io { path, source } => {
                write!(f, "failed to read '{}': {source}", path.display())
            }
            Self::FrontEnd(report) => {
                write!(f, "{} front-end error(s)", report.errors.len())
            }
            Self::Backend(errors) => {
                write!(f, "{} backend error(s)", errors.len())
            }
            Self::Output { path, source } => {
                write!(f, "failed to write '{}': {source}", path.display())
            }
        }
    }
}

impl std::error::Error for BuildError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } | Self::Output { source, .. } => Some(source),
            Self::FrontEnd(_) | Self::Backend(_) => None,
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
    // Discover all modules reachable from the root file.
    let modules = discover_modules(sources, path).map_err(|errors| match errors {
        // Root file I/O error: report as BuildError::Io.
        None => BuildError::Io {
            path: path.to_path_buf(),
            source: std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("failed to read '{}'", path.display()),
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
        let semantic = semantics::analyze(parsed.ast());
        let types = typecheck::check(parsed.ast(), &semantic, sources);
        errors.extend(semantic.errors().iter().cloned().map(CheckError::Semantic));
        errors.extend(types.errors().iter().cloned().map(CheckError::Type));
        let ownership = if errors.is_empty() {
            let result = crate::ownership::check(parsed.ast(), &semantic, &types);
            errors.extend(result.errors().iter().cloned().map(CheckError::Semantic));
            Some(result)
        } else {
            None
        };
        let (hir, mir) = if errors.is_empty() {
            match hir::lower(parsed.ast(), &semantic, &types) {
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
    errors.sort_by_key(|error| error.span().start());
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
            // Collect the root module's items, stripping mod/use.
            for item in module.parsed.ast().items() {
                match &item.kind {
                    crate::ast::ItemKind::Module(_) | crate::ast::ItemKind::Use(_) => {}
                    _ => root_items.push(item.clone()),
                }
            }
        } else {
            // Child modules: collect public items and add them to the
            // combined root. Private items are excluded because they
            // should not be visible from the root module.
            let mut items = Vec::new();
            for item in module.parsed.ast().items() {
                match &item.kind {
                    crate::ast::ItemKind::Module(_) | crate::ast::ItemKind::Use(_) => {}
                    crate::ast::ItemKind::Pub(pub_item) => {
                        // Public items are always included.
                        items.push(pub_item.item.as_ref().clone());
                    }
                    _ => {
                        // Non-public items are included too (they may be
                        // needed by public items that reference them, e.g.
                        // private helper functions called by public ones).
                        // For V1, all items are included from all modules.
                        items.push(item.clone());
                    }
                }
            }
            child_items.insert(module.name.clone(), items);
        }
    }

    if !root_valid {
        // Root module had no valid content. Return whatever errors we have.
        all_errors.sort_by_key(|e| e.span().start());
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
    let combined_ast = crate::ast::Ast::new(root_items);

    // Run semantic analysis.
    let semantic = semantics::analyze(&combined_ast);
    all_errors.extend(semantic.errors().iter().cloned().map(CheckError::Semantic));

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

    all_errors.sort_by_key(|e| e.span().start());
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
    discover_modules_recursive(
        sources,
        root_path,
        None,
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

fn discover_modules_recursive(
    sources: &mut SourceMap,
    path: &Path,
    parent: Option<String>,
    modules: &mut Vec<ModuleSource>,
    visited: &mut HashSet<PathBuf>,
    errors: &mut Vec<CheckError>,
) {
    // Canonicalize to detect cycles and missing files.
    let canon = match path.canonicalize() {
        Ok(p) => p,
        Err(_) => {
            errors.push(CheckError::Semantic(SemanticError::unresolved(
                format!("module file '{}' not found", path.display()),
                Span::new(SourceId::new(0), 0..0),
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
            errors.push(CheckError::Semantic(SemanticError::unresolved(
                format!("module file '{}' not found", path.display()),
                Span::new(SourceId::new(0), 0..0),
            )));
            return;
        }
    };
    let file = sources
        .get(source_id)
        .expect("loaded file always registered");
    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();
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
    let mod_names: Vec<String> = if parsed.is_valid() {
        parsed
            .ast()
            .items()
            .iter()
            .filter_map(|item| {
                if let crate::ast::ItemKind::Module(mod_decl) = &item.kind {
                    Some(mod_decl.name.name.clone())
                } else {
                    None
                }
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
        for child_name in mod_names {
            let child_path = parent_dir.join(format!("{child_name}.mink"));
            discover_modules_recursive(
                sources,
                &child_path,
                Some(module_name.clone()),
                modules,
                visited,
                errors,
            );
        }
    }
}

// ======================================================================
// Helpers
// ======================================================================

fn executable_path(path: &Path) -> PathBuf {
    path.with_extension("exe")
}
