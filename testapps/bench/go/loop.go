package main

import "fmt"

func main() {
	s := 0
	for i := 0; i < 30000000; i++ {
		s = s + i*i%7
	}
	fmt.Println(s)
}
