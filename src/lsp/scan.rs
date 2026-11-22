//! A token-level index of a MINK file: its declarations and, for any function, its locals.
//!
//! The language server needs answers for code that is half typed and does not
//! parse. So the index is built from tokens alone, never from the syntax tree: it
//! finds `fn`, `struct`, `enum`, `const`, `let` and `mod` at the top level by
//! keyword, and skips over their bodies by matching braces. The compiler still
//! produces the diagnostics; this only locates names.

use crate::lexer::{lex, TokenKind};
use crate::source::{SourceFile, SourceId};

/// What a declaration is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymKind {
    /// A function.
    Function,
    /// A struct.
    Struct,
    /// An enum.
    Enum,
    /// A top-level `const`.
    Constant,
    /// A `let` binding or a loop variable.
    Variable,
    /// A function parameter.
    Parameter,
    /// A `mod` declaration.
    Module,
    /// A field of a struct.
    Field,
    /// A variant of an enum.
    Variant,
}

/// A declared name.
#[derive(Debug, Clone)]
pub struct Symbol {
    /// The name.
    pub name: String,
    /// What it is.
    pub kind: SymKind,
    /// Byte offset where the name starts.
    pub name_start: usize,
    /// Byte offset where the name ends.
    pub name_end: usize,
    /// Byte offset where the whole declaration starts (including `pub`).
    pub start: usize,
    /// Byte offset where the whole declaration ends.
    pub end: usize,
    /// The declaration's signature or header as written, for hover and signature help.
    pub detail: String,
    /// The `//` comment lines directly above the declaration, joined.
    pub doc: String,
    /// The index of the symbol this one belongs to (a struct for a field, an enum for a variant).
    pub parent: Option<usize>,
}

/// A token with its byte range.
#[derive(Debug, Clone, Copy)]
pub struct Tok {
    /// The kind.
    pub kind: TokenKind,
    /// Start byte.
    pub start: usize,
    /// End byte.
    pub end: usize,
}

/// The tokens and top-level declarations of one file.
#[derive(Debug, Clone)]
pub struct FileIndex {
    /// Every token except the final end-of-input marker.
    pub tokens: Vec<Tok>,
    /// Top-level declarations in source order, with fields and variants after their owner.
    pub symbols: Vec<Symbol>,
    matches: Vec<Option<usize>>,
}

/// Every reserved word of the language, for completion and hover.
pub const KEYWORDS: &[&str] = &[
    "fn", "let", "mut", "const", "type", "struct", "enum", "trait", "impl", "mod", "use", "pub", "if", "else",
    "match", "loop", "while", "for", "in", "return", "break", "continue", "defer", "move", "async", "await",
    "unsafe", "true", "false", "null",
];

fn doc_before(text: &str, start: usize) -> String {
    let before = &text[..start];
    let mut lines: Vec<&str> = Vec::new();
    for line in before.lines().rev().skip(usize::from(!before.ends_with('\n') && !before.is_empty())) {
        let trimmed = line.trim();
        if let Some(comment) = trimmed.strip_prefix("//") {
            lines.push(comment.strip_prefix(' ').unwrap_or(comment));
        } else {
            break;
        }
    }
    lines.reverse();
    lines.join("\n")
}

impl FileIndex {
    /// Builds the index of `text`.
    pub fn build(text: &str) -> Self {
        let file = SourceFile::new(SourceId::new(0), "index", text.to_string());
        let lexed = lex(&file);
        let tokens: Vec<Tok> = lexed
            .tokens()
            .iter()
            .filter(|t| t.kind() != TokenKind::Eof)
            .map(|t| Tok {
                kind: t.kind(),
                start: t.span().start() as usize,
                end: t.span().end() as usize,
            })
            .collect();
        let matches = match_brackets(&tokens);
        let mut index = FileIndex {
            tokens,
            symbols: Vec::new(),
            matches,
        };
        index.scan(text);
        index
    }

    /// The text of token `i`.
    pub fn text<'a>(&self, source: &'a str, i: usize) -> &'a str {
        &source[self.tokens[i].start..self.tokens[i].end]
    }

    /// The index of the bracket that matches the one at token `i`.
    pub fn matching(&self, i: usize) -> Option<usize> {
        self.matches.get(i).copied().flatten()
    }

    /// The token that contains `offset`, or the identifier that ends exactly at it (a cursor just
    /// after a word counts as being on the word).
    pub fn token_at(&self, offset: usize) -> Option<usize> {
        let inside = self.tokens.iter().position(|t| t.start <= offset && offset < t.end);
        if let Some(i) = inside {
            return Some(i);
        }
        self.tokens
            .iter()
            .position(|t| t.end == offset && matches!(t.kind, TokenKind::Ident) || (t.end == offset && t.kind.is_keyword()))
    }

    /// The identifier token under (or just before) `offset`.
    pub fn ident_at(&self, offset: usize) -> Option<usize> {
        if let Some(i) = self.token_at(offset) {
            if self.tokens[i].kind == TokenKind::Ident {
                return Some(i);
            }
        }
        // Right after a word, as when the cursor sits at the end of `foo` in `foo(`.
        self.tokens.iter().position(|t| t.kind == TokenKind::Ident && t.end == offset)
    }

    fn scan(&mut self, text: &str) {
        let mut i = 0;
        while i < self.tokens.len() {
            let start_index = i;
            let mut j = i;
            if self.tokens[j].kind == TokenKind::Pub {
                j += 1;
            }
            let Some(head) = self.tokens.get(j).map(|t| t.kind) else { break };
            i = match head {
                TokenKind::Fn => self.scan_fn(text, start_index, j),
                TokenKind::Struct => self.scan_struct(text, start_index, j),
                TokenKind::Enum => self.scan_enum(text, start_index, j),
                TokenKind::Const | TokenKind::Let => self.scan_binding(text, start_index, j),
                TokenKind::Mod => self.scan_mod(text, start_index, j),
                _ => start_index + 1,
            }
            .max(start_index + 1);
        }
    }

    fn name_token(&self, after: usize) -> Option<usize> {
        let i = after + 1;
        (self.tokens.get(i)?.kind == TokenKind::Ident).then_some(i)
    }

    fn push(&mut self, text: &str, kind: SymKind, name: usize, start_tok: usize, end_byte: usize, detail_end: usize, parent: Option<usize>) -> usize {
        let t = self.tokens[name];
        let start = self.tokens[start_tok].start;
        let detail = text[start..detail_end.max(start).min(text.len())].trim().to_string();
        self.symbols.push(Symbol {
            name: text[t.start..t.end].to_string(),
            kind,
            name_start: t.start,
            name_end: t.end,
            start,
            end: end_byte.max(start),
            detail,
            doc: doc_before(text, start),
            parent,
        });
        self.symbols.len() - 1
    }

    /// The index of the first `kind` token from `from` that is not nested in brackets, stopping at
    /// a `;` or `}` at depth 0.
    fn find_at_depth0(&self, from: usize, kinds: &[TokenKind]) -> Option<usize> {
        let mut i = from;
        while i < self.tokens.len() {
            let kind = self.tokens[i].kind;
            if kinds.contains(&kind) {
                return Some(i);
            }
            match kind {
                TokenKind::LParen | TokenKind::LBracket => {
                    i = self.matching(i).map_or(self.tokens.len(), |m| m + 1);
                    continue;
                }
                TokenKind::Semi | TokenKind::RBrace => return None,
                _ => {}
            }
            i += 1;
        }
        None
    }

    fn scan_fn(&mut self, text: &str, start_tok: usize, at: usize) -> usize {
        let Some(name) = self.name_token(at) else { return start_tok + 1 };
        let open_paren = self.find_at_depth0(name + 1, &[TokenKind::LParen]);
        let body = open_paren.and_then(|p| self.matching(p)).and_then(|close| self.find_at_depth0(close + 1, &[TokenKind::LBrace, TokenKind::Semi]));
        let (detail_end, end_byte, next) = match body {
            Some(b) if self.tokens[b].kind == TokenKind::LBrace => {
                let close = self.matching(b);
                (
                    self.tokens[b].start,
                    close.map_or(text.len(), |c| self.tokens[c].end),
                    close.map_or(self.tokens.len(), |c| c + 1),
                )
            }
            Some(b) => (self.tokens[b].start, self.tokens[b].end, b + 1),
            None => (self.tokens[name].end, self.tokens[name].end, name + 1),
        };
        self.push(text, SymKind::Function, name, start_tok, end_byte, detail_end, None);
        next
    }

    fn scan_struct(&mut self, text: &str, start_tok: usize, at: usize) -> usize {
        let Some(name) = self.name_token(at) else { return start_tok + 1 };
        let open = self.find_at_depth0(name + 1, &[TokenKind::LBrace, TokenKind::Semi]);
        let Some(open) = open.filter(|o| self.tokens[*o].kind == TokenKind::LBrace) else {
            self.push(text, SymKind::Struct, name, start_tok, self.tokens[name].end, self.tokens[name].end, None);
            return name + 1;
        };
        let close = self.matching(open);
        let end_tok = close.unwrap_or(self.tokens.len() - 1);
        let owner = self.push(text, SymKind::Struct, name, start_tok, self.tokens[end_tok].end, self.tokens[open].start, None);
        // Fields: `name:` at the top of the braces.
        let mut k = open + 1;
        while k < end_tok {
            if self.tokens[k].kind == TokenKind::Ident && self.tokens.get(k + 1).is_some_and(|t| t.kind == TokenKind::Colon) {
                let value_end = self.find_at_depth0(k + 2, &[TokenKind::Comma, TokenKind::RBrace]).unwrap_or(end_tok);
                let detail_end = self.tokens[value_end.min(end_tok)].start;
                self.push(text, SymKind::Field, k, k, self.tokens[k].end, detail_end, Some(owner));
                k = value_end.max(k + 1);
            } else {
                k += 1;
            }
        }
        end_tok + 1
    }

    fn scan_enum(&mut self, text: &str, start_tok: usize, at: usize) -> usize {
        let Some(name) = self.name_token(at) else { return start_tok + 1 };
        let open = self.find_at_depth0(name + 1, &[TokenKind::LBrace, TokenKind::Semi]);
        let Some(open) = open.filter(|o| self.tokens[*o].kind == TokenKind::LBrace) else {
            self.push(text, SymKind::Enum, name, start_tok, self.tokens[name].end, self.tokens[name].end, None);
            return name + 1;
        };
        let close = self.matching(open);
        let end_tok = close.unwrap_or(self.tokens.len() - 1);
        let owner = self.push(text, SymKind::Enum, name, start_tok, self.tokens[end_tok].end, self.tokens[open].start, None);
        let mut k = open + 1;
        let mut expect_variant = true;
        while k < end_tok {
            match self.tokens[k].kind {
                TokenKind::Ident if expect_variant => {
                    let next_comma = self.find_at_depth0(k + 1, &[TokenKind::Comma, TokenKind::RBrace]).unwrap_or(end_tok);
                    let detail_end = self.tokens[next_comma.min(end_tok)].start;
                    self.push(text, SymKind::Variant, k, k, self.tokens[k].end, detail_end, Some(owner));
                    expect_variant = false;
                    k = next_comma.max(k + 1);
                }
                TokenKind::Comma => {
                    expect_variant = true;
                    k += 1;
                }
                TokenKind::LParen | TokenKind::LBracket => k = self.matching(k).map_or(end_tok, |m| m + 1),
                _ => k += 1,
            }
        }
        end_tok + 1
    }

    fn scan_binding(&mut self, text: &str, start_tok: usize, at: usize) -> usize {
        let mut name_at = at + 1;
        if self.tokens.get(name_at).is_some_and(|t| t.kind == TokenKind::Mut) {
            name_at += 1;
        }
        let Some(name) = self.tokens.get(name_at).filter(|t| t.kind == TokenKind::Ident).map(|_| name_at) else {
            return start_tok + 1;
        };
        let semi = self.find_at_depth0(name + 1, &[TokenKind::Semi]);
        let end_byte = semi.map_or(self.tokens[name].end, |s| self.tokens[s].end);
        let header_end = self
            .find_at_depth0(name + 1, &[TokenKind::Eq, TokenKind::Semi])
            .map_or(self.tokens[name].end, |e| self.tokens[e].start);
        let kind = if self.tokens[at].kind == TokenKind::Const { SymKind::Constant } else { SymKind::Variable };
        self.push(text, kind, name, start_tok, end_byte, header_end, None);
        semi.map_or(name + 1, |s| s + 1)
    }

    fn scan_mod(&mut self, text: &str, start_tok: usize, at: usize) -> usize {
        let Some(name) = self.name_token(at) else { return start_tok + 1 };
        let end = self.tokens.get(name + 1).filter(|t| t.kind == TokenKind::Semi).map_or(name, |_| name + 1);
        self.push(text, SymKind::Module, name, start_tok, self.tokens[end].end, self.tokens[name].end, None);
        end + 1
    }

    /// The function whose declaration contains `offset`.
    pub fn function_at(&self, offset: usize) -> Option<&Symbol> {
        self.symbols
            .iter()
            .find(|s| s.kind == SymKind::Function && s.start <= offset && offset <= s.end)
    }

    /// The parameters and locals of `function` that are in scope at `offset`: its parameters, the
    /// `let` bindings and loop variables declared before `offset`, innermost declaration last.
    pub fn locals(&self, text: &str, function: &Symbol, offset: usize) -> Vec<Symbol> {
        let mut found = Vec::new();
        // Parameters: `name` or `name: Type`, separated by commas inside the parentheses.
        let Some(name_tok) = self.tokens.iter().position(|t| t.start == function.name_start) else {
            return found;
        };
        let Some(open) = self.find_at_depth0(name_tok + 1, &[TokenKind::LParen]) else { return found };
        let close = self.matching(open).unwrap_or(open);
        let mut k = open + 1;
        let mut at_start = true;
        while k < close {
            match self.tokens[k].kind {
                TokenKind::Comma => {
                    at_start = true;
                    k += 1;
                }
                TokenKind::Mut | TokenKind::Amp => k += 1,
                TokenKind::Ident if at_start => {
                    let end = self.find_at_depth0(k + 1, &[TokenKind::Comma, TokenKind::RParen]).unwrap_or(close).min(close);
                    found.push(self.local_symbol(text, SymKind::Parameter, k, k, self.tokens[end].start));
                    at_start = false;
                    k = end;
                }
                TokenKind::Lt | TokenKind::LParen | TokenKind::LBracket => {
                    k = if self.tokens[k].kind == TokenKind::Lt { k + 1 } else { self.matching(k).map_or(close, |m| m + 1) };
                }
                _ => k += 1,
            }
        }
        // Bindings inside the body that are declared before the cursor.
        let body_open = self.find_at_depth0(close + 1, &[TokenKind::LBrace]);
        let body_end = body_open.and_then(|b| self.matching(b)).unwrap_or(self.tokens.len());
        let mut k = body_open.map_or(self.tokens.len(), |b| b + 1);
        while k < body_end {
            let t = self.tokens[k];
            if t.start >= offset {
                break;
            }
            match t.kind {
                TokenKind::Let => {
                    let mut n = k + 1;
                    if self.tokens.get(n).is_some_and(|t| t.kind == TokenKind::Mut) {
                        n += 1;
                    }
                    if self.tokens.get(n).is_some_and(|t| t.kind == TokenKind::Ident) {
                        let header_end = self
                            .find_at_depth0(n + 1, &[TokenKind::Eq, TokenKind::Semi])
                            .map_or(self.tokens[n].end, |e| self.tokens[e].start);
                        // A binding is in scope after its own statement ends.
                        let decl_end = self.find_at_depth0(n + 1, &[TokenKind::Semi]).map_or(self.tokens[n].end, |s| self.tokens[s].end);
                        if decl_end <= offset || (self.tokens[n].start <= offset && offset <= self.tokens[n].end) {
                            found.push(self.local_symbol(text, SymKind::Variable, n, k, header_end));
                        }
                    }
                }
                TokenKind::For => {
                    if self.tokens.get(k + 1).is_some_and(|t| t.kind == TokenKind::Ident) {
                        found.push(self.local_symbol(text, SymKind::Variable, k + 1, k, self.tokens[k + 1].end));
                    }
                }
                _ => {}
            }
            k += 1;
        }
        found
    }

    fn local_symbol(&self, text: &str, kind: SymKind, name: usize, start_tok: usize, detail_end: usize) -> Symbol {
        let t = self.tokens[name];
        let start = self.tokens[start_tok].start;
        Symbol {
            name: text[t.start..t.end].to_string(),
            kind,
            name_start: t.start,
            name_end: t.end,
            start,
            end: t.end,
            detail: text[start..detail_end.max(start).min(text.len())].trim().to_string(),
            doc: String::new(),
            parent: None,
        }
    }

    /// The splitting of a function's `(...)` into one label per parameter, for signature help.
    pub fn parameter_labels(&self, text: &str, function: &Symbol) -> Vec<String> {
        let Some(name_tok) = self.tokens.iter().position(|t| t.start == function.name_start) else {
            return Vec::new();
        };
        let Some(open) = self.find_at_depth0(name_tok + 1, &[TokenKind::LParen]) else { return Vec::new() };
        let close = self.matching(open).unwrap_or(open);
        let mut labels = Vec::new();
        let mut from = self.tokens.get(open + 1).map_or(text.len(), |t| t.start);
        let mut angle = 0i32;
        for k in open + 1..close {
            match self.tokens[k].kind {
                TokenKind::Lt => angle += 1,
                TokenKind::Gt => angle -= 1,
                TokenKind::Comma if angle <= 0 => {
                    labels.push(text[from..self.tokens[k].start].trim().to_string());
                    from = self.tokens.get(k + 1).map_or(text.len(), |t| t.start);
                }
                _ => {}
            }
        }
        if close > open + 1 {
            labels.push(text[from..self.tokens[close - 1].end].trim().to_string());
        }
        labels.retain(|l| !l.is_empty());
        labels
    }
}

fn match_brackets(tokens: &[Tok]) -> Vec<Option<usize>> {
    let mut matches = vec![None; tokens.len()];
    let mut stack: Vec<usize> = Vec::new();
    for (i, token) in tokens.iter().enumerate() {
        match token.kind {
            TokenKind::LBrace | TokenKind::LParen | TokenKind::LBracket => stack.push(i),
            TokenKind::RBrace | TokenKind::RParen | TokenKind::RBracket => {
                let wanted = match token.kind {
                    TokenKind::RBrace => TokenKind::LBrace,
                    TokenKind::RParen => TokenKind::LParen,
                    _ => TokenKind::LBracket,
                };
                if let Some(position) = stack.iter().rposition(|&open| tokens[open].kind == wanted) {
                    let open = stack[position];
                    matches[open] = Some(i);
                    matches[i] = Some(open);
                    stack.truncate(position);
                }
            }
            _ => {}
        }
    }
    matches
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(index: &FileIndex) -> Vec<(String, SymKind)> {
        index.symbols.iter().map(|s| (s.name.clone(), s.kind)).collect()
    }

    #[test]
    fn top_level_declarations_are_found_by_keyword() {
        let text = "mod math;\nconst LIMIT = 10;\nlet mut total = 0;\nstruct Point { x: Int, y: Int }\nenum Color { Red, Green(Int), Blue }\npub fn add(a: Int, b: Int) -> Int { return a + b; }\nfn main() {}\n";
        let index = FileIndex::build(text);
        let got = names(&index);
        for expected in [
            ("math", SymKind::Module),
            ("LIMIT", SymKind::Constant),
            ("total", SymKind::Variable),
            ("Point", SymKind::Struct),
            ("x", SymKind::Field),
            ("y", SymKind::Field),
            ("Color", SymKind::Enum),
            ("Red", SymKind::Variant),
            ("Green", SymKind::Variant),
            ("Blue", SymKind::Variant),
            ("add", SymKind::Function),
            ("main", SymKind::Function),
        ] {
            assert!(got.contains(&(expected.0.to_string(), expected.1)), "{expected:?} in {got:?}");
        }
        let add = index.symbols.iter().find(|s| s.name == "add").unwrap();
        assert_eq!(add.detail, "pub fn add(a: Int, b: Int) -> Int");
        let green = index.symbols.iter().find(|s| s.name == "Green").unwrap();
        assert_eq!(green.detail, "Green(Int)");
        assert_eq!(index.symbols[green.parent.unwrap()].name, "Color");
    }

    #[test]
    fn doc_comments_above_a_declaration_are_collected() {
        let text = "// Adds two numbers.\n// Overflow wraps.\nfn add(a: Int, b: Int) -> Int { return a + b; }\n\n// not attached\n\nfn other() {}\n";
        let index = FileIndex::build(text);
        let add = index.symbols.iter().find(|s| s.name == "add").unwrap();
        assert_eq!(add.doc, "Adds two numbers.\nOverflow wraps.");
        let other = index.symbols.iter().find(|s| s.name == "other").unwrap();
        assert_eq!(other.doc, "", "a blank line detaches the comment");
    }

    #[test]
    fn bodies_are_skipped_so_locals_are_not_top_level() {
        let text = "fn main() {\n    let x = 1;\n    fn_like();\n}\nlet y = 2;\n";
        let index = FileIndex::build(text);
        let got: Vec<_> = index.symbols.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(got, vec!["main", "y"]);
    }

    #[test]
    fn locals_and_parameters_in_scope_at_a_position() {
        let text = "fn work(a: Int, b) -> Int {\n    let first = a;\n    let mut second = first + b;\n    for i in 0..3 {\n        let inner = i;\n    }\n    return second;\n}\n";
        let index = FileIndex::build(text);
        let work = index.symbols[0].clone();
        let at_return = text.find("return").unwrap();
        let locals = index.locals(text, &work, at_return);
        let names: Vec<_> = locals.iter().map(|s| (s.name.as_str(), s.kind)).collect();
        assert!(names.contains(&("a", SymKind::Parameter)) && names.contains(&("b", SymKind::Parameter)), "{names:?}");
        assert!(names.contains(&("first", SymKind::Variable)) && names.contains(&("second", SymKind::Variable)), "{names:?}");
        let early = index.locals(text, &work, text.find("let first").unwrap());
        assert!(early.iter().all(|s| s.name != "first"), "a binding is not in scope before it is declared");
    }

    #[test]
    fn a_binding_is_not_in_scope_inside_its_own_initializer() {
        let text = "fn f() {\n    let x = x_value;\n}\n";
        let index = FileIndex::build(text);
        let f = index.symbols[0].clone();
        let inside = text.find("x_value").unwrap();
        assert!(index.locals(text, &f, inside).iter().all(|s| s.name != "x"));
    }

    #[test]
    fn parameter_labels_split_on_top_level_commas_only() {
        let text = "fn f(a: Int, m: Map<Str, Int>, c) -> Int {}\n";
        let index = FileIndex::build(text);
        let labels = index.parameter_labels(text, &index.symbols[0]);
        assert_eq!(labels, vec!["a: Int", "m: Map<Str, Int>", "c"]);
        let none = FileIndex::build("fn g() {}\n");
        assert!(none.parameter_labels("fn g() {}\n", &none.symbols[0]).is_empty());
    }

    #[test]
    fn broken_code_does_not_stop_the_index() {
        let text = "fn good() {}\nfn broken( {\nstruct S { x: Int\nfn after() {}\n";
        let index = FileIndex::build(text);
        let got: Vec<_> = index.symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(got.contains(&"good"), "{got:?}");
        let _ = FileIndex::build("fn (((");
        let _ = FileIndex::build("}}}{{{");
        let _ = FileIndex::build("");
    }

    #[test]
    fn the_identifier_at_a_cursor_includes_the_word_just_before_it() {
        let text = "fn main() { foo(1); }";
        let index = FileIndex::build(text);
        let at = text.find("foo").unwrap();
        for offset in [at, at + 1, at + 3] {
            let i = index.ident_at(offset).expect("on or just after the word");
            assert_eq!(index.text(text, i), "foo");
        }
        assert!(index.ident_at(text.find("1").unwrap()).is_none());
    }
}
