def main() -> None:
    print("start")
    seen = set()
    seen.add("ada")
    seen.add("alan")
    seen.add("ada")
    print(f"size {len(seen)}")
    if "ada" in seen:
        print("has-ada yes")
    else:
        print("has-ada no")
    if "grace" in seen:
        print("has-grace yes")
    else:
        print("has-grace no")
    seen.remove("alan")
    print(f"after {len(seen)}")
    print("done")


if __name__ == "__main__":
    main()
