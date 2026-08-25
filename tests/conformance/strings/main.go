package main

import (
	"fmt"
	"strings"
)

func main() {
	word := "Hello"
	fmt.Printf("upper %s\n", strings.ToUpper(word))
	fmt.Printf("lower %s\n", strings.ToLower(word))
	fmt.Printf("len %d\n", len(word))
	joined := word + "-" + "World"
	fmt.Printf("concat %s\n", joined)
	if strings.Contains(word, "ell") {
		fmt.Println("has yes")
	}
	if strings.Contains(word, "xyz") {
		fmt.Println("never")
	} else {
		fmt.Println("has no")
	}
	fmt.Println("done")
}
