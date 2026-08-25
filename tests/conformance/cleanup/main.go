package main

import "fmt"

func work() {
	fmt.Println("open a")
	defer fmt.Println("close a")
	fmt.Println("open b")
	defer fmt.Println("close b")
	fmt.Println("work")
}

func main() {
	work()
	fmt.Println("done")
}
