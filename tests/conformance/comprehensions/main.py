def main() -> None:
    print("start")
    nums = [1, 2, 3, 4]
    doubled = [n * 2 for n in nums]
    print(f"first {doubled[0]}")
    total = 0
    for d in doubled:
        total = total + d
    print(f"total {total}")
    big = [n for n in nums if n > 2]
    kept = 0
    for b in big:
        kept = kept + b
    print(f"kept {kept}")
    print("done")


if __name__ == "__main__":
    main()
