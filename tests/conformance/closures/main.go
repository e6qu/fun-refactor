package main

import "fmt"

func applyTo(f func(int) int, n int) int {
	return f(n)
}

func twice(f func(int) int, n int) int {
	return f(f(n))
}

func main() {
	fmt.Println("start")
	addOne := func(n int) int { return n + 1 }
	fmt.Println("apply", applyTo(addOne, 6))
	fmt.Println("twice", twice(addOne, 10))
	fmt.Println("done")
}
