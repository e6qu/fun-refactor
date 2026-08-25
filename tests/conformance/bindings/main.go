package main

import "fmt"

func main() {
	fmt.Println("start")
	n := 42
	total := n + 10
	fmt.Printf("n %d\n", n)
	fmt.Printf("sum %d\n", total)
	total = total * 2
	fmt.Printf("twice %d\n", total)
	q := 10 / 3
	r := 10 % 3
	fmt.Printf("q %d r %d\n", q, r)
	label := fmt.Sprintf("item-%d", 7)
	fmt.Printf("label %s\n", label)
	i := 0
	for i < 3 {
		fmt.Printf("tick %d\n", i)
		i = i + 1
	}
	fmt.Println("done")
}
