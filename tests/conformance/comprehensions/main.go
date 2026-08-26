package main

import "fmt"

func main() {
	fmt.Println("start")
	nums := []int64{1, 2, 3, 4}
	doubled := []int64{}
	for _, n := range nums {
		doubled = append(doubled, n*2)
	}
	fmt.Println("first", doubled[0])
	var total int64 = 0
	for _, d := range doubled {
		total = total + d
	}
	fmt.Println("total", total)
	big := []int64{}
	for _, n := range nums {
		if n > 2 {
			big = append(big, n)
		}
	}
	var kept int64 = 0
	for _, b := range big {
		kept = kept + b
	}
	fmt.Println("kept", kept)
	fmt.Println("done")
}
