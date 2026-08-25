package main

import "fmt"

func load(name string, base int) int {
	fmt.Printf("fetch %s\n", name)
	return base + 1
}

func total(a int, b int) int {
	first := load("a", a)
	second := load("b", b)
	return first + second
}

func main() {
	fmt.Println("start")
	result := total(10, 20)
	fmt.Printf("total %d\n", result)
	fmt.Println("done")
}
