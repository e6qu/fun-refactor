"""Execute independent behavior observations without using fr as the oracle."""
import argparse
import importlib
import json
from pathlib import Path
import re
import sys


CONVERSION_CASES = (
    ((None,), {}), ((None,), {'default': -7}), (('',), {}), (('0',), {}),
    (('-17',), {}), (('  +42  ',), {}), (('bad',), {'default': 3.25}),
    ((17,), {}), ((False,), {}), ((3.75,), {}),
    (('a12b34',), {'regexp': True}), (('a12b34',), {'regexp': r'(\d+)\D(\d+)'}),
    (('a12b34',), {'regexp': r'\d+'}), (('no digits',), {'regexp': True, 'default': 29}),
    (('-7',), {'regexp': True}), (('1e3',), {'regexp': True}),
    (('abc',), {'regexp': 123}), (('abc',), {'exception': 'ValueError'}),
)
SLUG_CASES = (
    ('Hello World', {}), ('A B C', {'separator': '::'}),
    ('日本 語 café', {'allow_unicode': True}), ('ab 🌱 cd', {'allow_unicode': True}),
    ('a b c', {'separator': '🌱'}), ('a b c', {'separator': ''}),
    ('alpha beta gamma', {'max_length': 10, 'word_boundary': True}),
    ('alpha beta gamma', {'max_length': 9, 'word_boundary': True, 'save_order': True}),
    ('A B C', {'lowercase': False}), ('one the two', {'stopwords': ['the']}),
    ('&#x65E5; &#x672C;', {'allow_unicode': True}),
    ('A B', {'replacements': [('a', '🌱')], 'replacement_stage': 'post'}),
    ('a-b-c', {'regex_pattern': r'[^a-z-]+', 'separator': '__'}),
    ('a\u0301 ब', {'allow_unicode': True}), ('', {}),
)


def observe(function, *args, **kwargs):
    try:
        value = function(*args, **kwargs)
        return {'value': value, 'type': type(value).__name__}
    except Exception as error:
        return {'error': type(error).__name__}


def conversion(project, name):
    package = importlib.import_module('python_utils')
    module = importlib.import_module('python_utils.converters')
    assert Path(module.__file__).resolve().is_relative_to(project.resolve())
    function = getattr(module, name)
    exported = getattr(package, name)
    assert function is exported and name in package.__all__ and name in dir(package)
    results = []
    for args, keywords in CONVERSION_CASES:
        keywords = dict(keywords)
        if keywords.get('exception') == 'ValueError':
            keywords['exception'] = ValueError
        results.append(observe(function, *args, **keywords))
    results.append(observe(function, 'a12b', regexp=re.compile(r'(\d+)')))
    results.append(observe(function, 'a12b', regexp=re.compile(r'\d+')))
    return results


def slug_baseline(project):
    module = importlib.import_module('slugify')
    assert Path(module.__file__).resolve().is_relative_to(project.resolve())
    return {mode: [observe(module.slugify, text, algorithm=mode, **options)
                   for text, options in SLUG_CASES] for mode in ('legacy', 'modern')}


def byte_prefix(text, limit):
    encoded = text.encode('utf-8')
    # The independent oracle cuts encoded bytes and discards only an incomplete
    # trailing code point. The public checker instead counts characters' sizes.
    return encoded[:limit].decode('utf-8', errors='ignore')


def slug_check(project, baseline):
    module = importlib.import_module('slugify')
    assert Path(module.__file__).resolve().is_relative_to(project.resolve())
    function = module.slugify
    assert slug_baseline(project) == baseline, 'existing behavior changed'
    for mode in ('legacy', 'modern'):
        for (text, options), expected in zip(SLUG_CASES, baseline[mode]):
            assert observe(function, text, algorithm=mode, max_bytes=0, **options) == expected
    for (text, options), expected in zip(SLUG_CASES, baseline['modern']):
        assert set(expected) == {'value', 'type'} and expected['type'] == 'str'
        complete = expected['value']
        for limit in range(1, len(complete.encode('utf-8')) + 3):
            result = function(text, algorithm='modern', max_bytes=limit, **options)
            assert type(result) is str and result == byte_prefix(complete, limit)
            assert len(result.encode('utf-8')) <= limit
    for mode in ('legacy', 'modern'):
        for value in (None, True, False, 1.5, '3', [], {}):
            assert observe(function, 'abc', algorithm=mode, max_bytes=value) == {'error': 'TypeError'}
        assert observe(function, 'abc', algorithm=mode, max_bytes=-1) == {'error': 'ValueError'}
    for value in (1, 3, 100):
        assert observe(function, 'abc', algorithm='legacy', max_bytes=value) == {'error': 'ValueError'}
        assert observe(function, 'abc', max_bytes=value) == {'error': 'ValueError'}
    for text in ('', 'hello world', 'A &amp; B'):
        assert function(text) == function(text, max_bytes=0)
    return {'passed': True, 'baseline_cases': len(SLUG_CASES) * 2}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('operation', choices=('conversion', 'slug-baseline', 'slug-check'))
    parser.add_argument('project', type=Path)
    parser.add_argument('--name')
    parser.add_argument('--baseline', type=Path)
    args = parser.parse_args()
    sys.path.insert(0, str(args.project.resolve()))
    if args.operation == 'conversion':
        value = conversion(args.project, args.name)
    elif args.operation == 'slug-baseline':
        value = slug_baseline(args.project)
    else:
        value = slug_check(args.project, json.loads(args.baseline.read_text()))
    print(json.dumps(value, ensure_ascii=False, sort_keys=True, allow_nan=False))


if __name__ == '__main__':
    main()
