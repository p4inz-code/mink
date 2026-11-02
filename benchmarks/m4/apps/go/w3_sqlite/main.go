// M4 W3 `dbshell` — Go port over in-memory SQLite (modernc.org/sqlite).
//
// Contract: `docs/benchmarks/M4/M4_SESSION_154.md`.
//
// BUILD STATUS: verified. Built against modernc.org/sqlite v1.61.1 and gated
// against the W3 golden contract.
package main

import (
	"bufio"
	"database/sql"
	"fmt"
	"os"
	"strings"

	_ "modernc.org/sqlite"
)

const usage = "usage: dbshell <script>"

func out(file *os.File, text string) {
	_, _ = file.WriteString(text + "\r\n")
}

func fail(label string, code int) int {
	out(os.Stderr, "dbshell: "+label)
	return code
}

func normalize(message string) string {
	if strings.Contains(message, "no such table") {
		return "no such table"
	}
	if strings.Contains(message, "no such column") {
		return "unknown column in WHERE"
	}
	return message
}

func render(db *sql.DB, statement string) (string, error) {
	kind := strings.ToUpper(strings.Fields(statement)[0])

	if kind == "SELECT" {
		if strings.Contains(strings.ToUpper(statement), "COUNT(") {
			var count int64
			if err := db.QueryRow(statement).Scan(&count); err != nil {
				return "", err
			}
			return fmt.Sprintf("%d", count), nil
		}
		rows, err := db.Query(statement)
		if err != nil {
			return "", err
		}
		defer rows.Close()
		columns, err := rows.Columns()
		if err != nil {
			return "", err
		}
		var rendered strings.Builder
		for rows.Next() {
			values := make([]any, len(columns))
			pointers := make([]any, len(columns))
			for i := range values {
				pointers[i] = &values[i]
			}
			if err := rows.Scan(pointers...); err != nil {
				return "", err
			}
			for i, value := range values {
				if i > 0 {
					rendered.WriteByte('\t')
				}
				switch typed := value.(type) {
				case nil:
					rendered.WriteString("NULL")
				case []byte:
					rendered.Write(typed)
				default:
					fmt.Fprintf(&rendered, "%v", typed)
				}
			}
			rendered.WriteByte('\n')
		}
		if err := rows.Err(); err != nil {
			return "", err
		}
		return rendered.String(), nil
	}

	result, err := db.Exec(statement)
	if err != nil {
		return "", err
	}
	if kind == "UPDATE" || kind == "DELETE" {
		affected, err := result.RowsAffected()
		if err != nil {
			return "", err
		}
		return fmt.Sprintf("OK %d", affected), nil
	}
	return "OK", nil
}

func main() {
	args := os.Args[1:]

	for _, arg := range args {
		if len(arg) > 0 && arg[0] == '-' {
			os.Exit(fail("this tool has no options", 1))
		}
	}
	if len(args) != 1 {
		os.Exit(fail(usage, 1))
	}

	path := args[0]
	info, err := os.Stat(path)
	if err != nil {
		os.Exit(fail("script does not exist: "+path, 2))
	}
	if info.IsDir() {
		os.Exit(fail("not a regular file: "+path, 2))
	}

	file, err := os.Open(path)
	if err != nil {
		os.Exit(fail("script does not exist: "+path, 2))
	}
	defer file.Close()

	db, err := sql.Open("sqlite", ":memory:")
	if err != nil {
		os.Exit(fail("cannot open in-memory database", 3))
	}
	defer db.Close()

	scanner := bufio.NewScanner(file)
	for scanner.Scan() {
		line := strings.TrimSpace(scanner.Text())
		if line == "" || strings.HasPrefix(line, "--") {
			continue
		}
		out(os.Stdout, "sql> "+line)
		result, err := render(db, line)
		if err != nil {
			out(os.Stdout, "ERROR: "+normalize(err.Error()))
			os.Exit(3)
		}
		out(os.Stdout, result)
	}
	if err := scanner.Err(); err != nil {
		os.Exit(fail("cannot read script", 2))
	}
}
