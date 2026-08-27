package main

import "fmt"

func main() {
	fmt.Println("start")
	seen := map[string]struct{}{}
	seen["ada"] = struct{}{}
	seen["alan"] = struct{}{}
	seen["ada"] = struct{}{}
	fmt.Println("size", len(seen))
	if _, ok := seen["ada"]; ok {
		fmt.Println("has-ada yes")
	} else {
		fmt.Println("has-ada no")
	}
	if _, ok := seen["grace"]; ok {
		fmt.Println("has-grace yes")
	} else {
		fmt.Println("has-grace no")
	}
	delete(seen, "alan")
	fmt.Println("after", len(seen))
	fmt.Println("done")
}
