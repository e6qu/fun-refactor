"""Recheck all earlier package questions with the current checker; all are familiar now."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import time
import shutil

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'sdk/python/src'))
from project_change_sources import digest, save, bindings as project_bindings


def load(name):
    spec = importlib.util.spec_from_file_location(name.replace('-', '_'), ROOT / 'tools' / (name + '.py'))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


packages = load('package-questions')
historical = load('audit-historical-package-questions')
DIRECTORIES = {phase: ROOT / f'tests/agent-eval/results/2026-10-20-leap-year-package-questions-{phase}'
               for phase in ('coverage', 'held-out')}


def bindings():
    files = ['tools/package_question_replay.py', 'tools/audit-historical-package-questions.py',
             'tools/project_change_sources.py', 'tools/project_change_gate.py']
    for directory in DIRECTORIES.values():
        files.extend(str((directory / name).relative_to(ROOT)) for name in ('receipt.json', 'artifact.zip', 'result.json'))
    return {**project_bindings(ROOT), **packages.bindings(), **{name: digest(ROOT / name) for name in files}}


def audit(output):
    report = json.loads((output / 'result.json').read_text())
    assert report['schema'] == 'fr-rechecked-package-questions' and report['complete'] is True
    assert all(report[key] is True for key in ('milestone_1_demonstrated', 'milestone_2_demonstrated', 'milestones_complete'))
    assert report['source_bindings'] == bindings() and report['all_projects_familiar'] is True
    assert report['original_source_commit'] == historical.REVISION and report['original_audit_passed'] is True
    assert report['limits'] == packages.LIMITS
    assert [row['project']['id'] for row in report['projects']] == ['boltons', 'dateutil', 'jdcal']
    for row in report['projects']:
        declared = {row['project']['id']: row['project']}
        packages.checked_questions([row], declared, row['project']['role'] == 'coverage')
        for path, expected in row['source_files'].items():
            assert digest(output / row['project']['id'] / 'upstream' / path) == expected
    for name, expected in report['files'].items():
        path = output / name
        assert path.resolve().is_relative_to(output.resolve()) and digest(path) == expected
    return {'complete': True, 'projects': 3, 'all_projects_familiar': True, 'new_unread_claim': False}


def collect(binary, output):
    output.mkdir(parents=True, exist_ok=False)
    save(output / 'setup.json', {'stage': 'original historical audit', 'passed': False,
        'source_commit': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()})
    historical.audit(historical.REVISION)
    save(output / 'setup.json', {'stage': 'original historical audit', 'passed': True,
        'source_commit': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()})
    before = bindings()
    archives = {}
    scalar = load('retain-scalar-evidence')
    for phase, directory in DIRECTORIES.items():
        receipt_path = directory / 'receipt.json'
        recorded = subprocess.check_output(['git', 'show', historical.REVISION + ':' + str(receipt_path.relative_to(ROOT))], cwd=ROOT)
        assert recorded == receipt_path.read_bytes(), 'the original collection receipt changed'
        receipt = json.loads(recorded)
        data = scalar.files((directory / 'artifact.zip').read_bytes(), receipt['artifact_digest'])
        archives.update({name: value for name, value in data.items() if name.endswith('/source.tar.gz')})
    result = {'schema': 'fr-rechecked-package-questions', 'complete': False, 'source_bindings': before,
        'source_commit': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
        'binary_sha256': digest(binary), 'original_source_commit': historical.REVISION,
        'original_audit_passed': True, 'all_projects_familiar': True, 'limits': packages.LIMITS,
        'projects': [], 'claim': 'Maintenance replay of the earlier Boltons, dateutil and JDcal questions. The original unread result is audited with its original source; this replay makes no new unread-project claim.'}
    save(output / 'result.json', result)
    for project in packages.PLAN['projects']:
        started = time.monotonic()
        directory = output / project['id']
        directory.mkdir()
        data = archives[project['id'] + '/source.tar.gz']
        (directory / 'source.tar.gz').write_bytes(data)
        upstream = directory / 'upstream'
        upstream.mkdir()
        source_files = packages.flow.snapshot(data, project, upstream)
        scope = upstream / project['python_root']
        row = {'project': project, 'cases': [], 'error': None, 'complete': False,
               'source_files': source_files, 'archive_sha256': hashlib.sha256(data).hexdigest()}
        result['projects'].append(row)
        for variant in ('original', 'renamed-relocated'):
            for case in ('positive', 'overwritten', 'unknown', 'steps'):
                path, entry, adapter = packages.prepare(scope, project, variant, case)
                name = variant + '-' + case
                cell = {'variant': variant, 'case': case, 'path': path, 'entry': entry, 'adapter': adapter, 'runtime': []}
                row['cases'].append(cell)
                request = {'mode': 'analysis', 'scope': str(scope), 'path': path, 'entry': entry,
                    'project': project, 'case': case, 'fr': str(binary), 'report': str(directory / (name + '.json'))}
                cell['analysis'] = packages.execute(directory, name, request, scope)
                for index, value in enumerate(project['values']):
                    request = {'mode': 'runtime', 'scope': str(scope), 'path': path, 'entry': entry, 'value': value}
                    observed = packages.execute(directory, name + '-python-' + str(index), request, scope)
                    expected = project['blocked'] if case == 'overwritten' else project['expected'][index]
                    cell['runtime'].append({'input': value, 'expected': expected, 'observation': observed})
                save(output / 'result.json', result)
        if project['role'] == 'coverage':
            packages.collect_cache_controls(scope, project, directory, binary, row, result, output)
        row.update(complete=True, original_sources_unchanged=all(digest(upstream / path) == expected
            for path, expected in source_files.items()), seconds=time.monotonic() - started)
        assert row['seconds'] <= 180
        packages.checked_questions([row], {project['id']: project}, project['role'] == 'coverage')
        for worker in directory.iterdir():
            cache = worker / '.cache'
            if worker.is_dir() and (worker / 'request.json').is_file() and cache.exists():
                shutil.rmtree(cache)
        save(output / 'result.json', result)
    assert bindings() == before
    result.update(complete=True, milestone_1_demonstrated=True, milestone_2_demonstrated=True,
        milestones_complete=True, files={str(path.relative_to(output)): digest(path)
        for path in sorted(output.rglob('*')) if path.is_file() and path != output / 'result.json'})
    save(output / 'result.json', result)
    print(json.dumps(audit(output)))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--fr', type=Path)
    parser.add_argument('--output', type=Path)
    parser.add_argument('--audit', type=Path)
    args = parser.parse_args()
    if args.audit is not None:
        print(json.dumps(audit(args.audit)))
    else:
        assert args.fr is not None and args.output is not None
        collect(args.fr.resolve(), args.output.resolve())
