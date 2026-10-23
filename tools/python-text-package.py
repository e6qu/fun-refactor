import argparse
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import urllib.request
import re

ROOT = Path(__file__).resolve().parents[1]


def load(name):
    spec = importlib.util.spec_from_file_location(name.replace('-', '_'), ROOT / 'tools' / (name + '.py'))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


text = load('python-text-flow')
packages = load('package-questions')
replay = load('package_question_replay')
scalar = load('retain-scalar-evidence')
require, save, digest = text.require, text.save, text.digest
SELECTION = json.loads((text.FIXTURE / 'selection.json').read_text())
PROJECT = {'id': 'stringcase', 'repository': SELECTION['repository'],
    'revision': SELECTION['revision'], 'role': 'held-out', 'python_root': '.',
    'module': SELECTION['module'], 'function': SELECTION['function'],
    'expression': 'relay(VALUE)', 'overwrite': repr(SELECTION['replacement']),
    'values': SELECTION['inputs'], 'expected': SELECTION['expected'],
    'blocked': SELECTION['replacement_expected'], 'standard_library': ['builtins', 're']}


def bindings():
    names = ['tools/python-text-package.py', '.github/workflows/python-text-package.yml']
    return text.bindings() | replay.bindings() | {name: digest(ROOT / name) for name in names}


def freeze(coverage_run, directory):
    require(os.environ.get('GITHUB_ACTIONS') == 'true', 'freeze and inspect the package on GitHub')
    data, identity = scalar.collect(coverage_run, 'language-behavior', 'language-behavior', os.environ['GITHUB_REPOSITORY'])
    selected = {name.removeprefix('language-behavior/text/'): value for name, value in data.items()
                if name.startswith('language-behavior/text/')}
    with tempfile.TemporaryDirectory() as temporary:
        coverage = Path(temporary)
        for name, value in selected.items():
            path = coverage / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(value)
        text.audit(coverage)
    require(not subprocess.check_output(['git', 'status', '--porcelain', '--untracked-files=all'], cwd=ROOT),
            'commit the checker and procedure before opening the package')
    record = {'source_commit': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
        'source_bindings': bindings(), 'selection': SELECTION, 'coverage': identity,
        'coverage_result_sha256': packages.flow.sha(selected['result.json']),
        'implementation_and_tests_unread': True,
        'boundary': 'The original package implementation is first opened after writing this freeze. All prior projects are familiar.'}
    save(directory / 'freeze.json', record)
    return record


def audit(directory):
    result = json.loads((directory / 'result.json').read_text())
    frozen = json.loads((directory / 'freeze.json').read_text())
    require(result['schema'] == 'fr-python-text-package' and result['complete'] is True,
            'package question did not complete')
    require(frozen['selection'] == SELECTION and frozen['source_bindings'] == bindings()
            and frozen['implementation_and_tests_unread'] is True, 'frozen checker or question changed')
    require(result['freeze_sha256'] == digest(directory / 'freeze.json'), 'freeze record changed')
    require(result['source_commit'] == frozen['source_commit'], 'package source commit changed')
    require(result['benchmark'] is False and result['live_model_calls'] == 0, 'unsupported cost or model-use claim')
    packages.checked_questions([result['project']], {'stringcase': PROJECT}, True)
    original = directory / 'stringcase/upstream'
    archive = directory / 'stringcase/source.tar.gz'
    require(digest(archive) == result['project']['archive_sha256'], 'reserved package archive changed')
    with tempfile.TemporaryDirectory() as temporary:
        require(packages.flow.snapshot(archive.read_bytes(), PROJECT, Path(temporary))
                == result['project']['source_files'], 'original package inventory changed')
    for name, expected in result['project']['source_files'].items():
        require(digest(original / name) == expected, 'original package source changed')
    require(result['independent_projects'] == sorted([SELECTION['repository'], 'dateutil/dateutil',
        'mahmoud/boltons', 'phn/jdcal']), 'project inventory changed')
    replay.audit(directory / 'earlier-questions')
    earlier = json.loads((directory / 'earlier-questions/result.json').read_text())
    require(earlier['binary_sha256'] == result['binary_sha256'], 'projects used different checker executables')
    require(result['unread_project'] == SELECTION['repository'], 'unread project changed')
    require(set(result['files']) == {str(path.relative_to(directory)) for path in directory.rglob('*')
        if path.is_file() and path not in (directory / 'receipt.json', directory / 'result.json')},
        'package result file inventory changed')
    for name, expected in result['files'].items():
        path = directory / name
        require(path.resolve().is_relative_to(directory.resolve()) and digest(path) == expected,
                'retained package question file changed')
    return result


def collect(binary, directory, coverage_run):
    directory.mkdir(parents=True, exist_ok=False)
    directory, binary = directory.resolve(), binary.resolve()
    frozen = freeze(coverage_run, directory)
    result = {'schema': 'fr-python-text-package', 'complete': False, 'source_commit': frozen['source_commit'],
        'freeze_sha256': digest(directory / 'freeze.json'), 'binary_sha256': digest(binary),
        'benchmark': False, 'live_model_calls': 0, 'project': None,
        'claim': 'One fixed checker answers four original-package questions. Stringcase was reserved from its README; the other three projects are familiar maintenance checks.'}
    save(directory / 'result.json', result)
    try:
        work = directory / 'stringcase'
        work.mkdir()
        url = 'https://codeload.github.com/' + PROJECT['repository'] + '/tar.gz/' + PROJECT['revision']
        with urllib.request.urlopen(url, timeout=30) as response:
            data = response.read(packages.flow.COHORT['budgets']['archive_bytes'] + 1)
        (work / 'source.tar.gz').write_bytes(data)
        upstream = work / 'upstream'
        upstream.mkdir()
        files = packages.flow.snapshot(data, PROJECT, upstream)
        admission = packages.admission(upstream)
        require(any('MIT License' in row['text'] or 'Permission is hereby granted' in row['text']
                    for row in admission['licenses']), 'review the reserved package license')
        row = {'project': PROJECT, 'cases': [], 'error': None, 'complete': False,
            'source_files': files, 'archive_sha256': packages.flow.sha(data), 'admission': admission}
        result['project'] = row
        for variant in ('original', 'renamed-relocated'):
            for case in ('positive', 'overwritten', 'unknown', 'steps'):
                path, entry, adapter = packages.prepare(upstream, PROJECT, variant, case)
                name = variant + '-' + case
                cell = {'variant': variant, 'case': case, 'path': path, 'entry': entry,
                    'adapter': adapter, 'runtime': []}
                row['cases'].append(cell)
                request = {'mode': 'analysis', 'scope': str(upstream), 'path': path, 'entry': entry,
                    'project': PROJECT, 'case': case, 'fr': str(binary), 'report': str(work / (name + '.json'))}
                cell['analysis'] = packages.execute(work, name, request, upstream)
                save(directory / 'result.json', result)
                for index, value in enumerate(PROJECT['values']):
                    request = {'mode': 'runtime', 'scope': str(upstream), 'path': path, 'entry': entry, 'value': value}
                    observed = packages.execute(work, name + '-python-' + str(index), request, upstream)
                    expected = PROJECT['blocked'] if case == 'overwritten' else PROJECT['expected'][index]
                    cell['runtime'].append({'input': value, 'expected': expected, 'observation': observed})
                save(directory / 'result.json', result)
        packages.collect_cache_controls(upstream, PROJECT, work, binary, row, result, directory)
        row.update(complete=True, original_sources_unchanged=all(digest(upstream / name) == value
            for name, value in files.items()))
        packages.checked_questions([row], {'stringcase': PROJECT}, True)
        replay.collect(binary, directory / 'earlier-questions')
        require(bindings() == frozen['source_bindings'], 'checker or evaluator changed after freeze')
        result.update(complete=True, independent_projects=sorted([PROJECT['repository'], 'dateutil/dateutil',
            'mahmoud/boltons', 'phn/jdcal']), milestone_1_demonstrated=True,
            milestone_2_demonstrated=True, milestones_complete=True, unread_project=SELECTION['repository'])
    except Exception as error:
        result['error'] = type(error).__name__ + ': ' + str(error)
        raise
    finally:
        result['files'] = {str(path.relative_to(directory)): digest(path) for path in directory.rglob('*')
            if path.is_file() and path != directory / 'result.json'}
        save(directory / 'result.json', result)
    return audit(directory)


def retain(run, prefix):
    require(os.environ.get('GITHUB_ACTIONS') == 'true', 'retain package records on GitHub')
    require(os.environ['GITHUB_REF'].startswith('refs/heads/') and os.environ['GITHUB_REF'] != 'refs/heads/main',
            'retain package records into the working branch')
    require(re.fullmatch(r'[a-zA-Z0-9][a-zA-Z0-9_-]{0,79}', prefix), 'supply a plain dated prefix')
    data, identity = scalar.collect(run, 'python-text-package', 'question', os.environ['GITHUB_REPOSITORY'])
    result = json.loads(data['result.json'])
    require(result['source_commit'] == identity['source_commit']
            and set(data) == {'result.json', *result['files']}, 'package artifact source or inventory changed')
    relative = Path('tests/agent-eval/results') / (prefix + '-python-text-package')
    directory = ROOT / relative
    directory.mkdir()
    for name, value in data.items():
        path = directory / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(value)
    audit(directory)
    save(directory / 'receipt.json', {'collection': identity,
        'files': {name: packages.flow.sha(value) for name, value in data.items()}})
    catalog_path = ROOT / 'tests/agent-eval/roadmap.json'
    catalog = json.loads(catalog_path.read_text())
    entry, = [row for row in catalog['evidence'] if row['id'] == 'original-package-questions']
    entry.update(path=str(relative / 'result.json'), sha256=digest(directory / 'result.json'),
        scope='Four original-package questions with one unchanged checker. Stringcase was unread until freeze; Boltons, dateutil and JDcal are familiar maintenance checks. Includes independent Python outcomes, renamed callers and saved-answer controls.',
        audit='python3 tools/python-text-package.py audit --directory ' + str(relative))
    obligation, = [row for row in catalog['obligations'] if row['id'] == 'B.repository-flow']
    obligation['requires'] = [{'evidence': 'original-package-questions', 'assertions': [
        {'pointer': '/complete', 'equals': True},
        {'pointer': '/independent_projects', 'equals': result['independent_projects']},
        {'pointer': '/unread_project', 'equals': SELECTION['repository']}]}]
    obligation['next'] = 'Keep these four original-package questions passing. Full Toolz, Funcy and Pydash imports, exception handling and shared objects remain open.'
    save(catalog_path, catalog)
    subprocess.run(['python3', '-B', 'tools/roadmap-status.py', '--write'], cwd=ROOT, check=True)
    return result


def main():
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(dest='command', required=True)
    capture = sub.add_parser('collect')
    capture.add_argument('--binary', required=True, type=Path)
    capture.add_argument('--output', required=True, type=Path)
    capture.add_argument('--coverage-run', required=True, type=int)
    check = sub.add_parser('audit')
    check.add_argument('--directory', required=True, type=Path)
    keep = sub.add_parser('retain')
    keep.add_argument('--run', required=True, type=int)
    keep.add_argument('--prefix', required=True)
    args = parser.parse_args()
    if args.command == 'collect':
        result = collect(args.binary, args.output, args.coverage_run)
    elif args.command == 'audit':
        result = audit(args.directory)
    else:
        result = retain(args.run, args.prefix)
    print(json.dumps({'complete': result['complete'], 'projects': len(result['independent_projects'])}))


if __name__ == '__main__':
    main()
