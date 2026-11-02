def pages(items, size):
    """Return -1 for invalid inputs, otherwise the number of pages needed."""
    if items < 0 or size <= 0:
        return -1
    return items // size
