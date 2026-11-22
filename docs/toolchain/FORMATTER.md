# mink fmt

`mink fmt` rewrites MINK source into one layout. It has no settings. Every project formats the same way, so
diffs show real changes and nobody argues about spaces.

```powershell
mink fmt .               # format every .mink file under the current folder
mink fmt src\main.mink   # one file
mink fmt --check .       # change nothing, list what would change, exit 1 if anything would
mink fmt --diff .        # change nothing, show what would change
mink fmt --stdin         # read source from standard input, write the result to standard output
mink fmt --stdout a.mink # print one file's result instead of writing it
```

A path is always required (`mink fmt` alone is a usage error), so running it in the wrong folder cannot
rewrite everything under it. Hidden folders (`.git`, `.mink`) and `target`, `node_modules` and `dist` are skipped. Exit code 0 means
everything is formatted (or was formatted); 1 means `--check` or `--diff` found a change, or a file could not be
formatted; 2 means the options were wrong.

## What it promises

**It never changes what a program says.** Only white space moves. The formatter works on the token stream and
puts the same tokens and the same comments back. Strings and characters are copied exactly. Before it gives
a result it lexes its own output and compares it with the input, token by token and comment by comment. If
anything differs, it refuses, writes nothing and says so. That case would be a bug in `mink fmt`.

It also refuses a file that does not lex, such as an unterminated string, and names the line. Fix that first.

Formatting twice gives the same result as formatting once. Files are written through a temporary file and renamed,
so an interrupted run never leaves half a file. Windows line endings and a byte-order mark are kept as they were.

## The layout

- Four spaces for each level, no tabs, no trailing white space, one newline at the end of the file.
- `{` stays on the line of its header. `} else {` stays together when the block before it spans several lines.
- Spaces around binary operators and after commas and colons. None inside parentheses and brackets, and none
  around `.`, `::`, `?` and ranges (`0..n`). Unary operators hug what they apply to: `-x`, `&x`, `*p`, `!ok`.
- A space after `if`, `while`, `match`, `for` and `return` before a parenthesis, none after a function name.
- At most one blank line anywhere, none just after `{` or just before `}`, and one blank line between top-level
  items. Header comments keep their blank line before the first item.
- Comments keep their text. A comment after code is one space away from it. A comment on its own line moves to
  the indent of the code around it.

### Line breaks stay where you put them

`mink fmt` does not reflow expressions. A statement you broke over several lines keeps its breaks. Continuation
lines get one extra level of indent, and lines inside an open parenthesis or bracket hang one level from the line
that opened it:

```mink
let total = alpha +
    beta +
    gamma;
let items = collect(
    first,
    second
);
```

### One statement per line, with one exception

A block with more than one statement is written one statement per line:

```mink
fn main() {
    let a = 1;
    let b = 2;
}
```

The exception is the guard clause. A block on one line that holds a single short statement keeps its line, as long
as the whole line fits in 100 characters and has no comment in it:

```mink
if n < 0 { return 0; }
```

A block of fields, such as `Point { x: 1, y: 2 }`, or of one expression with no `;`, stays as you wrote it.

### Where the tokens cannot tell

`<` and `>` can be a comparison or the edges of a type (`Vec<Int>`), and `|` can be bit-or or a closure bar. From
the tokens alone there is no way to know which, so `mink fmt` keeps the spacing you wrote around them.

## In an editor

`mink fmt --stdin` is the call an editor makes for "format document": it reads the buffer from standard input and
writes the formatted text to standard output. When the source cannot be formatted, it prints nothing to standard
output, explains on standard error and exits with 1, so a failed format never wipes the buffer. The MINK language
server and the VS Code extension use this.

## How it is tested

The unit tests cover each rule and the promises. Two tests run over every `.mink` file in the repository (more
than 300 of them, including the standard library and the examples): each must format, format again to the same
text, keep its tokens and comments, and, for the standard library and the examples, report the same number of
compiler diagnostics before and after. The command-line tests cover writing, `--check`, `--diff`, `--stdin`,
folders and failures.
