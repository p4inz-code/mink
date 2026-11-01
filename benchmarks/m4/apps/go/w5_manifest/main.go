// M4 W5 `manifest` — Go port.
//
// Contract: `docs/benchmarks/M4/M4_SESSION_156.md`.
//
// JSON config + recursive walk + SHA-256 + CSV report. Paths are relative to
// the config root with `/` separators and sorted in byte order.
package main

import (
	"bytes"
	"crypto/sha256"
	"encoding/csv"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io/fs"
	"os"
	"path/filepath"
	"sort"
	"strings"
)

const usage = "usage: manifest <config.json>"

type config struct {
	root       string
	out        string
	extensions []string
}

func out(text string) {
	_, _ = os.Stdout.WriteString(text + "\r\n")
}

func fail(label string, code int) int {
	_, _ = os.Stderr.WriteString("manifest: " + label + "\r\n")
	return code
}

func failFile(label, path string, code int) int {
	_, _ = os.Stderr.WriteString("manifest: " + label + path + "\r\n")
	return code
}

func parseConfig(raw []byte) (config, string, bool) {
	var document any
	if err := json.Unmarshal(raw, &document); err != nil {
		return config{}, "config is not valid JSON", false
	}
	object, ok := document.(map[string]any)
	if !ok {
		return config{}, "config must be a JSON object", false
	}
	rootValue, hasRoot := object["root"]
	outValue, hasOut := object["out"]
	extValue, hasExt := object["extensions"]
	if !hasRoot || !hasOut || !hasExt {
		return config{}, "config needs root, out and extensions", false
	}
	root, okRoot := rootValue.(string)
	outPath, okOut := outValue.(string)
	rawExtensions, okExt := extValue.([]any)
	if !okRoot || !okOut || !okExt {
		return config{}, "config types must be string, string, array", false
	}
	extensions := make([]string, 0, len(rawExtensions))
	for _, item := range rawExtensions {
		text, ok := item.(string)
		if !ok {
			return config{}, "config types must be string, string, array", false
		}
		extensions = append(extensions, text)
	}
	return config{root: root, out: outPath, extensions: extensions}, "", true
}

func selected(rel string, extensions []string) bool {
	ext := filepath.Ext(rel)
	if ext == "" {
		return false
	}
	for _, wanted := range extensions {
		if strings.EqualFold(ext, wanted) {
			return true
		}
	}
	return false
}

func csvField(value string) string {
	if strings.ContainsAny(value, ",\"\r\n") {
		return "\"" + strings.ReplaceAll(value, "\"", "\"\"") + "\""
	}
	return value
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
		os.Exit(failFile("config does not exist: ", path, 2))
	}
	if info.IsDir() {
		os.Exit(failFile("not a regular file: ", path, 2))
	}

	raw, err := os.ReadFile(path)
	if err != nil {
		os.Exit(failFile("config does not exist: ", path, 2))
	}
	cfg, message, ok := parseConfig(raw)
	if !ok {
		os.Exit(fail(message, 3))
	}

	rootInfo, err := os.Stat(cfg.root)
	if err != nil || !rootInfo.IsDir() {
		os.Exit(failFile("root does not exist: ", cfg.root, 4))
	}

	var files []string
	walkErr := filepath.WalkDir(cfg.root, func(full string, entry fs.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if entry.IsDir() {
			return nil
		}
		rel, err := filepath.Rel(cfg.root, full)
		if err != nil {
			return err
		}
		rel = filepath.ToSlash(rel)
		if selected(rel, cfg.extensions) {
			files = append(files, rel)
		}
		return nil
	})
	if walkErr != nil {
		os.Exit(failFile("root does not exist: ", cfg.root, 4))
	}
	sort.Strings(files)
	if len(files) == 0 {
		os.Exit(fail("no file matched the extensions", 5))
	}

	var buffer bytes.Buffer
	writer := csv.NewWriter(&buffer)
	_ = writer.Write([]string{"path", "size", "sha256", "ext"})
	totalBytes := int64(0)
	for _, rel := range files {
		data, err := os.ReadFile(filepath.Join(cfg.root, filepath.FromSlash(rel)))
		if err != nil {
			os.Exit(failFile("cannot read file: ", rel, 4))
		}
		totalBytes += int64(len(data))
		digest := sha256.Sum256(data)
		_ = writer.Write([]string{
			rel,
			fmt.Sprintf("%d", len(data)),
			hex.EncodeToString(digest[:])[:16],
			filepath.Ext(rel),
		})
	}
	writer.Flush()
	if err := writer.Error(); err != nil {
		os.Exit(failFile("cannot write report: ", cfg.out, 4))
	}
	if err := os.WriteFile(cfg.out, buffer.Bytes(), 0644); err != nil {
		os.Exit(failFile("cannot write report: ", cfg.out, 4))
	}

	out(fmt.Sprintf("files=%d", len(files)))
	out(fmt.Sprintf("bytes=%d", totalBytes))
	out("first=" + files[0])
}
