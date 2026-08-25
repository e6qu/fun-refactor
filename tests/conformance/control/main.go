package main

import "fmt"

func classify(n int) string {
	if n < 0 {
		return "negative"
	} else if n == 0 {
		return "zero"
	} else if n < 10 {
		return "small"
	}
	return "large"
}

func main() {
	fmt.Printf("classify -5 %s\n", classify(-5))
	fmt.Printf("classify 0 %s\n", classify(0))
	fmt.Printf("classify 7 %s\n", classify(7))
	fmt.Printf("classify 40 %s\n", classify(40))
	i := 0
	for i < 6 {
		i = i + 1
		if i%2 == 0 {
			continue
		}
		if i == 5 {
			break
		}
		fmt.Printf("odd %d\n", i)
	}
	for _, value := range []int{3, 1, 2} {
		fmt.Printf("item %d\n", value)
	}
	outer := 0
	for outer < 3 {
		inner := 0
		for inner < 3 {
			if inner == 2 {
				break
			}
			fmt.Printf("pair %d %d\n", outer, inner)
			inner = inner + 1
		}
		outer = outer + 1
	}
	fmt.Println("done")
}
