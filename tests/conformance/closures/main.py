def apply_to(f, n: int) -> int:
    return f(n)


def twice(f, n: int) -> int:
    return f(f(n))


def main() -> None:
    print("start")
    add_one = lambda n: n + 1
    print(f"apply {apply_to(add_one, 6)}")
    print(f"twice {twice(add_one, 10)}")
    print("done")


if __name__ == "__main__":
    main()
