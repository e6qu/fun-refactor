def first_of(items: list[int]) -> int:
    return items[0]


def count_of(items: list[str]) -> int:
    return len(items)


class Box:
    def __init__(self, value: int) -> None:
        self.value = value

    def get(self) -> int:
        return self.value


def main() -> None:
    print("start")
    numbers = [4, 5, 6]
    words = ["a", "b"]
    print(f"first {first_of(numbers)}")
    print(f"count {count_of(words)}")
    b = Box(9)
    print(f"box {b.get()}")
    print("done")


if __name__ == "__main__":
    main()
