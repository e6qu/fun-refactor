package main

import "fmt"

func main() {
	fmt.Println("start")
	seen := map[string]bool{}
	seen["ada"] = true
	seen["alan"] = true
	seen["ada"] = true
	fmt.Println("size", len(seen))
	if seen["ada"] {
		fmt.Println("has-ada yes")
	} else {
		fmt.Println("has-ada no")
	}
	if seen["grace"] {
		fmt.Println("has-grace yes")
	} else {
		fmt.Println("has-grace no")
	}
	delete(seen, "alan")
	fmt.Println("after", len(seen))
	fmt.Println("done")
}
