"""Small public checks for product tasks; hidden completeness checks stay outside projects."""
import importlib
import json
from pathlib import Path
import sys


def prefix(text, limit):
    result, used = [], 0
    for char in text:
        size = len(char.encode('utf-8'))
        if used + size > limit:
            break
        result.append(char)
        used += size
    return ''.join(result)


def rename_check():
    package = importlib.import_module('python_utils')
    converters = importlib.import_module('python_utils.converters')
    assert package.parse_int is converters.parse_int
    assert not hasattr(package, 'to_int') and not hasattr(converters, 'to_int')
    assert 'parse_int' in package.__all__ and 'to_int' not in package.__all__
    for args, kwargs, expected in [
        (('42',), {}, 42), (('bad',), {'default': 17}, 17),
        (('x12y34',), {'regexp': r'(\d+)'}, 12),
        (('x12y34',), {'regexp': r'(\d+)\D(\d+)'}, 34),
        (('x12',), {'regexp': r'\d+'}, 12), ((None,), {'default': -4}, -4),
    ]:
        assert package.parse_int(*args, **kwargs) == expected
    assert importlib.import_module('unrelated').to_int('42') == 'unrelated:42'


def graph_check():
    """Check the submission shape, not expose its required graph as a public answer."""
    answer = json.loads(Path('analysis.json').read_text())
    assert set(answer) == {'definitions', 'calls', 'runtime_dispatch_complete'}
    assert answer['runtime_dispatch_complete'] is False
    assert isinstance(answer['definitions'], list) and isinstance(answer['calls'], list)
    for row in answer['definitions']:
        assert set(row) == {'name', 'line'} and isinstance(row['name'], str)
        assert type(row['line']) is int and row['line'] > 0
    for row in answer['calls']:
        assert set(row) == {'caller', 'callee', 'line', 'column', 'source_target'}
        assert all(isinstance(row[name], str) and row[name] for name in ('caller', 'callee'))
        assert type(row['line']) is int and row['line'] > 0
        assert type(row['column']) is int and row['column'] >= 0
        target = row['source_target']
        if target is not None:
            assert set(target) == {'path', 'name', 'line'}
            assert all(isinstance(target[name], str) and target[name] for name in ('path', 'name'))
            assert type(target['line']) is int and target['line'] > 0
    assert answer['calls'] == sorted(answer['calls'], key=lambda row: (row['caller'], row['line'], row['column']))
    print('Schema checked. Independent grading checks completeness and source targets.')


def bytes_check():
    slugify = importlib.import_module('slugify').slugify
    for text, options in [('alpha beta', {}), ('日本 語 café', {'allow_unicode': True}),
                          ('a b c', {'separator': '🌱'}),
                          ('alpha beta gamma', {'max_length': 10, 'word_boundary': True})]:
        complete = slugify(text, algorithm='modern', **options)
        assert slugify(text, algorithm='modern', max_bytes=0, **options) == complete
        for limit in range(1, 16):
            assert slugify(text, algorithm='modern', max_bytes=limit, **options) == prefix(complete, limit)
    for value, exception in [(True, TypeError), (False, TypeError), (1.5, TypeError),
                             ('4', TypeError), (-1, ValueError)]:
        try:
            slugify('hello', algorithm='modern', max_bytes=value)
        except exception:
            pass
        else:
            raise AssertionError('invalid byte limit was accepted')
    assert slugify('Hello World') == slugify('Hello World', max_bytes=0)
    try:
        slugify('hello', max_bytes=1)
    except ValueError:
        pass
    else:
        raise AssertionError('legacy accepted a nonzero byte limit')


def main():
    sys.path.insert(0, str(Path.cwd()))
    checks = {'utils-rename': rename_check, 'sqlparse-graph': graph_check, 'slugify-bytes': bytes_check}
    checks[sys.argv[1]]()
    print('Public task checks passed.')


if __name__ == '__main__':
    main()
