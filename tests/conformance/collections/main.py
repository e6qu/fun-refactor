def main() -> None:
    nums = []
    nums.append(3)
    nums.append(1)
    nums.append(2)
    print(f"len {len(nums)}")
    print(f"first {nums[0]}")
    nums[1] = 10
    total = 0
    for value in nums:
        total = total + value
    print(f"sum {total}")
    for value in nums:
        print(f"item {value}")
    print("done")


if __name__ == "__main__":
    main()
