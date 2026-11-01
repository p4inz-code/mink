// M4 W2 `netcheck` — Go port.
//
// Contract: `docs/benchmarks/M4/M4_SESSION_153.md`.
//
//	netcheck <host> <port> <path>
//
// Transport failures and completed responses without a status line share
// exit 2 and one stable message. A 2xx body must be a JSON object with an
// integral `value`.
package main

import (
	"bytes"
	"encoding/json"
	"io"
	"net/http"
	"os"
	"strconv"
	"time"
)

const usage = "usage: netcheck <host> <port> <path>"

func writeLine(file *os.File, text string) {
	_, _ = file.WriteString(text + "\r\n")
}

func fail(label string, code int) int {
	writeLine(os.Stderr, "netcheck: "+label)
	return code
}

// integralValue reports (parsed, value). parsed is false when the body is not
// JSON; value is nil when it is valid JSON without an integral `value`.
func integralValue(body []byte) (bool, *int64) {
	decoder := json.NewDecoder(bytes.NewReader(body))
	decoder.UseNumber()
	var document any
	if err := decoder.Decode(&document); err != nil {
		return false, nil
	}
	var trailing any
	if err := decoder.Decode(&trailing); err != io.EOF {
		return false, nil
	}
	object, ok := document.(map[string]any)
	if !ok {
		return true, nil
	}
	number, ok := object["value"].(json.Number)
	if !ok {
		return true, nil
	}
	value, err := number.Int64()
	if err != nil {
		return true, nil
	}
	return true, &value
}

func main() {
	args := os.Args[1:]

	for _, arg := range args {
		if len(arg) > 0 && arg[0] == '-' {
			os.Exit(fail("this tool has no options", 1))
		}
	}
	if len(args) != 3 {
		os.Exit(fail(usage, 1))
	}
	port, err := strconv.Atoi(args[1])
	if err != nil || port < 1 || port > 65535 {
		os.Exit(fail(usage, 1))
	}

	url := "http://" + args[0] + ":" + strconv.Itoa(port) + args[2]
	client := &http.Client{Timeout: 10 * time.Second}
	response, err := client.Get(url)
	if err != nil {
		os.Exit(fail("connection failed", 2))
	}
	body, err := io.ReadAll(response.Body)
	_ = response.Body.Close()
	if err != nil {
		os.Exit(fail("connection failed", 2))
	}

	parsed, value := integralValue(body)
	if response.StatusCode < 400 {
		if !parsed {
			os.Exit(fail("malformed JSON", 3))
		}
		if value == nil {
			os.Exit(fail("missing value", 3))
		}
	}

	writeLine(os.Stdout, "STATUS "+strconv.Itoa(response.StatusCode))
	writeLine(os.Stdout, "BODY "+strconv.Itoa(len(body)))
	if value != nil {
		writeLine(os.Stdout, "VALUE "+strconv.FormatInt(*value, 10))
	}
}
