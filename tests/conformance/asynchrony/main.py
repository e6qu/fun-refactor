import asyncio


async def load(name: str, base: int) -> int:
    print(f"fetch {name}")
    return base + 1


async def total(a: int, b: int) -> int:
    first = await load("a", a)
    second = await load("b", b)
    return first + second


async def main() -> None:
    print("start")
    result = await total(10, 20)
    print(f"total {result}")
    print("done")


if __name__ == "__main__":
    asyncio.run(main())
