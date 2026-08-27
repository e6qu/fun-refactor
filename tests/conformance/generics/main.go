package main

import "fmt"

type Box struct {
	Value int
}

func (b Box) Get() int {
	return b.Value
}

func firstOf(items []int) int {
	return items[0]
}

func countOf(items []string) int {
	return len(items)
}

func main() {
	fmt.Println("start")
	numbers := []int{4, 5, 6}
	words := []string{"a", "b"}
	fmt.Println("first", firstOf(numbers))
	fmt.Println("count", countOf(words))
	b := Box{Value: 9}
	fmt.Println("box", b.Get())
	fmt.Println("done")
}
