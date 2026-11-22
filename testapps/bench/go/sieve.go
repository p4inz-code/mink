package main

import "fmt"

func main() {
	n := 2000000
	flags := make([]int, n+1)
	for i := range flags {
		flags[i] = 1
	}
	for p := 2; p*p <= n; p++ {
		if flags[p] == 1 {
			for m := p * p; m <= n; m += p {
				flags[m] = 0
			}
		}
	}
	count := 0
	for i := 2; i <= n; i++ {
		if flags[i] == 1 {
			count++
		}
	}
	fmt.Println(count)
}
