package main

import "fmt"

func dayName(day int) string {
	switch day {
	case 1:
		return "mon"
	case 2:
		return "tue"
	case 3:
		return "wed"
	default:
		return "other"
	}
}

func opKind(word string) string {
	switch word {
	case "add":
		return "plus"
	case "sub":
		return "minus"
	default:
		return "other"
	}
}

func main() {
	fmt.Printf("day 1 %s\n", dayName(1))
	fmt.Printf("day 3 %s\n", dayName(3))
	fmt.Printf("day 9 %s\n", dayName(9))
	fmt.Printf("kind add %s\n", opKind("add"))
	fmt.Printf("kind sub %s\n", opKind("sub"))
	fmt.Printf("kind mul %s\n", opKind("mul"))
	fmt.Println("done")
}
