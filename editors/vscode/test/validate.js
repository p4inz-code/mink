'use strict';
// Checks the extension's files without VS Code: every JSON file parses, every command in the
// manifest has a handler, the grammar's regular expressions compile, and every language keyword
// is highlighted.
const fs = require('fs');
const path = require('path');
const root = path.join(__dirname, '..');
let failed = 0;
const check = (ok, message) => { if (!ok) { failed++; console.error('FAIL: ' + message); } else console.log('ok: ' + message); };
const json = (file) => JSON.parse(fs.readFileSync(path.join(root, file), 'utf8'));

const pkg = json('package.json');
const grammar = json('syntaxes/mink.tmLanguage.json');
const snippets = json('snippets/mink.json');
json('language-configuration.json');
check(true, 'every JSON file parses');

const source = fs.readFileSync(path.join(root, 'src/extension.js'), 'utf8');
for (const c of pkg.contributes.commands) check(source.includes("'" + c.command + "'"), 'command ' + c.command + ' has a handler');
for (const file of [pkg.icon, pkg.contributes.languages[0].icon.dark, pkg.contributes.grammars[0].path, pkg.contributes.snippets[0].path, pkg.main]) {
  check(fs.existsSync(path.join(root, file)), file + ' exists');
}

let regexes = 0;
(function walk(node) {
  if (Array.isArray(node)) return node.forEach(walk);
  if (node && typeof node === 'object') {
    for (const [k, v] of Object.entries(node)) {
      if (['match', 'begin', 'end'].includes(k) && typeof v === 'string') {
        try { new RegExp(v); regexes++; } catch (e) { check(false, 'regex ' + v + ': ' + e.message); }
      } else walk(v);
    }
  }
})(grammar.repository);
check(regexes > 20, regexes + ' grammar expressions compile');

const rule = (name) => grammar.repository[name].patterns;
const hits = (group, text) => rule(group).some((p) => p.match && new RegExp(p.match).test(text));
check(hits('keywords', 'while x {'), 'keywords: while');
check(hits('constants', 'let a = true;'), 'constants: true');
check(hits('runtime', 'rt_print_int(1)'), 'runtime: rt_print_int');
check(hits('numbers', 'let n = 0xFF;'), 'numbers: hex');
check(hits('declarations', 'fn main() {'), 'declarations: fn');

const rs = fs.readFileSync(path.join(root, '../../src/lsp/scan.rs'), 'utf8');
const list = rs.match(/pub const KEYWORDS: &\[&str\] = &\[([\s\S]*?)\];/)[1].match(/"[a-z]+"/g).map((s) => s.slice(1, -1));
const text = JSON.stringify(grammar);
for (const k of list) check(new RegExp('\\b' + k + '\\b').test(text), 'keyword ' + k + ' is highlighted');
check(Object.keys(snippets).length >= 10, 'snippets are present');
process.exit(failed ? 1 : 0);
