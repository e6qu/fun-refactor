package main

import "fmt"

func main() {
	nums := []int{}
	nums = append(nums, 3)
	nums = append(nums, 1)
	nums = append(nums, 2)
	fmt.Printf("len %d\n", len(nums))
	fmt.Printf("first %d\n", nums[0])
	nums[1] = 10
	total := 0
	for _, value := range nums {
		total = total + value
	}
	fmt.Printf("sum %d\n", total)
	for _, value := range nums {
		fmt.Printf("item %d\n", value)
	}
	fmt.Println("done")
}
