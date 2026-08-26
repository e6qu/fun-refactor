def main() -> None:
    print("start")
    ages = {"ada": 36, "alan": 41}
    ages["grace"] = 45
    print(f"size {len(ages)}")
    print(f"ada {ages['ada']}")
    total = 0
    for name in ["ada", "alan", "grace"]:
        total = total + ages[name]
    print(f"total {total}")
    print("done")


if __name__ == "__main__":
    main()
