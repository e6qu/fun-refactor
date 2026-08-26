package main

import "fmt"

func main() {
	fmt.Println("start")
	ages := map[string]int64{"ada": 36, "alan": 41}
	ages["grace"] = 45
	fmt.Println("size", len(ages))
	fmt.Println("ada", ages["ada"])
	var total int64 = 0
	for _, name := range []string{"ada", "alan", "grace"} {
		total = total + ages[name]
	}
	fmt.Println("total", total)
	fmt.Println("done")
}
