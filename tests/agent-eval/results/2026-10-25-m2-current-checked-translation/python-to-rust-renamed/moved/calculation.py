def total_pages(items: int, size: int) -> int:
    if items < 0 or size <= 0:
        return -1
    quotient = items // size
    if items % size == 0:
        return quotient
    return quotient + 1
