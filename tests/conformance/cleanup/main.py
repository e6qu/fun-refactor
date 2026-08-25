def work() -> None:
    print("open a")
    try:
        print("open b")
        try:
            print("work")
        finally:
            print("close b")
    finally:
        print("close a")


def main() -> None:
    work()
    print("done")


if __name__ == "__main__":
    main()
