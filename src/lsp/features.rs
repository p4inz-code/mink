//! What the language server answers: completion, hover, definition, references, rename,
//! outline, signature help and formatting. Every function is pure: it reads the analysed
//! files and returns the JSON the protocol wants.

use super::analysis::{Analysis, AnalyzedFile};
use super::position::Lines;
use super::scan::{FileIndex, SymKind, Symbol, KEYWORDS};
use crate::installer::json::Value;
use crate::lexer::TokenKind;
use crate::runtime::intrinsics;

// LSP numbering.
const KIND_METHOD_FUNCTION: u32 = 3;
const KIND_FIELD: u32 = 5;
const KIND_VARIABLE: u32 = 6;
const KIND_MODULE: u32 = 9;
const KIND_ENUM: u32 = 13;
const KIND_KEYWORD: u32 = 14;
const KIND_ENUM_MEMBER: u32 = 20;
const KIND_CONSTANT: u32 = 21;
const KIND_STRUCT: u32 = 22;

fn range_value(lines: &Lines<'_>, start: usize, end: usize) -> Value {
    let (a, b) = (lines.position(start), lines.position(end));
    Value::object(vec![
        (
            "start",
            Value::object(vec![("line", Value::Number(a.line as f64)), ("character", Value::Number(a.character as f64))]),
        ),
        (
            "end",
            Value::object(vec![("line", Value::Number(b.line as f64)), ("character", Value::Number(b.character as f64))]),
        ),
    ])
}

fn location(file: &AnalyzedFile, start: usize, end: usize) -> Value {
    let lines = Lines::new(&file.text);
    Value::object(vec![("uri", Value::text(&file.uri)), ("range", range_value(&lines, start, end))])
}

fn markdown(text: String) -> Value {
    Value::object(vec![("kind", Value::text("markdown")), ("value", Value::Text(text))])
}

// --------------------------------------------------------------- resolving names

/// What a name in the source stands for.
enum Target<'a> {
    /// A parameter or `let` in a function: its symbol and the byte range where it can be seen.
    Local { symbol: Symbol, scope: (usize, usize) },
    /// A declaration at the top of a file.
    Global { file: &'a AnalyzedFile, symbol: &'a Symbol },
    /// A runtime function the compiler provides.
    Builtin(&'static intrinsics::Intrinsic),
}

fn builtin(name: &str) -> Option<&'static intrinsics::Intrinsic> {
    intrinsics::by_name(name)
}

fn builtin_signature(i: &intrinsics::Intrinsic) -> String {
    let params: Vec<String> = i.params.iter().map(|p| format!("{p:?}")).collect();
    format!("fn {}({}) -> {:?}", i.name, params.join(", "), i.result)
}

/// The name before `::` when token `i` is the member of a path like `math::abs` or `Color::Red`.
fn qualifier(file: &AnalyzedFile, i: usize) -> Option<String> {
    let tokens = &file.index.tokens;
    if i >= 2 && tokens[i - 1].kind == TokenKind::ColonColon && tokens[i - 2].kind == TokenKind::Ident {
        return Some(file.index.text(&file.text, i - 2).to_string());
    }
    None
}

fn file_for_module<'a>(analysis: &'a Analysis, name: &str) -> Option<&'a AnalyzedFile> {
    analysis.files.iter().find(|f| {
        f.path.file_stem().is_some_and(|s| s.to_string_lossy() == name)
            || f.path
                .parent()
                .and_then(|p| p.file_name())
                .is_some_and(|d| d.to_string_lossy() == name && f.path.file_name().is_some_and(|n| n == "mod.mink"))
    })
}

fn find_global<'a>(analysis: &'a Analysis, current: &'a AnalyzedFile, name: &str) -> Option<(&'a AnalyzedFile, &'a Symbol)> {
    let pick = |f: &'a AnalyzedFile| {
        f.index
            .symbols
            .iter()
            .find(|s| s.name == name && !matches!(s.kind, SymKind::Field | SymKind::Variant | SymKind::Parameter))
            .map(|s| (f, s))
    };
    pick(current).or_else(|| analysis.files.iter().find_map(pick))
}

fn resolve<'a>(analysis: &'a Analysis, file: &'a AnalyzedFile, token: usize) -> Option<Target<'a>> {
    let text = &file.text;
    let name = file.index.text(text, token);
    let offset = file.index.tokens[token].start;

    if let Some(q) = qualifier(file, token) {
        // `module::item` and `Enum::Variant`.
        if let Some(module) = file_for_module(analysis, &q) {
            if let Some(symbol) = module.index.symbols.iter().find(|s| s.name == name && s.parent.is_none()) {
                return Some(Target::Global { file: module, symbol });
            }
        }
        for f in &analysis.files {
            for (i, owner) in f.index.symbols.iter().enumerate() {
                if owner.kind == SymKind::Enum && owner.name == q {
                    if let Some(variant) = f.index.symbols.iter().find(|s| s.parent == Some(i) && s.name == name) {
                        return Some(Target::Global { file: f, symbol: variant });
                    }
                }
            }
        }
        return None;
    }

    // Locals shadow everything else.
    if let Some(function) = file.index.function_at(offset) {
        let locals = file.index.locals(text, function, offset.max(file.index.tokens[token].end));
        if let Some(symbol) = locals.into_iter().rev().find(|s| s.name == name) {
            let scope = (symbol.start, function.end);
            return Some(Target::Local { symbol, scope });
        }
    }
    if let Some((f, symbol)) = find_global(analysis, file, name) {
        return Some(Target::Global { file: f, symbol });
    }
    builtin(name).map(Target::Builtin)
}

// ---------------------------------------------------------------------- hover

fn keyword_help(word: &str) -> Option<&'static str> {
    Some(match word {
        "fn" => "Declares a function: `fn name(params) -> Type { ... }`.",
        "let" => "Declares a variable. Add `mut` to allow it to change.",
        "mut" => "Marks a binding or a reference as changeable.",
        "const" => "Declares a constant.",
        "struct" => "Declares a record of named fields.",
        "enum" => "Declares a type with a fixed set of variants, optionally carrying a value.",
        "mod" => "Loads `name.mink` (or the bundled module of that name) into the program.",
        "use" => "Names one item of a loaded module: `use module::item;`.",
        "pub" => "Marks a declaration public. Visibility is recorded but not enforced yet.",
        "match" => "Chooses an arm by pattern.",
        "defer" => "Runs a block when the enclosing scope exits.",
        "move" => "Hands ownership of captured values to a deferred block.",
        _ => return None,
    })
}

/// Hover text for the identifier or keyword at `offset`.
pub fn hover(analysis: &Analysis, file: &AnalyzedFile, offset: usize) -> Value {
    let lines = Lines::new(&file.text);
    let Some(i) = file.index.token_at(offset) else { return Value::Null };
    let token = file.index.tokens[i];
    let word = file.index.text(&file.text, i);
    if token.kind.is_keyword() || matches!(token.kind, TokenKind::Defer | TokenKind::Move) {
        return match keyword_help(word) {
            Some(help) => Value::object(vec![
                ("contents", markdown(format!("```mink\n{word}\n```\n\n{help}"))),
                ("range", range_value(&lines, token.start, token.end)),
            ]),
            None => Value::Null,
        };
    }
    if token.kind != TokenKind::Ident {
        return Value::Null;
    }
    let body = match resolve(analysis, file, i) {
        Some(Target::Local { symbol, .. }) => {
            let what = if symbol.kind == SymKind::Parameter { "parameter" } else { "local variable" };
            format!("```mink\n{}\n```\n\n({what})", symbol.detail)
        }
        Some(Target::Global { symbol, .. }) => {
            let mut text = format!("```mink\n{}\n```", symbol.detail);
            if !symbol.doc.is_empty() {
                text.push_str(&format!("\n\n{}", symbol.doc));
            }
            text
        }
        Some(Target::Builtin(b)) => format!("```mink\n{}\n```\n\nBuilt into the compiler's runtime.", builtin_signature(b)),
        None => return Value::Null,
    };
    Value::object(vec![
        ("contents", markdown(body)),
        ("range", range_value(&lines, token.start, token.end)),
    ])
}

// ----------------------------------------------------------------- definition

/// Where the name at `offset` is declared.
pub fn definition(analysis: &Analysis, file: &AnalyzedFile, offset: usize) -> Value {
    let Some(i) = file.index.ident_at(offset) else { return Value::Null };
    match resolve(analysis, file, i) {
        Some(Target::Local { symbol, .. }) => location(file, symbol.name_start, symbol.name_end),
        Some(Target::Global { file: f, symbol }) => location(f, symbol.name_start, symbol.name_end),
        _ => Value::Null,
    }
}

// ----------------------------------------------------------------- references

/// Every use of the name at `offset` in the analysed files, as (file index, start, end).
fn reference_sites(analysis: &Analysis, file: &AnalyzedFile, token: usize) -> Option<(String, Vec<(usize, usize, usize)>)> {
    let name = file.index.text(&file.text, token).to_string();
    let target = resolve(analysis, file, token)?;
    let mut sites = Vec::new();
    match &target {
        Target::Local { scope, .. } => {
            let at = analysis.files.iter().position(|f| f.uri == file.uri)?;
            for (k, t) in file.index.tokens.iter().enumerate() {
                if t.kind == TokenKind::Ident
                    && t.start >= scope.0
                    && t.end <= scope.1
                    && file.index.text(&file.text, k) == name
                    && !is_member_access(file, k)
                {
                    sites.push((at, t.start, t.end));
                }
            }
        }
        Target::Global { file: owner, symbol } => {
            for (at, f) in analysis.files.iter().enumerate() {
                for (k, t) in f.index.tokens.iter().enumerate() {
                    if t.kind != TokenKind::Ident || f.index.text(&f.text, k) != name || is_member_access(f, k) {
                        continue;
                    }
                    // A path like `math::abs` counts only when it names this declaration.
                    if let Some(q) = qualifier(f, k) {
                        let names_owner = file_for_module(analysis, &q).is_some_and(|m| m.uri == owner.uri)
                            || (symbol.kind == SymKind::Variant && symbol.parent.is_some_and(|p| owner.index.symbols[p].name == q));
                        if !names_owner {
                            continue;
                        }
                    } else if symbol.kind == SymKind::Variant {
                        continue;
                    }
                    // A local of the same name hides the declaration inside its function.
                    if let Some(function) = f.index.function_at(t.start) {
                        let hidden = f
                            .index
                            .locals(&f.text, function, t.end)
                            .iter()
                            .any(|l| l.name == name && !(l.name_start == t.start));
                        if hidden && symbol.kind != SymKind::Function {
                            continue;
                        }
                        if hidden {
                            continue;
                        }
                    }
                    // Field declarations and variants of other owners are not this name.
                    if f.index.symbols.iter().any(|s| s.name_start == t.start && matches!(s.kind, SymKind::Field | SymKind::Variant) && !std::ptr::eq(*symbol, s)) {
                        continue;
                    }
                    sites.push((at, t.start, t.end));
                }
            }
        }
        Target::Builtin(_) => return None,
    }
    Some((name, sites))
}

fn is_member_access(file: &AnalyzedFile, k: usize) -> bool {
    k > 0 && file.index.tokens[k - 1].kind == TokenKind::Dot
}

/// Every use of the name at `offset`.
pub fn references(analysis: &Analysis, file: &AnalyzedFile, offset: usize, include_declaration: bool) -> Value {
    let Some(i) = file.index.ident_at(offset) else { return Value::List(Vec::new()) };
    let Some((_, sites)) = reference_sites(analysis, file, i) else { return Value::List(Vec::new()) };
    let declaration = match resolve(analysis, file, i) {
        Some(Target::Local { symbol, .. }) => Some((file.uri.clone(), symbol.name_start)),
        Some(Target::Global { file: f, symbol }) => Some((f.uri.clone(), symbol.name_start)),
        _ => None,
    };
    let mut out = Vec::new();
    for (at, start, end) in sites {
        let f = &analysis.files[at];
        if !include_declaration && declaration.as_ref().is_some_and(|(uri, s)| *uri == f.uri && *s == start) {
            continue;
        }
        out.push(location(f, start, end));
    }
    Value::List(out)
}

// --------------------------------------------------------------------- rename

/// Whether `text` is a valid new name.
pub fn valid_name(text: &str) -> Result<(), String> {
    let mut chars = text.chars();
    let ok_start = chars.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_');
    if !ok_start || !chars.all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(format!("'{text}' is not a valid name: use letters, digits and underscores, not starting with a digit"));
    }
    if KEYWORDS.contains(&text) {
        return Err(format!("'{text}' is a reserved word"));
    }
    if text.starts_with("rt_") {
        return Err("names starting with rt_ are reserved for the runtime".to_string());
    }
    Ok(())
}

fn renamable<'a>(analysis: &'a Analysis, file: &'a AnalyzedFile, offset: usize) -> Result<usize, String> {
    let Some(i) = file.index.ident_at(offset) else {
        return Err("put the cursor on a name to rename it".to_string());
    };
    match resolve(analysis, file, i) {
        Some(Target::Builtin(_)) => Err("runtime functions are built into the compiler and cannot be renamed".to_string()),
        Some(Target::Global { file: f, .. }) if is_bundled(f) => {
            Err("this name belongs to the standard library that ships with MINK, so it cannot be renamed here".to_string())
        }
        Some(_) => Ok(i),
        None => Err("this name is not declared in the files MINK has loaded, so it cannot be renamed".to_string()),
    }
}

fn is_bundled(file: &AnalyzedFile) -> bool {
    file.path.parent().and_then(|p| p.file_name()).is_some_and(|n| n == "stdlib")
}

/// The range that a rename of the name at `offset` would replace, or why it cannot be renamed.
pub fn prepare_rename(analysis: &Analysis, file: &AnalyzedFile, offset: usize) -> Result<Value, String> {
    let i = renamable(analysis, file, offset)?;
    let lines = Lines::new(&file.text);
    let t = file.index.tokens[i];
    Ok(Value::object(vec![
        ("range", range_value(&lines, t.start, t.end)),
        ("placeholder", Value::text(file.index.text(&file.text, i))),
    ]))
}

/// The edits that rename the name at `offset` to `new_name` everywhere it is used.
pub fn rename(analysis: &Analysis, file: &AnalyzedFile, offset: usize, new_name: &str) -> Result<Value, String> {
    valid_name(new_name)?;
    let i = renamable(analysis, file, offset)?;
    let old = file.index.text(&file.text, i).to_string();
    if old == new_name {
        return Ok(Value::object(vec![("changes", Value::object(Vec::<(String, Value)>::new()))]));
    }
    let (_, sites) = reference_sites(analysis, file, i).ok_or("nothing to rename here")?;
    // The new name must not already be taken where the old one lives.
    match resolve(analysis, file, i) {
        Some(Target::Local { scope, .. }) => {
            let function = file.index.function_at(scope.0);
            if let Some(function) = function {
                let taken = file.index.locals(&file.text, function, function.end).iter().any(|l| l.name == new_name);
                if taken {
                    return Err(format!("'{new_name}' is already used by another variable or parameter in this function"));
                }
            }
        }
        Some(Target::Global { symbol, .. }) => {
            let top_level = symbol.parent.is_none();
            let taken = analysis
                .files
                .iter()
                .any(|f| f.index.symbols.iter().any(|s| s.name == new_name && (top_level == s.parent.is_none()) && s.kind != SymKind::Field));
            if taken || builtin(new_name).is_some() {
                return Err(format!("'{new_name}' is already declared, and MINK shares one namespace across all loaded files"));
            }
        }
        _ => {}
    }
    let mut per_file: Vec<(String, Vec<Value>)> = Vec::new();
    for (at, start, end) in sites {
        let f = &analysis.files[at];
        let lines = Lines::new(&f.text);
        let edit = Value::object(vec![("range", range_value(&lines, start, end)), ("newText", Value::text(new_name))]);
        match per_file.iter_mut().find(|(uri, _)| *uri == f.uri) {
            Some((_, edits)) => edits.push(edit),
            None => per_file.push((f.uri.clone(), vec![edit])),
        }
    }
    let changes = Value::Object(per_file.into_iter().map(|(uri, edits)| (uri, Value::List(edits))).collect());
    Ok(Value::object(vec![("changes", changes)]))
}

// ------------------------------------------------------------------- outline

fn symbol_kind(kind: SymKind) -> u32 {
    match kind {
        SymKind::Function => 12,
        SymKind::Struct => 23,
        SymKind::Enum => 10,
        SymKind::Constant => 14,
        SymKind::Variable | SymKind::Parameter => 13,
        SymKind::Module => 2,
        SymKind::Field => 8,
        SymKind::Variant => 22,
    }
}

/// The outline of a file: its declarations, with fields and variants under their owner.
pub fn document_symbols(file: &AnalyzedFile) -> Value {
    let lines = Lines::new(&file.text);
    let symbols = &file.index.symbols;
    let mut top = Vec::new();
    for (i, s) in symbols.iter().enumerate() {
        if s.parent.is_some() {
            continue;
        }
        let children: Vec<Value> = symbols
            .iter()
            .filter(|c| c.parent == Some(i))
            .map(|c| node(&lines, c, Vec::new()))
            .collect();
        top.push(node(&lines, s, children));
    }
    Value::List(top)
}

fn node(lines: &Lines<'_>, s: &Symbol, children: Vec<Value>) -> Value {
    Value::object(vec![
        ("name", Value::text(&s.name)),
        ("detail", Value::text(&s.detail)),
        ("kind", Value::Number(symbol_kind(s.kind) as f64)),
        ("range", range_value(lines, s.start, s.end)),
        ("selectionRange", range_value(lines, s.name_start, s.name_end)),
        ("children", Value::List(children)),
    ])
}

// ---------------------------------------------------------------- completion

fn completion_item(label: &str, kind: u32, detail: &str, doc: &str) -> Value {
    let mut pairs = vec![("label", Value::text(label)), ("kind", Value::Number(kind as f64))];
    if !detail.is_empty() {
        pairs.push(("detail", Value::text(detail)));
    }
    if !doc.is_empty() {
        pairs.push(("documentation", markdown(doc.to_string())));
    }
    Value::object(pairs)
}

fn item_for(s: &Symbol) -> Value {
    let kind = match s.kind {
        SymKind::Function => KIND_METHOD_FUNCTION,
        SymKind::Struct => KIND_STRUCT,
        SymKind::Enum => KIND_ENUM,
        SymKind::Constant => KIND_CONSTANT,
        SymKind::Variable | SymKind::Parameter => KIND_VARIABLE,
        SymKind::Module => KIND_MODULE,
        SymKind::Field => KIND_FIELD,
        SymKind::Variant => KIND_ENUM_MEMBER,
    };
    completion_item(&s.name, kind, &s.detail, &s.doc)
}

/// Completion items at `offset`.
pub fn completion(analysis: &Analysis, file: &AnalyzedFile, offset: usize) -> Value {
    const LIMIT: usize = 500;
    let text = &file.text;
    // What comes right before the word being typed.
    let word_start = {
        let mut s = offset.min(text.len());
        while s > 0 && text.is_char_boundary(s - 1) && {
            let c = text[..s].chars().next_back().unwrap_or(' ');
            c.is_ascii_alphanumeric() || c == '_'
        } {
            s -= text[..s].chars().next_back().map_or(1, char::len_utf8);
        }
        s
    };
    let before = text[..word_start].trim_end_matches([' ', '\t']);
    let mut items: Vec<Value> = Vec::new();

    if let Some(path) = before.strip_suffix("::") {
        let qualifier: String = path
            .chars()
            .rev()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        if let Some(module) = file_for_module(analysis, &qualifier) {
            items.extend(module.index.symbols.iter().filter(|s| s.parent.is_none()).map(item_for));
        }
        for f in &analysis.files {
            for (i, owner) in f.index.symbols.iter().enumerate() {
                if owner.kind == SymKind::Enum && owner.name == qualifier {
                    items.extend(f.index.symbols.iter().filter(|s| s.parent == Some(i)).map(item_for));
                }
            }
        }
        return completion_list(items);
    }
    if before.ends_with('.') && !before.ends_with("..") {
        let mut seen = std::collections::BTreeSet::new();
        for f in &analysis.files {
            for s in f.index.symbols.iter().filter(|s| s.kind == SymKind::Field) {
                if seen.insert(s.name.clone()) {
                    items.push(item_for(s));
                }
            }
        }
        return completion_list(items);
    }

    let mut seen = std::collections::BTreeSet::new();
    if let Some(function) = file.index.function_at(offset) {
        for local in file.index.locals(text, function, offset).iter().rev() {
            if seen.insert(local.name.clone()) {
                items.push(item_for(local));
            }
        }
    }
    for f in &analysis.files {
        for s in f.index.symbols.iter().filter(|s| s.parent.is_none()) {
            if items.len() < LIMIT && seen.insert(s.name.clone()) {
                items.push(item_for(s));
            }
        }
    }
    for b in intrinsics::ALL {
        if items.len() < LIMIT && seen.insert(b.name.to_string()) {
            items.push(completion_item(b.name, KIND_METHOD_FUNCTION, &builtin_signature(b), "Built into the compiler's runtime."));
        }
    }
    for k in KEYWORDS {
        items.push(completion_item(k, KIND_KEYWORD, "", ""));
    }
    completion_list(items)
}

fn completion_list(items: Vec<Value>) -> Value {
    Value::object(vec![("isIncomplete", Value::Bool(false)), ("items", Value::List(items))])
}

// ------------------------------------------------------------ signature help

/// The signature of the call being typed at `offset`.
pub fn signature_help(analysis: &Analysis, file: &AnalyzedFile, offset: usize) -> Value {
    let tokens = &file.index.tokens;
    // Walk back to the `(` that is still open at the cursor.
    let mut depth = 0i32;
    let mut commas = 0usize;
    let mut open = None;
    for k in (0..tokens.len()).rev() {
        let t = tokens[k];
        if t.start >= offset {
            continue;
        }
        match t.kind {
            TokenKind::RParen | TokenKind::RBracket | TokenKind::RBrace => depth += 1,
            TokenKind::LBracket | TokenKind::LBrace => {
                if depth == 0 {
                    return Value::Null;
                }
                depth -= 1;
            }
            TokenKind::LParen => {
                if depth == 0 {
                    open = Some(k);
                    break;
                }
                depth -= 1;
            }
            TokenKind::Comma if depth == 0 => commas += 1,
            TokenKind::Semi if depth == 0 => return Value::Null,
            _ => {}
        }
    }
    let Some(open) = open else { return Value::Null };
    let Some(callee) = open.checked_sub(1).filter(|c| tokens[*c].kind == TokenKind::Ident) else {
        return Value::Null;
    };
    // `fn name(` is a declaration, not a call.
    if callee > 0 && tokens[callee - 1].kind == TokenKind::Fn {
        return Value::Null;
    }
    let (label, doc, params) = match resolve(analysis, file, callee) {
        Some(Target::Global { file: f, symbol }) if symbol.kind == SymKind::Function => {
            (symbol.detail.clone(), symbol.doc.clone(), f.index.parameter_labels(&f.text, symbol))
        }
        Some(Target::Builtin(b)) => {
            let label = builtin_signature(b);
            let params = b.params.iter().map(|p| format!("{p:?}")).collect();
            (label, "Built into the compiler's runtime.".to_string(), params)
        }
        _ => return Value::Null,
    };
    let parameters = params
        .iter()
        .map(|p| Value::object(vec![("label", Value::text(p))]))
        .collect();
    let mut signature = vec![("label", Value::Text(label)), ("parameters", Value::List(parameters))];
    if !doc.is_empty() {
        signature.push(("documentation", markdown(doc)));
    }
    Value::object(vec![
        ("signatures", Value::List(vec![Value::object(signature)])),
        ("activeSignature", Value::Number(0.0)),
        ("activeParameter", Value::Number(commas as f64)),
    ])
}

// ---------------------------------------------------------------- formatting

/// The edits that turn the file into its formatted form: none when it is already formatted.
pub fn formatting(file: &AnalyzedFile) -> Result<Value, String> {
    let formatted = crate::formatter::format_source(&file.text).map_err(|e| e.to_string())?;
    if formatted == *file.text {
        return Ok(Value::List(Vec::new()));
    }
    let old = file.text.as_str();
    // One edit covering the lines that differ, so the editor keeps cursors and folds elsewhere.
    let old_lines: Vec<&str> = old.split_inclusive('\n').collect();
    let new_lines: Vec<&str> = formatted.split_inclusive('\n').collect();
    let mut prefix = 0;
    while prefix < old_lines.len() && prefix < new_lines.len() && old_lines[prefix] == new_lines[prefix] {
        prefix += 1;
    }
    let mut suffix = 0;
    while suffix < old_lines.len() - prefix
        && suffix < new_lines.len() - prefix
        && old_lines[old_lines.len() - 1 - suffix] == new_lines[new_lines.len() - 1 - suffix]
    {
        suffix += 1;
    }
    let start: usize = old_lines[..prefix].iter().map(|l| l.len()).sum();
    let end: usize = old.len() - old_lines[old_lines.len() - suffix..].iter().map(|l| l.len()).sum::<usize>();
    let replacement: String = new_lines[prefix..new_lines.len() - suffix].concat();
    let lines = Lines::new(old);
    Ok(Value::List(vec![Value::object(vec![
        ("range", range_value(&lines, start, end)),
        ("newText", Value::Text(replacement)),
    ])]))
}

/// A small helper for tests and the server: the index of `file` at `offset`'s identifier.
pub fn _index_of(_file: &FileIndex) {}
