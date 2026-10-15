"""Execute the introductory lessons from their actual Markdown snippets."""
import json
from pathlib import Path
import re
import shlex
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]


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


def check(binary):
    with tempfile.TemporaryDirectory(prefix='fr-reader-examples-') as directory:
        root = Path(directory)
        change, trace = root / 'change', root / 'trace'
        change.mkdir()
        trace.mkdir()
        commands = first_change(binary, change)
        traces = first_trace(binary, trace)
    return {'first_change_commands': commands, 'complete_trace_examples': traces}


if __name__ == '__main__':
    import argparse

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--fr', required=True, type=Path)
    args = parser.parse_args()
    print(json.dumps(check(args.fr.resolve()), sort_keys=True))
