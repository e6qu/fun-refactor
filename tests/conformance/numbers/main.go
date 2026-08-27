package main

import "fmt"

func floorDiv(a int, b int) int {
	quotient := a / b
	if a%b != 0 && (a < 0) != (b < 0) {
		return quotient - 1
	}
	return quotient
}

func floorMod(a int, b int) int {
	return a - floorDiv(a, b)*b
}

func main() {
	fmt.Println("start")
	a := 7
	b := 2
	fmt.Println("sum", a+b)
	fmt.Println("diff", a-b)
	fmt.Println("product", a*b)
	fmt.Println("quotient", floorDiv(a, b))
	fmt.Println("remainder", floorMod(a, b))
	negative := -7
	fmt.Println("negquotient", floorDiv(negative, b))
	fmt.Println("negremainder", floorMod(negative, b))
	fmt.Println("done")
}
