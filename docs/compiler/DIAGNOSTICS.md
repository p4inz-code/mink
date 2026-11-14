# MINK diagnostics: human-readable and machine-readable output

**Status:** implemented (M7 Phase C, Session 279; schema version 1)
**Sources:** `src/diagnostics/mod.rs` (JSON renderer), `src/cli.rs` (output
selection and exit codes), `src/driver.rs` (`CheckReport`, error ordering)

This document is the contract a tool consumes. It covers both output modes of
`mink check`, the JSON schema field by field, how source locations are
attributed across modules, and what the schema currently cannot express.

---

## 1. Modes

| Invocation | stdout on success | stdout on errors | stderr |
| --- | --- | --- | --- |
| `mink check file.mink` | one "passed …" sentence | human diagnostics | empty |
| `mink check file.mink --json` | JSON report | JSON report | empty |
| `mink build` / `mink run` | program output | human diagnostics | empty |
| usage errors (missing path, unknown flag) | empty | — | usage message |

`--json` only changes `check`. `build` and `run` keep human diagnostics,
because their stdout is program output.

Exit codes: `0` no errors; `1` any error — lex, parse, semantic, type, HIR,
MIR, backend, or a driver-level failure such as an unreadable file; for
`mink run`, the program's own status. There is no separate internal-error
status: a compiler panic is a crash, not a documented exit code.

## 2. Human-readable format

```
mink: error[E-T02]: cannot apply operator `+` to types `Bool` and `Bool`
  --> helper.mink:2:12
  = note: related location is here
  --> other.mink:4:5
```

- the first line carries `mink: error[CODE]: MESSAGE`;
- `--> file:line:column` is the primary location, resolved from the error's own
  span, so an error inside a module file names that file;
- optional `= note:` plus a second `-->` line is the related location (for
  example "previous declaration is here" for a duplicate definition);
- ordering is deterministic: file (as a source id), then byte offset, then
  error code.

## 3. JSON schema, version 1

```json
{
  "schema_version": 1,
  "success": false,
  "exit_code": 1,
  "files_checked": 2,
  "files": ["main.mink", "helper.mink"],
  "token_count": 37,
  "errors": [
    {
      "code": "E-S01",
      "severity": "error",
      "stage": "semantics",
      "message": "cannot find `nope` in module `helper`",
      "span": {
        "file": "main.mink",
        "start_byte": 21,
        "end_byte": 25,
        "start_line": 2,
        "start_column": 13,
        "end_line": 2,
        "end_column": 17
      },
      "related": [
        {
          "message": "previous declaration is here",
          "file": "other.mink",
          "start_byte": 4,
          "end_byte": 10,
          "line": 1,
          "column": 8
        }
      ],
      "suggestions": []
    }
  ],
  "warnings": []
}
```

### Fields

| Field | Type | Meaning |
| --- | --- | --- |
| `schema_version` | integer | `1`. Bumped only for a breaking change; additive fields keep the version. |
| `success` | boolean | `true` when `errors` is empty. |
| `exit_code` | integer | The status this invocation returns (`0` or `1`). |
| `files_checked` | integer | Number of source files the compilation loaded, including modules. |
| `files` | string[] | Those files, in source-map order: the root first, then each module in the order it was loaded. |
| `token_count` | integer | Tokens across all `files_checked`; `0` when the pipeline stopped before lexing. |
| `errors` | object[] | Diagnostics, deterministically ordered (see §2). |
| `errors[].code` | string \| null | Stable code (`E-L*`, `E-P*`, `E-S*`, `E-T*`, `E-H*`, `E-M*`, `E-B*`). `null` for failures that are not compiler diagnostics. |
| `errors[].severity` | string | `"error"`. Warnings are separate (`warnings`, currently always empty). |
| `errors[].stage` | string | Pipeline stage that produced it: `lexing`, `parsing`, `semantics`, `typecheck`, `hir`, `mir`, `backend`, `driver`. |
| `errors[].message` | string | Complete human-readable message (§2). |
| `errors[].span` | object \| null | Primary location, or `null` when no real source location exists. |
| `span.file` | string | Source file the span belongs to. |
| `span.start_byte` / `span.end_byte` | integer | Byte offsets into that file, half-open. Slice the file with them to get the offending text. |
| `span.start_line` / `end_line` | integer | 1-based. |
| `span.start_column` / `end_column` | integer | 1-based **byte** columns, not Unicode scalar columns. |
| `errors[].related` | object[] | Secondary locations; each carries its own `file`. Empty when there are none. |
| `errors[].suggestions` | string[] | Fix suggestions. No diagnostic computes one yet; always empty. |
| `warnings` | object[] | Non-fatal diagnostics. None are emitted yet. |

Additive fields may appear in later sessions without a version bump;
consumers must ignore unknown fields.

### Path form

`file`, `files[]`, and `related[].file` are the paths the compiler used: the
path passed on the command line for the root, and the resolved path for each
module (`<dir of the declaring file>/<name>.mink`, or
`<name>/mod.mink`). They are not normalised to relative form, so a tool should
treat them as opaque and open them as given.

## 4. Source attribution across modules

Every diagnostic carries its own `SourceId` in its `Span`. The renderer resolves
that id against the source map, so:

- an error located in `helper.mink` reports `helper.mink` and its own line and
  byte offset, not the root file's;
- `files_checked` counts every loaded file;
- a module file that cannot be read is reported at the `mod name;` declaration
  in the declaring file (the only real source location involved);
- a failure with no real location (an unreadable root file, a directory
  argument, an I/O error) has `"span": null` and `"stage": "driver"`.

Before Session 279 all spans were resolved against the root file, so a tool
jumping to a module error landed at an unrelated position in the root file.

## 5. Determinism

The same input produces byte-identical JSON. Errors are sorted by `(source id,
start byte, code)`; `files` is in load order; no field contains a timestamp, a
random id, or a hash-ordered map. `tests/diagnostics_json.rs`
(`C-JSON-DET`) compares two runs byte for byte.

## 6. Limitations

- **Usage errors stay human.** `mink check` with no path, or with an unknown
  flag, prints a usage message on stderr and nothing on stdout, because the
  arguments were never parsed far enough to know whether JSON was requested.
- **No suggestions.** The schema has room, but no diagnostic produces one.
- **No warnings.** `warnings` exists for forward compatibility and is empty.
- **Byte columns.** Editors that count UTF-16 code units or characters must
  convert; `start_byte` is exact and is the safer field to slice with.
- **No streaming.** Diagnostics are printed once, after the pipeline stage that
  failed, so an early parse error suppresses later semantic errors.
- **Text only.** There is no SARIF or LSP output yet; this schema is the
  dependency-free minimum those formats would be built from.

## 7. Consumers

`tests/cli.rs`, `tests/diagnostics_json.rs` (schema, spans, determinism,
failure paths), and `tests/phase_c_cross_gate.rs` (modules + generics +
attribution) exercise this contract against the real binary. A minimal JSON
validator in `tests/diagnostics_json.rs` checks well-formedness without adding
a dependency to the compiler crate, which has none by design.
