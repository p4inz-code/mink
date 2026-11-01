// M4 W4 `poolrun` — Go port.
//
// Contract: `docs/benchmarks/M4/M4_SESSION_155.md`.
//
// Work is computed in goroutines; the mutex guards only the accumulator.
// Go goroutines are not OS threads — recorded as a fairness limitation.
package main

import (
	"fmt"
	"os"
	"strconv"
	"sync"
)

const usage = "usage: poolrun <squares|pipeline> <n> <workers>"

func out(text string) {
	_, _ = os.Stdout.WriteString(text + "\r\n")
}

func fail(label string) int {
	_, _ = os.Stderr.WriteString("poolrun: " + label + "\r\n")
	return 1
}

func ranges(n, workers int64) [][2]int64 {
	chunk := n / workers
	parts := make([][2]int64, 0, workers)
	for k := int64(0); k < workers; k++ {
		start := k*chunk + 1
		end := (k + 1) * chunk
		if k == workers-1 {
			end = n
		}
		parts = append(parts, [2]int64{start, end})
	}
	return parts
}

func sumSquares(start, end int64) int64 {
	total := int64(0)
	for value := start; value <= end; value++ {
		total += value * value
	}
	return total
}

func expectedSquares(n int64) int64 {
	return n * (n + 1) * (2*n + 1) / 6
}

func runSquares(n, workers int64) int {
	parts := ranges(n, workers)
	subtotals := make([]int64, workers)
	var mutex sync.Mutex
	var wait sync.WaitGroup
	total := int64(0)

	for k, part := range parts {
		wait.Add(1)
		go func(index int, start, end int64) {
			defer wait.Done()
			subtotal := sumSquares(start, end)
			subtotals[index] = subtotal
			mutex.Lock()
			total += subtotal
			mutex.Unlock()
		}(k, part[0], part[1])
	}

	out(fmt.Sprintf("workers=%d", workers))
	out(fmt.Sprintf("n=%d", n))
	wait.Wait()

	joined := int64(0)
	for index, subtotal := range subtotals {
		joined += subtotal
		out(fmt.Sprintf("subtotal[%d]=%d", index, subtotal))
	}
	mutex.Lock()
	lockedTotal := total
	mutex.Unlock()

	expected := expectedSquares(n)
	out(fmt.Sprintf("locked-total=%d", lockedTotal))
	out(fmt.Sprintf("joined-total=%d", joined))
	out(fmt.Sprintf("expected=%d", expected))
	if lockedTotal != expected || joined != expected {
		return 3
	}
	return 0
}

func runPipeline(n, workers int64) int {
	parts := ranges(n, workers)
	slots := make([]int64, workers)
	var wait sync.WaitGroup

	for k, part := range parts {
		wait.Add(1)
		go func(index int, start, end int64) {
			defer wait.Done()
			slots[index] = sumSquares(start, end)
		}(k, part[0], part[1])
	}

	out(fmt.Sprintf("workers=%d", workers))
	out(fmt.Sprintf("n=%d", n))
	wait.Wait() // join barrier: stage 1 completes before stage 2

	for index, value := range slots {
		out(fmt.Sprintf("stage1[%d]=%d", index, value))
	}

	reduced := make(chan int64, 1)
	go func() {
		total := int64(0)
		for _, value := range slots {
			total += value
		}
		reduced <- total
	}()
	total := <-reduced

	expected := expectedSquares(n)
	out(fmt.Sprintf("total=%d", total))
	out(fmt.Sprintf("expected=%d", expected))
	if total != expected {
		return 3
	}
	return 0
}

func main() {
	args := os.Args[1:]

	for _, arg := range args {
		if len(arg) > 0 && arg[0] == '-' {
			os.Exit(fail("this tool has no options"))
		}
	}
	if len(args) != 3 {
		os.Exit(fail(usage))
	}

	n, errN := strconv.ParseInt(args[1], 10, 64)
	workers, errW := strconv.ParseInt(args[2], 10, 64)
	if errN != nil || errW != nil || n <= 0 || workers <= 0 {
		os.Exit(fail(usage))
	}

	switch args[0] {
	case "squares":
		os.Exit(runSquares(n, workers))
	case "pipeline":
		os.Exit(runPipeline(n, workers))
	default:
		os.Exit(fail(usage))
	}
}
