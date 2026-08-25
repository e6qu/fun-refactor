def main() -> None:
    print("start")
    n = 42
    total = n + 10
    print(f"n {n}")
    print(f"sum {total}")
    total = total * 2
    print(f"twice {total}")
    q = 10 // 3
    r = 10 % 3
    print(f"q {q} r {r}")
    label = f"item-{7}"
    print(f"label {label}")
    i = 0
    while i < 3:
        print(f"tick {i}")
        i = i + 1
    print("done")

if __name__ == "__main__":
    main()
