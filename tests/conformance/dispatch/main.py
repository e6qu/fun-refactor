def day_name(day: int) -> str:
    match day:
        case 1:
            return "mon"
        case 2:
            return "tue"
        case 3:
            return "wed"
        case _:
            return "other"


def op_kind(word: str) -> str:
    match word:
        case "add":
            return "plus"
        case "sub":
            return "minus"
        case _:
            return "other"


def main() -> None:
    print(f"day 1 {day_name(1)}")
    print(f"day 3 {day_name(3)}")
    print(f"day 9 {day_name(9)}")
    print(f"kind add {op_kind('add')}")
    print(f"kind sub {op_kind('sub')}")
    print(f"kind mul {op_kind('mul')}")
    print("done")


if __name__ == "__main__":
    main()
