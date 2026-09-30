"""Independent public behavior checks for pinned Python repository tasks."""
from __future__ import annotations

import argparse
import ast
import json
from pathlib import Path
import sys


def singularization():
    from boltons.strutils import singularize
    pairs = [(word, word) for word in ('glass', 'boss', 'class', 'kiss', 'address', 'dress', 'mass', 'press')]
    pairs += [('glasses', 'glass'), ('bosses', 'boss'), ('classes', 'class'), ('kisses', 'kiss'),
              ('addresses', 'address'), ('dresses', 'dress'), ('feet', 'foot'), ('children', 'child'),
              ('activities', 'activity'), ('chances', 'chance'), ('cats', 'cat'), ('', '')]
    failures, count = [], 0
    for original, expected in pairs:
        for case in (str.lower, str.upper, str.title):
            text, wanted = case(original), case(expected)
            actual = singularize(text)
            count += 1
            if actual != wanted: failures.append([text, wanted, actual])
            if expected.endswith('ss'):
                count += 1
                repeated = singularize(wanted)
                if repeated != wanted: failures.append([wanted, wanted, repeated])
    return count, failures


def tail_counts():
    from more_itertools import take
    failures, count = [], 0
    def expect(label, operation, wanted):
        nonlocal count
        count += 1
        try: actual = operation()
        except Exception as error: actual = type(error).__name__
        if actual != wanted: failures.append([label, wanted, actual])
    for length in range(10):
        values = list(range(length))
        for amount in range(-12, 13):
            expected = values[amount:] if amount < 0 else values[:amount]
            expect(f'{length}:{amount}', lambda: take(amount, iter(values)), expected)
    class Index:
        def __index__(self): return -3
    expect('index protocol', lambda: take(Index(), range(8)), [5, 6, 7])
    expect('all', lambda: take(None, range(4)), [0, 1, 2, 3])
    for bad in (1.5, -1.5, '2', object()):
        expect('nonintegral', lambda: take(bad, range(3)), 'TypeError')
    for amount in (0, 2, -2):
        seen = []
        def values():
            for value in range(5):
                seen.append(value)
                yield value
        try: take(amount, values())
        except Exception: pass  # Baseline failures are retained by the value checks.
        expect(f'consumption:{amount}', lambda: seen, [] if amount == 0 else [0, 1] if amount > 0 else list(range(5)))
    expect('boolean count', lambda: take(True, range(3)), [0])
    import tracemalloc
    tracemalloc.start()
    try:
        expect('large finite stream', lambda: take(-3, range(100_000)), [99_997, 99_998, 99_999])
        _, peak = tracemalloc.get_traced_memory()
    finally:
        tracemalloc.stop()
    expect('bounded tail storage', lambda: peak < 262_144, True)
    return count, failures


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('task', choices=('singularize-idempotency', 'take-tail-counts'))
    parser.add_argument('root', type=Path)
    parser.add_argument('--syntax', action='store_true')
    args = parser.parse_args()
    sys.path.insert(0, str(args.root.resolve()))
    if args.syntax:
        paths = [p for p in args.root.rglob('*.py') if not any(x.startswith('.') for x in p.relative_to(args.root).parts)]
        for path in paths: ast.parse(path.read_bytes(), filename=str(path))
        result = {'passed': True, 'files': len(paths)}
    else:
        try:
            count, failures = singularization() if args.task == 'singularize-idempotency' else tail_counts()
            result = {'passed': not failures, 'cases': count, 'failures': failures}
        except Exception as error:
            result = {'passed': False, 'error': type(error).__name__, 'message': str(error)}
    print(json.dumps(result, sort_keys=True))
    raise SystemExit(0 if result['passed'] else 1)


if __name__ == '__main__':
    main()
