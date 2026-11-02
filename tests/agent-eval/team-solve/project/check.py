"""Ordinary Python integers do not overflow; this is not the Rust M3 task."""
from counter import pages

for items, size, expected in [(11, 5, 3), (10, 5, 2), (1, 5, 1), (0, 5, 0),
                              (-1, 5, -1), (1, 0, -1), (1, -1, -1), (2**63 - 1, 2, 2**62)]:
    actual = pages(items, size)
    assert actual == expected, (items, size, expected, actual)
print('All eight page-count examples passed.')
