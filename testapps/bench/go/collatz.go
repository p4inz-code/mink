package main

import "fmt"

func main() {
	best, bestStart := 0, 0
	for start := 1; start < 100000; start++ {
		n, steps := start, 0
		for n != 1 {
			if n%2 == 0 {
				n = n / 2
			} else {
				n = 3*n + 1
			}
			steps++
		}
		if steps > best {
			best, bestStart = steps, start
		}
	}
	fmt.Println(bestStart)
	fmt.Println(best)
}
