def check(n: int) -> int:
    if n < 0:
        raise ValueError("negative")
    return n * 2


def double(n: int) -> int:
    return check(n) + 1


def main() -> None:
    try:
        v = check(5)
        print(f"checked 5 -> {v}")
    except ValueError as e:
        print(f"caught {e}")
    try:
        v = check(-3)
        print(f"never {v}")
    except ValueError as e:
        print(f"caught {e}")
    try:
        v = double(4)
        print(f"double 4 -> {v}")
    except ValueError as e:
        print(f"caught {e}")
    try:
        v = double(-2)
        print(f"never {v}")
    except ValueError as e:
        print(f"caught {e}")
    print("done")


if __name__ == "__main__":
    main()
