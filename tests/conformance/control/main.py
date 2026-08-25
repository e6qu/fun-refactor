def classify(n: int) -> str:
    if n < 0:
        return "negative"
    elif n == 0:
        return "zero"
    elif n < 10:
        return "small"
    return "large"


def main() -> None:
    print(f"classify -5 {classify(-5)}")
    print(f"classify 0 {classify(0)}")
    print(f"classify 7 {classify(7)}")
    print(f"classify 40 {classify(40)}")
    i = 0
    while i < 6:
        i = i + 1
        if i % 2 == 0:
            continue
        if i == 5:
            break
        print(f"odd {i}")
    for value in [3, 1, 2]:
        print(f"item {value}")
    outer = 0
    while outer < 3:
        inner = 0
        while inner < 3:
            if inner == 2:
                break
            print(f"pair {outer} {inner}")
            inner = inner + 1
        outer = outer + 1
    print("done")


if __name__ == "__main__":
    main()
