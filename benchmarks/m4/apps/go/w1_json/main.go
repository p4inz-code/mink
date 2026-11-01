// M4 W1 `jsontool` — Go port.
//
// Contract: `docs/benchmarks/M4/M4_SESSION_151.md` (reduced from the M1
// `validation/c_json` application).
//
// The file is read and parsed before the command is dispatched (M1 ordering),
// and any `-`-prefixed argument is a usage error because the tool has no
// options.
//
// `compact` uses encoding/json's json.Compact, which removes insignificant
// whitespace while preserving member order and the original number/escape
// text. That is byte-identical to re-serialization for the committed W1
// fixtures; the difference is recorded in the contract audit.
package main

import (
	"bytes"
	"encoding/json"
	"errors"
	"io"
	"os"
	"strconv"
)

const usage = "usage: jsontool <validate|type|sum|compact> <file> [args]"

func writeLine(file *os.File, text string) {
	_, _ = file.WriteString(text + "\r\n")
}

func out(text string) {
	writeLine(os.Stdout, text)
}

func fail(text string) int {
	writeLine(os.Stderr, "jsontool: "+text)
	return 1
}

func failFile(label string, path string) int {
	writeLine(os.Stderr, "jsontool: "+label+path)
	return 2
}

func typeName(value any) string {
	switch value.(type) {
	case nil:
		return "null"
	case bool:
		return "bool"
	case json.Number:
		return "number"
	case string:
		return "string"
	case []any:
		return "array"
	case map[string]any:
		return "object"
	default:
		return "unknown"
	}
}

func main() {
	args := os.Args[1:]

	for _, arg := range args {
		if len(arg) > 0 && arg[0] == '-' {
			os.Exit(fail("this tool has no options"))
		}
	}
	if len(args) < 2 {
		os.Exit(fail(usage))
	}

	command := args[0]
	path := args[1]

	info, statErr := os.Stat(path)
	if statErr != nil {
		os.Exit(failFile("input file does not exist: ", path))
	}
	if info.IsDir() {
		os.Exit(failFile("not a regular file: ", path))
	}

	raw, err := os.ReadFile(path)
	if err != nil {
		os.Exit(failFile("input file does not exist: ", path))
	}

	decoder := json.NewDecoder(bytes.NewReader(raw))
	decoder.UseNumber()
	var doc any
	if err := decoder.Decode(&doc); err != nil {
		os.Exit(invalidJSON())
	}
	var trailing any
	if err := decoder.Decode(&trailing); !errors.Is(err, io.EOF) {
		os.Exit(invalidJSON())
	}

	switch command {
	case "validate":
		if len(args) != 2 {
			os.Exit(fail("usage: jsontool validate <file>"))
		}
		out("ok")
	case "type":
		if len(args) != 2 {
			os.Exit(fail("usage: jsontool type <file>"))
		}
		out(typeName(doc))
	case "sum":
		if len(args) != 3 {
			os.Exit(fail("usage: jsontool sum <file> <field>"))
		}
		records, ok := doc.([]any)
		if !ok {
			writeLine(os.Stderr, "jsontool: document is not an array")
			os.Exit(4)
		}
		field := args[2]
		total := int64(0)
		for _, record := range records {
			object, ok := record.(map[string]any)
			if !ok {
				continue
			}
			number, ok := object[field].(json.Number)
			if !ok {
				continue
			}
			value, err := strconv.ParseInt(number.String(), 10, 64)
			if err != nil {
				continue
			}
			total += value
		}
		out(strconv.FormatInt(total, 10))
	case "compact":
		if len(args) != 2 {
			os.Exit(fail("usage: jsontool compact <file>"))
		}
		var compacted bytes.Buffer
		if err := json.Compact(&compacted, raw); err != nil {
			os.Exit(invalidJSON())
		}
		out(compacted.String())
	default:
		os.Exit(fail(usage))
	}
}

func invalidJSON() int {
	writeLine(os.Stderr, "jsontool: invalid JSON")
	return 3
}
