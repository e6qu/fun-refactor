import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

from evidence_basis import native_source_bindings

ROOT = Path(__file__).resolve().parents[1]
FIXTURE = ROOT / 'tests/agent-eval/python-text-flow'


def require(condition, message):
    if not condition:
        raise ValueError(message)


def save(path, value):
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + '\n')


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def bindings():
    names = ['tools/python-text-flow.py', 'tools/test-python-text-flow.py', 'tools/evidence_basis.py',
             'tests/language_behavior.rs', 'sdk/python/tests/test_text_flow.py']
    paths = [ROOT / name for name in names] + list(FIXTURE.iterdir())
    paths += list((ROOT / 'sdk/python/src').rglob('*.py'))
    require(all(path.is_file() for path in paths), 'required text-flow input missing')
    return native_source_bindings(ROOT) | {str(path.relative_to(ROOT)): digest(path)
        for path in paths}


def program(case, renamed):
    text = case['prelude'] + 'def entry():\n'
    text += ''.join('    ' + line + '\n' for line in case['body'].splitlines())
    return text.replace('entry', 'customer_entry').replace('read_user', 'load_input').replace('send', 'emit') if renamed else text


def run(argv, workspace, timeout, commands):
    try:
        result = subprocess.run(list(map(str, argv)), cwd=workspace, capture_output=True, timeout=timeout)
    except subprocess.TimeoutExpired as error:
        commands.append({'argv': list(map(str, argv)), 'stop_reason': 'wall_seconds',
            'stdout': (error.stdout or b'').decode(errors='replace'), 'stderr': (error.stderr or b'').decode(errors='replace')})
        raise
    commands.append({'argv': list(map(str, argv)), 'exit_code': result.returncode,
        'stdout': result.stdout.decode(errors='replace'), 'stderr': result.stderr.decode(errors='replace')})
    require(len(result.stdout) + len(result.stderr) <= 1048576, 'command output exceeds 1 MiB')
    require(result.returncode == 0, 'command failed: ' + str(argv[0]) + '\n' + result.stderr.decode(errors='replace')[-8192:])
    return json.loads(result.stdout)


def runtime(case, source, renamed, workspace, commands):
    source_name, sink_name = ('load_input', 'emit') if renamed else ('read_user', 'send')
    entry = 'customer_entry' if renamed else 'entry'
    driver = ('import json, runpy\nsent = []\n' +
        'def read(): return ' + repr(case['input']) + '\n' +
        'def send(value): sent.append(value); return value\n' +
        f'namespace = runpy.run_path({str(source)!r}, init_globals={{{source_name!r}: read, {sink_name!r}: send}})\n' +
        'returned = None\nraised = None\ntry:\n' +
        f'    returned = namespace[{entry!r}]()\n' +
        'except Exception as error:\n    raised = type(error).__name__\n' +
        "print(json.dumps({'sent': sent, 'returned': returned, 'raised': raised}))\n")
    return run([sys.executable, '-I', '-B', '-c', driver], workspace, 10, commands)


def validate(case, analysis, observed):
    require(observed == {name: case[name] for name in ('sent', 'returned', 'raised')},
            'Python disagrees with authored expectations: ' + case['id'])
    require(analysis['complete'] is case['complete'], 'unexpected model completion: ' + case['id'])
    if case['witness'] is not None:
        require(bool(analysis['witnesses']) is case['witness'], 'unexpected value connection: ' + case['id'])
    else:
        require(analysis['complete'] is False, 'an unchecked call cannot establish no path')
    require(analysis['runtime_effects']['exception_safety_established'] is False,
            'text operations establish no runtime exception-safety claim')


def audit(directory):
    report = json.loads((directory / 'result.json').read_text())
    cases = json.loads((FIXTURE / 'cases.json').read_text())
    require(report['schema'] == 'fr-python-text-flow' and report['passed'] is True,
            'text-flow collection did not pass')
    require(report['source_bindings'] == bindings(), 'text-flow inputs changed')
    require(report['benchmark'] is False and report['live_model_calls'] == 0,
            'text-flow checks do not measure speed or use live models')
    require(len(report['cases']) == len(cases) * 2, 'text-flow cases missing')
    require({(row['id'], row['renamed']) for row in report['cases']} == {
        (case['id'], renamed) for case in cases for renamed in (False, True)}, 'text-flow case identities changed')
    for row in report['cases']:
        case, = [case for case in cases if case['id'] == row['id']]
        for name, expected in row['files'].items():
            path = Path(name)
            require(not path.is_absolute() and '..' not in path.parts and '\\' not in name
                    and (directory / path).resolve().is_relative_to(directory.resolve())
                    and digest(directory / path) == expected,
                    'text-flow source or results changed')
        require(type(row['renamed']) is bool and row['directory'] == case['id'] + ('-renamed' if row['renamed'] else '-original'),
                'unsafe or mismatched text-flow directory')
        work = directory / row['directory']
        relative = 'workspace/moved/customer.py' if row['renamed'] else 'workspace/subject.py'
        require(set(row['files']) == {str(Path(row['directory']) / name) for name in (
            relative, 'workspace/rules.json', 'analysis.json', 'runtime.json', 'commands.json')}, 'text-flow file inventory changed')
        require((work / relative).read_text() == program(case, row['renamed']), 'text-flow program changed')
        validate(case, json.loads((work / 'analysis.json').read_text()), json.loads((work / 'runtime.json').read_text()))
        commands = json.loads((work / 'commands.json').read_text())
        require(len(commands) == 3 and all(command['exit_code'] == 0 for command in commands), 'text-flow commands did not pass')
    return report


def collect(binary, directory):
    require(os.environ.get('GITHUB_ACTIONS') == 'true', 'collect text-flow results on GitHub')
    require(sys.implementation.name == 'cpython' and sys.version_info[:3] == (3, 14, 8), 'use the declared CPython 3.14.8 runtime')
    directory.mkdir(parents=True, exist_ok=False)
    directory, binary = directory.resolve(), binary.resolve()
    cases = json.loads((FIXTURE / 'cases.json').read_text())
    report = {'schema': 'fr-python-text-flow', 'passed': False, 'source_bindings': bindings(),
        'source_commit': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
        'cases': [], 'live_model_calls': 0, 'benchmark': False, 'python': sys.version,
        'binary_sha256': digest(binary)}
    active, commands = None, []
    try:
        for case in cases:
            for renamed in (False, True):
                work = directory / (case['id'] + ('-renamed' if renamed else '-original'))
                workspace = work / 'workspace'
                active, commands = work, []
                relative = 'moved/customer.py' if renamed else 'subject.py'
                source = workspace / relative
                source.parent.mkdir(parents=True)
                source.write_text(program(case, renamed))
                rules = {'version': 'text-flow', 'sources': ['load_input' if renamed else 'read_user'],
                    'sinks': ['emit' if renamed else 'send'], 'sanitizers': {}, 'propagators': []}
                save(workspace / 'rules.json', rules)
                prefix = [binary, '--json', '--no-cache', '-C', workspace]
                found = run([*prefix, 'project', 'find', 'customer_entry' if renamed else 'entry', '--in', relative], workspace, 30, commands)
                require(len(found['rows']) == 1, 'select exactly one entry')
                models = case['standard_library']
                arguments = [*prefix, 'project', 'dataflow', found['rows'][0][0], '--summaries', '--imports',
                    '--rules', workspace / 'rules.json', '--context', 'html', '--steps', '4096', '--depth', '8', '--bytes', '262144']
                if models:
                    arguments += ['--standard-library', ','.join(models)]
                analysis = run(arguments, workspace, 30, commands)
                observed = runtime(case, source, renamed, workspace, commands)
                save(work / 'analysis.json', analysis)
                save(work / 'runtime.json', observed)
                save(work / 'commands.json', commands)
                row = {'id': case['id'], 'renamed': renamed, 'directory': str(work.relative_to(directory)),
                    'files': {str(path.relative_to(directory)): digest(path) for path in (
                        source, workspace / 'rules.json', work / 'analysis.json', work / 'runtime.json', work / 'commands.json')}}
                report['cases'].append(row)
                save(directory / 'result.json', report)
                validate(case, analysis, observed)
        report['passed'] = True
    except Exception as error:
        report['error'] = type(error).__name__ + ': ' + str(error)
        raise
    finally:
        if active is not None:
            save(active / 'commands.json', commands)
        save(directory / 'result.json', report)
    return audit(directory)


def main():
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(dest='command', required=True)
    capture = sub.add_parser('collect')
    capture.add_argument('--binary', type=Path, required=True)
    capture.add_argument('--output', type=Path, required=True)
    check = sub.add_parser('audit')
    check.add_argument('--directory', type=Path, required=True)
    args = parser.parse_args()
    result = collect(args.binary, args.output) if args.command == 'collect' else audit(args.directory)
    print(json.dumps({'passed': result['passed'], 'cases': len(result['cases'])}))


if __name__ == '__main__':
    main()
