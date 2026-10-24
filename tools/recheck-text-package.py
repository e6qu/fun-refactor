"""Repeat the four familiar package questions; preserve the first unread result separately."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]


def load(name):
    spec = importlib.util.spec_from_file_location(name.replace('-', '_'), ROOT / 'tools' / (name + '.py'))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


original = load('python-text-package')
replay = load('package_question_replay')
packages = replay.packages
require, save, digest = original.require, original.save, original.digest
SELECTION = original.SELECTION
PROJECT = {'id': 'stringcase', 'repository': SELECTION['repository'], 'revision': SELECTION['revision'],
    'role': 'held-out', 'python_root': '.', 'module': SELECTION['module'], 'function': SELECTION['function'],
    'expression': 'relay(VALUE)', 'overwrite': repr(SELECTION['replacement']),
    'values': SELECTION['inputs'], 'expected': SELECTION['expected'], 'blocked': SELECTION['replacement_expected'],
    'standard_library': ['builtins', 're']}
RECORD = ROOT / original.HISTORICAL_DIRECTORY


def bindings():
    names = ['tools/recheck-text-package.py', 'tools/test-recheck-text-package.py', 'tools/python-text-package.py',
        '.github/workflows/recheck-text-package.yml', original.SELECTION_PATH]
    names += [original.HISTORICAL_DIRECTORY + '/' + name for name in ('result.json', 'receipt.json', 'artifact.zip', 'delivery.json')]
    return replay.bindings() | {name: digest(ROOT / name) for name in names}


def validate_claims(result):
    require(result['schema'] == 'fr-rechecked-text-package' and result['complete'] is True,
        'four-package maintenance replay did not pass')
    require(result['all_projects_familiar'] is True and result['current_checker_was_unread'] is False,
        'a maintenance replay cannot claim another unread evaluation')
    require(result['original_auditor'] == original.HISTORICAL_AUDITOR and
        result['original_source_commit'] == original.FIRST_SOURCE and result['original_audit_passed'] is True,
        'original package provenance changed')
    require(result['original_unread'] == {'project': SELECTION['repository'], 'qualified': True,
        'first_evaluation_run': original.FIRST_RUN}, 'original unread qualification changed')
    require(result['independent_projects'] == sorted([SELECTION['repository'], 'dateutil/dateutil', 'mahmoud/boltons', 'phn/jdcal']),
        'four-package inventory changed')
    require(all(result[name] is True for name in ('milestone_1_demonstrated', 'milestone_2_demonstrated', 'milestones_complete')),
        'package questions are unfinished')
    require(result['benchmark'] is False and result['live_model_calls'] == 0, 'unsupported cost or model-use claim')


def audit(directory):
    result = json.loads((directory / 'result.json').read_text())
    validate_claims(result)
    require(result['source_bindings'] == bindings(), 'current package checker or procedure changed')
    require(result['limits'] == SELECTION['limits'], 'Stringcase limits changed')
    packages.checked_questions([result['project']], {'stringcase': PROJECT}, True)
    work = directory / 'stringcase'
    reports = [work / (cell['variant'] + '-' + cell['case'] + '.json') for cell in result['project']['cases']]
    reports += [work / (stage + '-' + kind + '.json') for stage, _, _ in packages.CACHE_STAGES for kind in ('saved', 'fresh')]
    for path in reports:
        value = json.loads(path.read_text())
        limits = SELECTION['limits']
        require(value['budget']['response_bytes'] == limits['report_bytes'] and value['budget']['call_depth'] == limits['depth']
            and value['budget']['steps'] == (1 if path.stem.endswith('-steps') else limits['steps'])
            and value['inputs']['modules']['limits']['modules'] == limits['modules']
            and value['inputs']['modules']['limits']['source_bytes'] == limits['source_bytes'], 'package analysis limits changed')
    archive = work / 'source.tar.gz'
    require(digest(archive) == result['project']['archive_sha256'], 'Stringcase archive changed')
    with tempfile.TemporaryDirectory() as temporary:
        require(packages.flow.snapshot(archive.read_bytes(), PROJECT, Path(temporary)) == result['project']['source_files'],
            'Stringcase source inventory changed')
    for name, expected in result['project']['source_files'].items():
        require(digest(work / 'upstream' / name) == expected, 'original Stringcase source changed')
    replay.audit(directory / 'earlier-questions')
    earlier = json.loads((directory / 'earlier-questions/result.json').read_text())
    require(earlier['binary_sha256'] == result['binary_sha256'], 'projects used different checker executables')
    require(set(result['files']) == {str(path.relative_to(directory)) for path in directory.rglob('*')
        if path.is_file() and path.name != 'receipt.json' and path != directory / 'result.json'}, 'package replay inventory changed')
    for name, expected in result['files'].items():
        path = directory / name
        require(not Path(name).is_absolute() and path.resolve().is_relative_to(directory.resolve())
            and digest(path) == expected, 'package replay file changed')
    original.historical_audit(RECORD, original.HISTORICAL_AUDITOR)
    return result


def collect(binary, directory):
    require(os.environ.get('GITHUB_ACTIONS') == 'true', 'run complete package checks on GitHub')
    directory.mkdir(parents=True, exist_ok=False)
    directory, binary = directory.resolve(), binary.resolve()
    original.historical_audit(RECORD, original.HISTORICAL_AUDITOR)
    receipt = json.loads((RECORD / 'receipt.json').read_text())
    data = original.scalar.files((RECORD / 'artifact.zip').read_bytes(), receipt['artifact_digest'])['stringcase/source.tar.gz']
    result = {'schema': 'fr-rechecked-text-package', 'complete': False, 'source_bindings': bindings(),
        'source_commit': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
        'binary_sha256': digest(binary), 'all_projects_familiar': True, 'current_checker_was_unread': False,
        'collection': {'workflow': '.github/workflows/recheck-text-package.yml', 'run_id': os.environ['GITHUB_RUN_ID'],
            'attempt': os.environ['GITHUB_RUN_ATTEMPT'], 'repository': os.environ['GITHUB_REPOSITORY']},
        'original_auditor': original.HISTORICAL_AUDITOR, 'original_source_commit': original.FIRST_SOURCE,
        'original_audit_passed': True, 'original_unread': {'project': SELECTION['repository'], 'qualified': True,
            'first_evaluation_run': original.FIRST_RUN}, 'limits': SELECTION['limits'], 'benchmark': False, 'live_model_calls': 0,
        'claim': 'Maintenance replay with the current checker. All four projects are familiar. The first unread Stringcase result is checked separately with its original checker and procedure.'}
    work = directory / 'stringcase'
    work.mkdir()
    (work / 'source.tar.gz').write_bytes(data)
    upstream = work / 'upstream'
    upstream.mkdir()
    files = packages.flow.snapshot(data, PROJECT, upstream)
    row = {'project': PROJECT, 'cases': [], 'error': None, 'complete': False,
        'source_files': files, 'archive_sha256': hashlib.sha256(data).hexdigest()}
    result['project'] = row
    try:
        for variant in ('original', 'renamed-relocated'):
            for case in ('positive', 'overwritten', 'unknown', 'steps'):
                path, entry, adapter = packages.prepare(upstream, PROJECT, variant, case)
                name = variant + '-' + case
                cell = {'variant': variant, 'case': case, 'path': path, 'entry': entry, 'adapter': adapter, 'runtime': []}
                row['cases'].append(cell)
                request = {'mode': 'analysis', 'scope': str(upstream), 'path': path, 'entry': entry,
                    'report_bytes': SELECTION['limits']['report_bytes'], 'project': PROJECT, 'case': case,
                    'fr': str(binary), 'report': str(work / (name + '.json'))}
                cell['analysis'] = packages.execute(work, name, request, upstream)
                save(directory / 'result.json', result)
                for index, value in enumerate(PROJECT['values']):
                    observed = packages.execute(work, name + '-python-' + str(index),
                        {'mode': 'runtime', 'scope': str(upstream), 'path': path, 'entry': entry, 'value': value}, upstream)
                    cell['runtime'].append({'input': value, 'expected': PROJECT['blocked'] if case == 'overwritten' else PROJECT['expected'][index],
                        'observation': observed})
        packages.collect_cache_controls(upstream, PROJECT, work, binary, row, result, directory,
            report_bytes=SELECTION['limits']['report_bytes'])
        row.update(complete=True, original_sources_unchanged=all(digest(upstream / name) == value for name, value in files.items()))
        packages.checked_questions([row], {'stringcase': PROJECT}, True)
        replay.collect(binary, directory / 'earlier-questions')
        result.update(complete=True, independent_projects=sorted([PROJECT['repository'], 'dateutil/dateutil', 'mahmoud/boltons', 'phn/jdcal']),
            milestone_1_demonstrated=True, milestone_2_demonstrated=True, milestones_complete=True)
    except Exception as error:
        result['error'] = type(error).__name__ + ': ' + str(error)
        raise
    finally:
        result['files'] = {str(path.relative_to(directory)): digest(path) for path in directory.rglob('*')
            if path.is_file() and path != directory / 'result.json'}
        save(directory / 'result.json', result)
    return audit(directory)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='command', required=True)
    capture = sub.add_parser('collect')
    capture.add_argument('--binary', required=True, type=Path)
    capture.add_argument('--output', required=True, type=Path)
    check = sub.add_parser('audit')
    check.add_argument('--directory', required=True, type=Path)
    args = parser.parse_args()
    result = collect(args.binary, args.output) if args.command == 'collect' else audit(args.directory)
    print(json.dumps({'complete': result['complete'], 'projects': len(result['independent_projects']), 'all_projects_familiar': True}))


if __name__ == '__main__':
    main()
