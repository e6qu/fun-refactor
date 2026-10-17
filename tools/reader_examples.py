"""Execute the introductory lessons from their actual Markdown snippets."""
import gzip
import json
from pathlib import Path
import re
import shlex
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]


def check_measurements():
    source = ROOT / 'tests/agent-eval/results/2026-09-25-flow-cache/result.json.gz'
    with gzip.open(source, 'rt') as stream:
        report = json.load(stream)
    assert report['repetitions'] == 3
    labels = {
        'pipeline-8': 'Pipeline, 8 helpers', 'pipeline-16': 'Pipeline, 16 helpers',
        'fanout-8': 'Imported calls, 8 helpers', 'fanout-24': 'Imported calls, 24 helpers',
        'recursive-8': 'Recursive calls, 8 helpers', 'recursive-12': 'Recursive calls, 12 helpers',
    }
    assert set(report['summary']) == set(labels)
    page = (ROOT / 'docs/benchmarks.md').read_text()
    for name, summary in report['summary'].items():
        cold = summary['cold']['chunks']['elapsed_seconds']
        warm = summary['warm']['chunks']['elapsed_seconds']
        row = f'| {labels[name]} | {cold:.3f} | {warm:.3f} | {(warm / cold - 1) * 100:.1f}% |'
        assert row in page, f'Cache comparison must use the recorded medians: {row}'
    ratios = [value['warm']['chunks']['stored_bytes'] / value['warm']['legacy']['stored_bytes']
              for value in report['summary'].values()]
    assert f'{min(ratios):.2f}–{max(ratios):.2f} times' in page
    return len(labels)


def blocks(text, language):
    return re.findall(r'```' + language + r'\n(.*?)```', text, re.S)


def run(binary, folder, arguments):
    output_mode = [] if '--json' in arguments else ['--json']
    result = subprocess.run([str(binary), *output_mode, *arguments], cwd=folder,
                            capture_output=True, text=True, timeout=20)
    assert result.returncode == 0, (arguments, result.stdout, result.stderr)
    return json.loads(result.stdout)


def first_change(binary, folder):
    lesson = (ROOT / 'TUTORIAL.md').read_text().split('## Recorded walkthrough in Helm')[0]
    sources = blocks(lesson, 'python')
    assert len(sources) == 2, 'The first lesson must supply both complete files.'
    for name, source in zip(('app.py', 'test_app.py'), sources):
        (folder / name).write_text(source)
    original = {name: (folder / name).read_bytes() for name in ('app.py', 'test_app.py')}
    transaction, commands = None, 0
    for block in blocks(lesson, 'sh'):
        for line in block.splitlines():
            if not line.strip():
                continue
            command = shlex.split(line)
            commands += 1
            if command == ['python3', 'test_app.py']:
                subprocess.run([sys.executable, '-B', 'test_app.py'], cwd=folder,
                               capture_output=True, check=True, timeout=10)
                continue
            assert command[0] == 'fr', command
            arguments = [transaction if part == '<TX>' else part for part in command[1:]]
            assert all(isinstance(part, str) for part in arguments), 'A transaction must be saved before use.'
            result = run(binary, folder, arguments)
            current = {name: (folder / name).read_bytes() for name in original}
            if '--save-plan' in arguments:
                transaction = str(result['transaction'])
                assert current == original, 'Saving the example must leave source unchanged.'
            if '--write' in arguments:
                if 'undo' in arguments:
                    assert current == original, 'Undo must restore both documented files.'
                else:
                    assert all(b'increment' in value and b'calculate' not in value for value in current.values())
    assert transaction is not None and (folder / 'rename.patch').stat().st_size > 0
    return commands


def first_trace(binary, folder):
    lesson = (ROOT / 'docs/python-contract.md').read_text().split('## Read the answer')[0]
    (folder / 'app.py').write_text(blocks(lesson, 'python')[0])
    (folder / 'rules.json').write_text(blocks(lesson, 'json')[0])
    for name, expected in [('selected', True), ('blocked', False)]:
        found = run(binary, folder, ['-C', '.', 'project', 'find', name])
        assert len(found['rows']) == 1 and found['page']['remaining'] == 0
        target = dict(zip(found['columns'], found['rows'][0]))['handle']
        command = shlex.split(blocks(lesson, 'sh')[0].splitlines()[1])
        arguments = [target if part == '<HANDLE>' else part for part in command[1:]]
        result = run(binary, folder, arguments)
        assert result['complete'] is True, (name, result.get('cutoffs'))
        assert bool(result['witnesses']) is expected, name
    return 2


def registry_trace(binary, folder):
    page = (ROOT / 'docs/python-contract.md').read_text()
    lesson = page.split('## Prepare a callback table without calling its entries\n', 1)[1].split('\n## ', 1)[0]
    source, = blocks(lesson, 'python')
    helper, app = source.removeprefix('# helpers.py\n').split('# app.py\n', 1)
    (folder / 'app.py').write_text(app)
    (folder / 'rules.json').write_text(blocks(page, 'json')[0])
    variants = [
        (helper, True, True),
        (helper.replace('\n    return value\n', '\n    return 0\n'), True, False),
        (helper.replace('lambda value: unknown_operation(value)', 'read_input()'), False, True),
    ]
    assert len({source for source, _, _ in variants}) == 3
    for source, complete, reaches in variants:
        (folder / 'helpers.py').write_text(source)
        found = run(binary, folder, ['-C', '.', 'project', 'find', 'entry'])
        assert len(found['rows']) == 1 and found['page']['remaining'] == 0
        target = dict(zip(found['columns'], found['rows'][0]))['handle']
        result = run(binary, folder, ['-C', '.', 'project', 'dataflow', target,
                                     '--summaries', '--imports', '--rules', 'rules.json'])
        assert result['complete'] is complete, result.get('cutoffs')
        assert bool(result['witnesses']) is reaches
    return len(variants)


def import_order_trace(binary, folder):
    page = (ROOT / 'docs/python-contract.md').read_text()
    lesson = page.split('## Follow imports that return to a file already loading\n', 1)[1].split('\n## ', 1)[0]
    source, = blocks(lesson, 'python')
    helper, rest = source.removeprefix('# helpers.py\n').split('# neighbor.py\n', 1)
    neighbor, app = rest.split('# app.py\n', 1)
    (folder / 'neighbor.py').write_text(neighbor)
    (folder / 'app.py').write_text(app)
    (folder / 'rules.json').write_text(blocks(page, 'json')[0])
    import_line = 'from neighbor import ready\n'
    variants = [
        (helper, True, 17),
        (helper.replace('return value', 'return 0'), True, 0),
        (import_line + helper.replace(import_line, ''), False, None),
    ]
    assert len({source for source, _, _ in variants}) == 3
    script = ('import json\ntry:\n import app\n app.read_input=lambda:17\n app.send=lambda value:value\n'
              ' print(json.dumps({"returned":app.entry()}))\n'
              'except Exception as error:\n print(json.dumps({"raised":type(error).__name__}))\n')
    for source, complete, returned in variants:
        (folder / 'helpers.py').write_text(source)
        observed = json.loads(subprocess.check_output([sys.executable, '-B', '-c', script], cwd=folder,
                                                     text=True, timeout=10))
        assert observed == ({'returned': returned} if complete else {'raised': 'ImportError'})
        found = run(binary, folder, ['-C', '.', 'project', 'find', 'entry'])
        assert len(found['rows']) == 1 and found['page']['remaining'] == 0
        target = dict(zip(found['columns'], found['rows'][0]))['handle']
        result = run(binary, folder, ['-C', '.', 'project', 'dataflow', target,
                                     '--summaries', '--imports', '--rules', 'rules.json'])
        assert result['complete'] is complete, result.get('cutoffs')
        if complete:
            assert bool(result['witnesses']) is (returned == 17)
        else:
            assert 'cyclic-module-initialization' in result['cutoffs']
    return len(variants)


def startup_disclosure(binary, folder):
    page = (ROOT / 'docs/python-contract.md').read_text()
    lesson = page.split('## Find the statement that prevents an answer\n', 1)[1].split('\n## ', 1)[0]
    helper, = blocks(lesson, 'python')
    helper = helper.removeprefix('# helpers.py\n')
    (folder / 'app.py').write_text('from helpers import relay\ndef entry():\n    return send(relay(0))\n')
    (folder / 'rules.json').write_text(blocks(page, 'json')[0])
    variants = [(helper, 1), (helper.replace('read_input()', '0'), 0),
                ('read_input()\n' * 20 + helper.replace('read_input()', '0'), 20)]
    for source, count in variants:
        (folder / 'helpers.py').write_text(source)
        found = run(binary, folder, ['-C', '.', 'project', 'find', 'entry'])
        target = dict(zip(found['columns'], found['rows'][0]))['handle']
        result = run(binary, folder, ['-C', '.', 'project', 'dataflow', target,
                                     '--summaries', '--imports', '--rules', 'rules.json'])
        assert result['complete'] is (count == 0), result.get('cutoffs')
        disclosure = result['initialization']
        assert len(disclosure['sites']) + disclosure['omitted'] == count
        assert disclosure['by_file'] == ({'helpers.py': {
            'statements': count, 'reasons': {'startup-call': count}}} if count else {})
    return len(variants)


def function_default_trace(binary, folder):
    page = (ROOT / 'docs/python-contract.md').read_text()
    lesson = page.split('## Store a function as a default without calling it\n', 1)[1].split('\n## ', 1)[0]
    source, = blocks(lesson, 'python')
    helper, app = source.removeprefix('# helpers.py\n').split('# app.py\n', 1)
    (folder / 'app.py').write_text(app)
    (folder / 'rules.json').write_text(blocks(page, 'json')[0])
    variants = [(helper, True, True), (helper.replace('return value', 'return 0'), True, False),
                (helper.replace('follow=selected', 'follow=selected(0)'), False, True)]
    script = ('import json\ntry:\n import app\n app.read_input=lambda:17\n app.send=lambda value:value\n'
              ' print(json.dumps({"returned":app.entry()}))\n'
              'except Exception as error:\n print(json.dumps({"raised":type(error).__name__}))\n')
    for source, complete, reaches in variants:
        (folder / 'helpers.py').write_text(source)
        observed = json.loads(subprocess.check_output([sys.executable, '-B', '-c', script], cwd=folder,
                                                     text=True, timeout=10))
        assert observed == ({'returned': 17 if reaches else 0} if complete else {'raised': 'NameError'})
        found = run(binary, folder, ['-C', '.', 'project', 'find', 'entry'])
        target = dict(zip(found['columns'], found['rows'][0]))['handle']
        result = run(binary, folder, ['-C', '.', 'project', 'dataflow', target,
                                     '--summaries', '--imports', '--rules', 'rules.json'])
        assert result['complete'] is complete, result.get('cutoffs')
        assert bool(result['witnesses']) is reaches
        if not complete:
            assert any('definition-time-code' in item['reasons'] for item in result['initialization']['sites'])
    return len(variants)


def parallel_binding_trace(binary, folder):
    page = (ROOT / 'docs/python-contract.md').read_text()
    lesson = page.split('## Save several helper names in one assignment\n', 1)[1].split('\n## ', 1)[0]
    source, = blocks(lesson, 'python')
    helper, app = source.removeprefix('# helpers.py\n').split('# app.py\n', 1)
    (folder / 'app.py').write_text(app)
    (folder / 'rules.json').write_text(blocks(lesson, 'json')[0])
    variants = [(helper, True, 17), (helper.replace('= relay, discard', '= discard, relay'), True, 0),
                (helper.replace('= relay, discard', '= (relay,)'), False, None)]
    script = ('import json\ntry:\n import app\n app.read_input=lambda:17\n app.send=lambda value:value\n'
              ' print(json.dumps({"returned":app.entry()}))\n'
              'except Exception as error:\n print(json.dumps({"raised":type(error).__name__}))\n')
    for source, complete, returned in variants:
        (folder / 'helpers.py').write_text(source)
        observed = json.loads(subprocess.check_output([sys.executable, '-B', '-c', script], cwd=folder,
                                                     text=True, timeout=10))
        assert observed == ({'returned': returned} if complete else {'raised': 'ValueError'})
        found = run(binary, folder, ['-C', '.', 'project', 'find', 'entry'])
        target = dict(zip(found['columns'], found['rows'][0]))['handle']
        result = run(binary, folder, ['-C', '.', 'project', 'dataflow', target,
                                     '--summaries', '--imports', '--rules', 'rules.json'])
        assert result['complete'] is complete, result.get('cutoffs')
        if complete:
            assert bool(result['witnesses']) is (returned == 17)
        else:
            assert result['initialization']['sites']
    return len(variants)


def imported_default_trace(binary, folder):
    page = (ROOT / 'docs/python-contract.md').read_text()
    lesson = page.split('## Share a default value between files\n', 1)[1].split('\n## ', 1)[0]
    source, = blocks(lesson, 'python')
    settings, rest = source.removeprefix('# settings.py\n').split('# helpers.py\n', 1)
    helper, app = rest.split('# app.py\n', 1)
    explicit = app.replace('choose()', 'choose(read_input())')
    late = helper.replace('from settings import LIMIT\n', '') + '\nfrom settings import LIMIT\n'
    variants = [(settings, helper, app, True, {'returned': 0}),
                (settings, helper, explicit, True, {'returned': 17}),
                (settings.replace('LIMIT = 0', 'LIMIT = []'), helper, app, False, {'returned': []}),
                (settings.replace('LIMIT = 0', 'LIMIT = []'), helper, explicit, True, {'returned': 17}),
                (settings, late, app, False, {'raised': 'NameError'})]
    return default_trace_variants(binary, folder, lesson, variants)


def value_alias_trace(binary, folder):
    page = (ROOT / 'docs/python-contract.md').read_text()
    lesson = page.split('## Give a shared setting another name\n', 1)[1].split('\n## ', 1)[0]
    source, = blocks(lesson, 'python')
    settings, rest = source.removeprefix('# settings.py\n').split('# helpers.py\n', 1)
    helper, app = rest.split('# app.py\n', 1)
    explicit = app.replace('choose()', 'choose(read_input())')
    late = settings.replace('BASE = 0\nLIMIT = BASE', 'LIMIT = BASE\nBASE = 0')
    variants = [(settings, helper, app, True, {'returned': 0}),
                (settings, helper, explicit, True, {'returned': 17}),
                (settings.replace('BASE = 0', 'BASE = []'), helper, app, False, {'returned': []}),
                (settings.replace('BASE = 0', 'BASE = []'), helper, explicit, True, {'returned': 17}),
                (late, helper, app, False, {'raised': 'NameError'}),
                (settings, helper.replace('return value', 'return FALLBACK'), app, False, {'returned': 0})]
    return default_trace_variants(binary, folder, lesson, variants)


def default_trace_variants(binary, folder, lesson, variants):
    (folder / 'rules.json').write_text(blocks(lesson, 'json')[0])
    script = ('import json\ntry:\n import app\n app.read_input=lambda:17\n app.send=lambda value:value\n'
              ' print(json.dumps({"returned":app.entry()}))\n'
              'except Exception as error:\n print(json.dumps({"raised":type(error).__name__}))\n')
    for constants, functions, entry, complete, expected in variants:
        for name, text in [('settings.py', constants), ('helpers.py', functions), ('app.py', entry)]:
            (folder / name).write_text(text)
        observed = json.loads(subprocess.check_output([sys.executable, '-B', '-c', script], cwd=folder,
                                                     text=True, timeout=10))
        assert observed == expected
        found = run(binary, folder, ['-C', '.', 'project', 'find', 'entry'])
        target = dict(zip(found['columns'], found['rows'][0]))['handle']
        result = run(binary, folder, ['-C', '.', 'project', 'dataflow', target,
                                     '--summaries', '--imports', '--rules', 'rules.json'])
        assert result['complete'] is complete, result.get('cutoffs')
        if complete:
            assert bool(result['witnesses']) is (expected['returned'] == 17)
        else:
            assert result['cutoffs']
    return len(variants)


def check(binary):
    measurements = check_measurements()
    with tempfile.TemporaryDirectory(prefix='fr-reader-examples-') as directory:
        root = Path(directory)
        change, trace, registry, order, startup = (root / name for name in ('change', 'trace', 'registry', 'order', 'startup'))
        change.mkdir()
        trace.mkdir()
        registry.mkdir()
        order.mkdir()
        startup.mkdir()
        commands = first_change(binary, change)
        traces = first_trace(binary, trace)
        setups = registry_trace(binary, registry)
        cycles = import_order_trace(binary, order)
        disclosures = startup_disclosure(binary, startup)
        defaults = root / 'defaults'
        defaults.mkdir()
        captures = function_default_trace(binary, defaults)
        parallel = root / 'parallel'
        parallel.mkdir()
        bindings = parallel_binding_trace(binary, parallel)
        shared = root / 'shared'
        shared.mkdir()
        imported = imported_default_trace(binary, shared)
        aliases = root / 'value-aliases'
        aliases.mkdir()
        values = value_alias_trace(binary, aliases)
    return {'first_change_commands': commands, 'complete_trace_examples': traces,
            'registry_setup_examples': setups,
            'import_order_examples': cycles,
            'startup_diagnostic_examples': disclosures,
            'function_default_examples': captures,
            'parallel_binding_examples': bindings,
            'imported_default_examples': imported,
            'value_alias_examples': values,
            'verified_measurement_rows': measurements}


if __name__ == '__main__':
    import argparse

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--fr', required=True, type=Path)
    args = parser.parse_args()
    print(json.dumps(check(args.fr.resolve()), sort_keys=True))
