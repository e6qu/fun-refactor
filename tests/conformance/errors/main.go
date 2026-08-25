package main

import (
	"errors"
	"fmt"
)

func check(n int) (int, error) {
	if n < 0 {
		return 0, errors.New("negative")
	}
	return n * 2, nil
}

func double(n int) (int, error) {
	v, err := check(n)
	if err != nil {
		return 0, err
	}
	return v + 1, nil
}

func main() {
	if v, err := check(5); err != nil {
		fmt.Printf("caught %s\n", err.Error())
	} else {
		fmt.Printf("checked 5 -> %d\n", v)
	}
	if v, err := check(-3); err != nil {
		fmt.Printf("caught %s\n", err.Error())
	} else {
		fmt.Printf("never %d\n", v)
	}
	if v, err := double(4); err != nil {
		fmt.Printf("caught %s\n", err.Error())
	} else {
		fmt.Printf("double 4 -> %d\n", v)
	}
	if v, err := double(-2); err != nil {
		fmt.Printf("caught %s\n", err.Error())
	} else {
		fmt.Printf("never %d\n", v)
	}
	fmt.Println("done")
}
