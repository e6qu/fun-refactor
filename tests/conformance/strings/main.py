def main() -> None:
    word = "Hello"
    print(f"upper {word.upper()}")
    print(f"lower {word.lower()}")
    print(f"len {len(word)}")
    joined = word + "-" + "World"
    print(f"concat {joined}")
    if "ell" in word:
        print("has yes")
    if "xyz" in word:
        print("never")
    else:
        print("has no")
    print("done")


if __name__ == "__main__":
    main()
