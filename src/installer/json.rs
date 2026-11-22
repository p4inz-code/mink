//! The little JSON the installer needs, and nothing more.
//!
//! The installer talks to its own window over a local connection and keeps a
//! small record of what it installed. Both are flat: objects of strings, numbers
//! and booleans, plus lists of strings. This module writes that and reads flat
//! objects back, so the compiler stays free of dependencies.

use std::collections::BTreeMap;

/// A JSON value of the kinds the installer uses.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// `null`.
    Null,
    /// `true` or `false`.
    Bool(bool),
    /// A number.
    Number(f64),
    /// A string.
    Text(String),
    /// A list of values.
    List(Vec<Value>),
    /// An object, keys in order.
    Object(Vec<(String, Value)>),
}

impl Value {
    /// Builds a string value.
    pub fn text(text: impl Into<String>) -> Self {
        Value::Text(text.into())
    }

    /// Builds an object from pairs.
    pub fn object<K: Into<String>>(pairs: Vec<(K, Value)>) -> Self {
        Value::Object(pairs.into_iter().map(|(k, v)| (k.into(), v)).collect())
    }

    /// The text of a string value.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Text(text) => Some(text),
            _ => None,
        }
    }

    /// The value of a boolean.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(flag) => Some(*flag),
            _ => None,
        }
    }

    /// Looks a key up in an object.
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Object(pairs) => pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    /// Writes the value as compact JSON.
    pub fn to_json(&self) -> String {
        let mut out = String::new();
        self.write(&mut out);
        out
    }

    fn write(&self, out: &mut String) {
        match self {
            Value::Null => out.push_str("null"),
            Value::Bool(flag) => out.push_str(if *flag { "true" } else { "false" }),
            Value::Number(n) => {
                if n.fract() == 0.0 && n.abs() < 1e15 {
                    out.push_str(&format!("{}", *n as i64));
                } else {
                    out.push_str(&format!("{n}"));
                }
            }
            Value::Text(text) => write_string(text, out),
            Value::List(items) => {
                out.push('[');
                for (index, item) in items.iter().enumerate() {
                    if index > 0 {
                        out.push(',');
                    }
                    item.write(out);
                }
                out.push(']');
            }
            Value::Object(pairs) => {
                out.push('{');
                for (index, (key, value)) in pairs.iter().enumerate() {
                    if index > 0 {
                        out.push(',');
                    }
                    write_string(key, out);
                    out.push(':');
                    value.write(out);
                }
                out.push('}');
            }
        }
    }
}

fn write_string(text: &str, out: &mut String) {
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

/// Parses any JSON document. Returns `None` for malformed input.
pub fn parse(text: &str) -> Option<Value> {
    let mut parser = Parser {
        chars: text.chars().collect(),
        at: 0,
        depth: 0,
    };
    parser.skip_space();
    let value = parser.value()?;
    parser.skip_space();
    (parser.at == parser.chars.len()).then_some(value)
}

/// Parses a JSON object into a map of keys to values.
pub fn parse_object(text: &str) -> Option<BTreeMap<String, Value>> {
    match parse(text)? {
        Value::Object(pairs) => Some(pairs.into_iter().collect()),
        _ => None,
    }
}

const MAX_DEPTH: usize = 32;

struct Parser {
    chars: Vec<char>,
    at: usize,
    depth: usize,
}

impl Parser {
    fn skip_space(&mut self) {
        while self.chars.get(self.at).is_some_and(|c| c.is_whitespace()) {
            self.at += 1;
        }
    }

    fn eat(&mut self, expected: char) -> bool {
        if self.chars.get(self.at) == Some(&expected) {
            self.at += 1;
            true
        } else {
            false
        }
    }

    fn value(&mut self) -> Option<Value> {
        self.skip_space();
        match *self.chars.get(self.at)? {
            '"' => self.string().map(Value::Text),
            '{' => self.object(),
            '[' => self.list(),
            't' => self.word("true", Value::Bool(true)),
            'f' => self.word("false", Value::Bool(false)),
            'n' => self.word("null", Value::Null),
            _ => self.number(),
        }
    }

    fn word(&mut self, word: &str, value: Value) -> Option<Value> {
        let end = self.at + word.len();
        let found: String = self.chars.get(self.at..end)?.iter().collect();
        if found == word {
            self.at = end;
            Some(value)
        } else {
            None
        }
    }

    fn number(&mut self) -> Option<Value> {
        let start = self.at;
        while self
            .chars
            .get(self.at)
            .is_some_and(|c| c.is_ascii_digit() || matches!(c, '-' | '+' | '.' | 'e' | 'E'))
        {
            self.at += 1;
        }
        let text: String = self.chars[start..self.at].iter().collect();
        text.parse::<f64>().ok().map(Value::Number)
    }

    fn string(&mut self) -> Option<String> {
        if !self.eat('"') {
            return None;
        }
        let mut out = String::new();
        loop {
            let c = *self.chars.get(self.at)?;
            self.at += 1;
            match c {
                '"' => return Some(out),
                '\\' => {
                    let escaped = *self.chars.get(self.at)?;
                    self.at += 1;
                    match escaped {
                        'n' => out.push('\n'),
                        'r' => out.push('\r'),
                        't' => out.push('\t'),
                        'b' => out.push('\u{8}'),
                        'f' => out.push('\u{c}'),
                        'u' => {
                            let hex: String = self.chars.get(self.at..self.at + 4)?.iter().collect();
                            self.at += 4;
                            out.push(char::from_u32(u32::from_str_radix(&hex, 16).ok()?).unwrap_or('\u{fffd}'));
                        }
                        other => out.push(other),
                    }
                }
                c => out.push(c),
            }
        }
    }

    fn list(&mut self) -> Option<Value> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return None;
        }
        self.eat('[');
        let mut items = Vec::new();
        self.skip_space();
        if self.eat(']') {
            self.depth -= 1;
            return Some(Value::List(items));
        }
        loop {
            items.push(self.value()?);
            self.skip_space();
            if self.eat(',') {
                continue;
            }
            if self.eat(']') {
                self.depth -= 1;
                return Some(Value::List(items));
            }
            return None;
        }
    }

    fn object(&mut self) -> Option<Value> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return None;
        }
        self.eat('{');
        let mut pairs = Vec::new();
        self.skip_space();
        if self.eat('}') {
            self.depth -= 1;
            return Some(Value::Object(pairs));
        }
        loop {
            self.skip_space();
            let key = self.string()?;
            self.skip_space();
            if !self.eat(':') {
                return None;
            }
            pairs.push((key, self.value()?));
            self.skip_space();
            if self.eat(',') {
                continue;
            }
            if self.eat('}') {
                self.depth -= 1;
                return Some(Value::Object(pairs));
            }
            return None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_round_trip() {
        let value = Value::object(vec![
            ("name", Value::text("MINK \"1.0.5\"\n\\")),
            ("ok", Value::Bool(true)),
            ("n", Value::Number(42.0)),
            ("pi", Value::Number(3.5)),
            ("none", Value::Null),
            ("list", Value::List(vec![Value::text("a"), Value::Number(1.0)])),
        ]);
        let text = value.to_json();
        assert_eq!(parse(&text), Some(value));
    }

    #[test]
    fn control_characters_are_escaped() {
        let text = Value::text("a\u{1}b").to_json();
        assert_eq!(text, "\"a\\u0001b\"");
        assert_eq!(parse(&text), Some(Value::text("a\u{1}b")));
    }

    #[test]
    fn objects_are_read_into_a_map() {
        let map = parse_object(r#" { "dir" : "C:\\Users\\a b", "on": true, "count": 3 } "#).unwrap();
        assert_eq!(map["dir"].as_str(), Some("C:\\Users\\a b"));
        assert_eq!(map["on"].as_bool(), Some(true));
        assert_eq!(map["count"], Value::Number(3.0));
    }

    #[test]
    fn malformed_input_is_refused() {
        for bad in ["", "{", "{\"a\"}", "{\"a\":}", "[1,]", "\"open", "{\"a\":1} extra", "nul"] {
            assert_eq!(parse(bad), None, "{bad:?}");
        }
        assert_eq!(parse_object("[1]"), None);
    }

    #[test]
    fn very_deep_nesting_is_refused_not_crashed() {
        let deep = format!("{}1{}", "[".repeat(200), "]".repeat(200));
        assert_eq!(parse(&deep), None);
    }

    #[test]
    fn unicode_escapes_are_read() {
        assert_eq!(parse(r#""\u00e9\u65e5""#), Some(Value::text("é日")));
    }
}
