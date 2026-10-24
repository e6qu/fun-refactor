import argparse
from datetime import datetime
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile

from evidence_basis import native_source_bindings, native_source_roots

ROOT = Path(__file__).resolve().parents[1]
FIRST_RUN = 40026358439
FIRST_SOURCE = '37d9cad03424fc02526db49a45a544083af0b511'
WORKFLOW = '.github/workflows/python-text-package.yml'
HISTORICAL_AUDITOR = 'de291111549d8764565963ab346d0d7647747df5'
HISTORICAL_DIRECTORY = 'tests/agent-eval/results/2026-10-24-pr573-python-text-package'
SELECTION_PATH = 'tests/agent-eval/python-text-flow/selection.json'
SELECTION = json.loads((ROOT / SELECTION_PATH).read_text())
spec = importlib.util.spec_from_file_location('text_package_retention', ROOT / 'tools/retain-scalar-evidence.py')
scalar = importlib.util.module_from_spec(spec)
spec.loader.exec_module(scalar)
require = scalar.require


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def save(path, value):
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + '\n')


def checker_bindings():
    return native_source_bindings(ROOT) | {str(path.relative_to(ROOT)): digest(path)
        for path in (ROOT / 'sdk/python/src').rglob('*.py')}


def bindings():
    names = ['tools/python-text-package.py', 'tools/test-python-text-package.py',
        'tools/retain-scalar-evidence.py', 'tools/retain-python-contract.py', 'tools/python-contract.py', 'tools/evidence_basis.py',
        WORKFLOW, SELECTION_PATH]
    return checker_bindings() | {name: digest(ROOT / name) for name in names}


def checked_collection(run, artifact, jobs, repository):
    require(run['id'] == FIRST_RUN and run['head_sha'] == FIRST_SOURCE
        and run['path'] == WORKFLOW and run['event'] == 'workflow_dispatch'
        and run['status'] == 'completed' and run['conclusion'] == 'success' and run['run_attempt'] == 1
        and run['repository']['full_name'] == repository and run['head_repository']['full_name'] == repository,
        'retain the successful first package collection from its declared workflow and source')
    require(artifact['name'] == 'python-text-package' and artifact['expired'] is False
        and artifact['workflow_run']['id'] == FIRST_RUN and artifact['workflow_run']['head_sha'] == FIRST_SOURCE
        and artifact['size_in_bytes'] <= 16 * 1024 * 1024, 'package artifact identity or admission changed')
    produced = datetime.fromisoformat(artifact['created_at'])
    producers = [job for job in jobs if job['name'] == 'question' and job['run_id'] == FIRST_RUN
        and job['status'] == 'completed' and job['conclusion'] == 'success' and job['run_attempt'] == 1
        and datetime.fromisoformat(job['started_at']) <= produced <= datetime.fromisoformat(job['completed_at'])]
    require(len(producers) == 1, 'package artifact requires one successful matching producer')
    return {'run_id': str(FIRST_RUN), 'source_commit': FIRST_SOURCE, 'workflow': WORKFLOW,
        'attempt': 1, 'producer_job_id': producers[0]['id'], 'artifact_id': artifact['id'],
        'artifact_digest': artifact['digest'], 'url': run['html_url']}


def delivery(data):
    result = json.loads(data['result.json'])
    earlier = json.loads(data['earlier-questions/result.json'])
    sources = {'stringcase/upstream/' + name: value for name, value in result['project']['source_files'].items()}
    for row in earlier['projects']:
        sources.update({'earlier-questions/' + row['project']['id'] + '/upstream/' + name: value
            for name, value in row['source_files'].items()})
    require(set(data) <= {'result.json', *result['files']}, 'unexpected first-artifact file')
    missing = set(result['files']) - set(data)
    restored = {name: result['files'][name] for name in sorted(missing & set(sources))}
    require(all(expected == sources[name] for name, expected in restored.items()), 'missing source digest changed')
    workers = [variant + '-' + case for variant in ('original', 'renamed-relocated')
        for case in ('positive', 'overwritten', 'unknown', 'steps')]
    workers += ['cache-' + stage for stage in ('cold', 'restored', 'unrelated-edit',
        'helper-replaced', 'helper-restored', 'import-replaced')]
    omitted = {name: result['files'][name] for name in sorted(missing - set(restored))}
    require(all(any(name.startswith('stringcase/' + worker + '/.cache/fun-refactor/') for worker in workers)
                for name in omitted), 'first artifact lost a required answer, command or saved object')
    require(all(name in sources and any(part.startswith('.') for part in Path(name).parts) for name in restored),
        'source restoration must explain hidden-file filtering')
    for name, value in data.items():
        if name != 'result.json':
            require(hashlib.sha256(value).hexdigest() == result['files'][name], 'first artifact file changed: ' + name)
    return {'original_upload_complete': not missing, 'restored_source_file_count': len(restored),
        'unavailable_native_cache_file_count': len(omitted),
        'restored_source_files': restored, 'unavailable_native_cache_files': omitted,
        'scope': 'The first upload omitted hidden files. Recover original project files from their authenticated source archives. Internal native index cache payloads remain unavailable; all question outputs, commands and SDK saved objects must remain present. This record makes no timing or native-cache storage claim.'}


def validate_original(original, records, omissions):
    result = json.loads((records / 'result.json').read_text())
    frozen = json.loads((records / 'freeze.json').read_text())
    require(result['source_bindings'] == frozen['source_bindings'] == original.bindings(), 'original procedure inputs changed')
    require(result['freeze_sha256'] == digest(records / 'freeze.json') and frozen['implementation_and_tests_unread'] is True,
        'original first-evaluation declaration changed')
    earlier = json.loads((records / 'earlier-questions/result.json').read_text())
    projects = [(result['project'], records / 'stringcase')]
    projects += [(row, records / 'earlier-questions' / row['project']['id']) for row in earlier['projects']]
    with tempfile.TemporaryDirectory(prefix='fr-original-package-sources-') as temporary:
        for row, work in projects:
            archive = work / 'source.tar.gz'
            require(digest(archive) == row['archive_sha256'], 'original source archive changed')
            canonical = Path(temporary) / row['project']['id']
            canonical.mkdir()
            inventory = original.packages.flow.snapshot(archive.read_bytes(), row['project'], canonical)
            require(inventory == row['source_files'], 'original source inventory changed')
            for name, expected in inventory.items():
                target = work / 'upstream' / name
                if target.exists():
                    require(digest(target) == expected, 'retained source changed')
                else:
                    relative = str(target.relative_to(records))
                    require(omissions['restored_source_files'][relative] == expected, 'undeclared missing original source')
                    target.parent.mkdir(parents=True, exist_ok=True)
                    target.write_bytes((canonical / name).read_bytes())
    original.packages.checked_questions([result['project']], {'stringcase': original.PROJECT}, True)
    original.replay.audit(records / 'earlier-questions')
    require(earlier['binary_sha256'] == result['binary_sha256'], 'projects used different executables')
    require(result['benchmark'] is False and result['live_model_calls'] == 0, 'unexpected cost or model claim')
    require(result['project']['original_sources_unchanged'] is True and frozen['selection'] == original.SELECTION,
        'reserved question or original sources changed')
    work = records / 'stringcase'
    reports = [work / (cell['variant'] + '-' + cell['case'] + '.json') for cell in result['project']['cases']]
    reports += [work / (stage + '-' + kind + '.json') for stage, _, _ in original.packages.CACHE_STAGES
        for kind in ('saved', 'fresh')]
    for path in reports:
        value, limits = json.loads(path.read_text()), original.SELECTION['limits']
        require(value['budget']['response_bytes'] == limits['report_bytes']
            and value['budget']['call_depth'] == limits['depth']
            and value['budget']['steps'] == (1 if path.stem.endswith('-steps') else limits['steps'])
            and value['inputs']['modules']['limits']['modules'] == limits['modules']
            and value['inputs']['modules']['limits']['source_bytes'] == limits['source_bytes'], 'original limits changed')
    for name, expected in result['files'].items():
        if name not in omissions['unavailable_native_cache_files']:
            require(digest(records / name) == expected, 'original answer or source file changed: ' + name)


def original_audit(data, directory):
    original = json.loads(data['result.json'])
    require(original['source_commit'] == FIRST_SOURCE and original['complete'] is True,
        'original package result did not pass at its recorded source')
    frozen = json.loads(data['freeze.json'])
    require(frozen['source_commit'] == FIRST_SOURCE and frozen['selection'] == SELECTION,
        'original frozen question changed')
    roots = (*native_source_roots(ROOT), 'sdk/python/src')
    recorded = {name: value for name, value in original['source_bindings'].items()
        if any(name == root or name.startswith(root + '/') for root in roots)}
    require(recorded == checker_bindings(), 'current checker differs from the first evaluation')
    subprocess.run(['git', 'fetch', '--no-tags', '--depth=1', 'origin', FIRST_SOURCE], cwd=ROOT,
        check=True, capture_output=True, timeout=60)
    paths = {'tools/', 'sdk/python/src/', *original['source_bindings']}
    require(all(not Path(name).is_absolute() and '..' not in Path(name).parts
        and not any(char in name for char in '\\!*?[]') for name in paths), 'unsafe historical source input')
    source = directory / 'procedure'
    subprocess.run(['git', 'worktree', 'add', '--no-checkout', '--detach', str(source), FIRST_SOURCE],
        cwd=ROOT, check=True, capture_output=True, timeout=60)
    try:
        subprocess.run(['git', 'sparse-checkout', 'set', '--no-cone', '--stdin'], cwd=source,
            input=''.join('/' + name + '\n' for name in sorted(paths)), text=True, check=True, timeout=60)
        subprocess.run(['git', 'checkout', '--force', '--detach', FIRST_SOURCE], cwd=source,
            check=True, capture_output=True, timeout=60)
        records = directory / 'original'
        records.mkdir()
        for name, value in data.items():
            path = records / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(value)
        omissions = directory / 'delivery.json'
        save(omissions, delivery(data))
        driver = ('import importlib.util, json, sys\nfrom pathlib import Path\n'
            'sys.path.insert(0, str(Path.cwd() / "tools"))\n'
            'def load(name, path):\n'
            '    spec = importlib.util.spec_from_file_location(name, path)\n'
            '    value = importlib.util.module_from_spec(spec)\n'
            '    spec.loader.exec_module(value)\n'
            '    return value\n'
            'first = load("first", Path.cwd() / "tools/python-text-package.py")\n'
            'auditor = load("auditor", sys.argv[1])\n'
            'auditor.validate_original(first, Path(sys.argv[2]), json.loads(Path(sys.argv[3]).read_text()))\n')
        subprocess.run(['python3', '-I', '-B', '-c', driver, str(Path(__file__).resolve()), str(records), str(omissions)],
            cwd=source, check=True, timeout=120)
    finally:
        subprocess.run(['git', 'worktree', 'remove', '--force', str(source)], cwd=ROOT,
            check=True, capture_output=True, timeout=60)
    return original


def audit(directory):
    result = json.loads((directory / 'result.json').read_text())
    require(result['schema'] == 'fr-qualified-text-package' and result['complete'] is True,
        'unknown or unfinished qualified package record')
    require(result['source_bindings'] == bindings(), 'current package auditor or checker changed')
    require(result['original_source_commit'] == FIRST_SOURCE, 'original procedure source changed')
    require(result['files'] == {name: digest(directory / name) for name in ('artifact.zip', 'receipt.json', 'delivery.json')},
        'original package archive or collection identity changed')
    require({path.name for path in directory.iterdir()} == {'result.json', 'receipt.json', 'artifact.zip', 'delivery.json'},
        'qualified package inventory changed')
    receipt = json.loads((directory / 'receipt.json').read_text())
    require(receipt['run_id'] == str(FIRST_RUN) and receipt['source_commit'] == FIRST_SOURCE
        and receipt['workflow'] == WORKFLOW, 'original package receipt changed')
    data = scalar.files((directory / 'artifact.zip').read_bytes(), receipt['artifact_digest'])
    require(json.loads((directory / 'delivery.json').read_text()) == delivery(data), 'original delivery omissions changed')
    with tempfile.TemporaryDirectory(prefix='fr-original-text-package-') as temporary:
        original = original_audit(data, Path(temporary))
    projects = sorted([SELECTION['repository'], 'dateutil/dateutil', 'mahmoud/boltons', 'phn/jdcal'])
    require(original['independent_projects'] == result['independent_projects'] == projects,
        'independent project inventory changed')
    require(result['original_unread'] == {'project': SELECTION['repository'], 'qualified': True,
        'first_evaluation_run': FIRST_RUN} and original['unread_project'] == SELECTION['repository'],
        'original unread-project qualification changed')
    require(all(result[key] is True and original[key] is True for key in
        ('milestone_1_demonstrated', 'milestone_2_demonstrated', 'milestones_complete')),
        'package milestone checks did not pass')
    return result


def historical_audit(directory, source_commit):
    require(source_commit == HISTORICAL_AUDITOR, 'use the recorded qualified-package auditor')
    subprocess.run(['git', 'fetch', '--no-tags', '--depth=1', 'origin', source_commit], cwd=ROOT,
        check=True, capture_output=True, timeout=60)
    require({path.name for path in directory.iterdir()} == {'result.json', 'receipt.json', 'artifact.zip', 'delivery.json'},
        'historical package inventory changed')
    for name in ('result.json', 'receipt.json', 'artifact.zip', 'delivery.json'):
        recorded = subprocess.check_output(['git', 'show', source_commit + ':' + HISTORICAL_DIRECTORY + '/' + name],
            cwd=ROOT, timeout=30)
        require((directory / name).read_bytes() == recorded, 'historical package bytes changed: ' + name)
    report = json.loads((directory / 'result.json').read_text())
    paths = {'tools/', 'sdk/python/src/', *report['source_bindings']}
    require(all(not Path(name).is_absolute() and '..' not in Path(name).parts
        and not any(char in name for char in '\\!*?[]') for name in paths), 'unsafe historical package source input')
    with tempfile.TemporaryDirectory(prefix='fr-qualified-package-audit-') as temporary:
        source = Path(temporary) / 'source'
        subprocess.run(['git', 'worktree', 'add', '--no-checkout', '--detach', str(source), source_commit],
            cwd=ROOT, check=True, capture_output=True, timeout=60)
        try:
            subprocess.run(['git', 'sparse-checkout', 'set', '--no-cone', '--stdin'], cwd=source,
                input=''.join('/' + name + '\n' for name in sorted(paths)), text=True, check=True, timeout=60)
            subprocess.run(['git', 'checkout', '--force', '--detach', source_commit], cwd=source,
                check=True, capture_output=True, timeout=60)
            subprocess.run(['python3', '-B', 'tools/python-text-package.py', 'audit', '--directory', str(directory.resolve())],
                cwd=source, check=True, timeout=180)
        finally:
            subprocess.run(['git', 'worktree', 'remove', '--force', str(source)], cwd=ROOT,
                check=True, capture_output=True, timeout=60)
    return report


def retain(run_id, prefix):
    require(os.environ.get('GITHUB_ACTIONS') == 'true' and run_id == FIRST_RUN,
        'retain the first package result on GitHub')
    require(os.environ['GITHUB_REF'].startswith('refs/heads/') and os.environ['GITHUB_REF'] != 'refs/heads/main',
        'retain into the working branch')
    require(re.fullmatch(r'[a-zA-Z0-9][a-zA-Z0-9_-]{0,79}', prefix), 'supply a plain dated prefix')
    repository = os.environ['GITHUB_REPOSITORY']
    run = scalar.retain.api(f'repos/{repository}/actions/runs/{run_id}')
    artifacts = scalar.retain.api(f'repos/{repository}/actions/runs/{run_id}/artifacts?per_page=100')['artifacts']
    artifact, = [item for item in artifacts if item['name'] == 'python-text-package' and not item['expired']]
    jobs = scalar.retain.api(f'repos/{repository}/actions/runs/{run_id}/jobs?per_page=100')['jobs']
    receipt = checked_collection(run, artifact, jobs, repository)
    archive = scalar.retain.download_artifact(repository, artifact['id'])
    data = scalar.files(archive, artifact['digest'])
    original = json.loads(data['result.json'])
    relative = Path('tests/agent-eval/results') / (prefix + '-python-text-package')
    directory = ROOT / relative
    directory.mkdir()
    (directory / 'artifact.zip').write_bytes(archive)
    save(directory / 'receipt.json', receipt)
    save(directory / 'delivery.json', delivery(data))
    result = {'schema': 'fr-qualified-text-package', 'complete': True, 'source_bindings': bindings(),
        'original_source_commit': FIRST_SOURCE, 'independent_projects': original['independent_projects'],
        'original_unread': {'project': SELECTION['repository'], 'qualified': True, 'first_evaluation_run': FIRST_RUN},
        'milestone_1_demonstrated': True, 'milestone_2_demonstrated': True, 'milestones_complete': True,
        'claim': 'Check the first Stringcase results with their original semantic validators. Current native checker and Python client source must match that first evaluation exactly. Record omitted internal native cache payloads explicitly; recover original source from its retained archives. Boltons, dateutil and JDcal were familiar maintenance checks.',
        'files': {name: digest(directory / name) for name in ('artifact.zip', 'receipt.json', 'delivery.json')}}
    save(directory / 'result.json', result)
    audit(directory)
    catalog_path = ROOT / 'tests/agent-eval/roadmap.json'
    catalog = json.loads(catalog_path.read_text())
    entry, = [row for row in catalog['evidence'] if row['id'] == 'original-package-questions']
    entry.update(path=str(relative / 'result.json'), sha256=digest(directory / 'result.json'),
        scope='Four original-package questions with one unchanged checker. Stringcase passed its first frozen evaluation; audit that procedure at its recorded source. The other three projects were familiar maintenance checks.',
        audit='python3 tools/python-text-package.py audit --directory ' + str(relative))
    obligation, = [row for row in catalog['obligations'] if row['id'] == 'B.repository-flow']
    checks = [(key, True) for key in ('complete', 'milestone_1_demonstrated', 'milestone_2_demonstrated', 'milestones_complete')]
    checks += [('independent_projects', original['independent_projects']), ('original_unread/qualified', True),
        ('original_unread/project', SELECTION['repository'])]
    obligation['requires'] = [{'evidence': 'original-package-questions', 'assertions': [
        {'pointer': '/' + key, 'equals': value} for key, value in checks]}]
    obligation['next'] = 'Keep these four original-package questions reproducible. Full Toolz, Funcy and Pydash imports, exception handling and shared objects remain open.'
    save(catalog_path, catalog)
    subprocess.run(['python3', '-B', 'tools/roadmap-status.py', '--write'], cwd=ROOT, check=True)
    return result


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(dest='command', required=True)
    keep = sub.add_parser('retain')
    keep.add_argument('--run', required=True, type=int)
    keep.add_argument('--prefix', required=True)
    check = sub.add_parser('audit')
    check.add_argument('--directory', required=True, type=Path)
    historical = sub.add_parser('audit-historical')
    historical.add_argument('--directory', required=True, type=Path)
    historical.add_argument('--source-commit', required=True)
    args = parser.parse_args()
    if args.command == 'retain':
        result = retain(args.run, args.prefix)
    elif args.command == 'audit-historical':
        result = historical_audit(args.directory, args.source_commit)
    else:
        result = audit(args.directory)
    print(json.dumps({'complete': result['complete'], 'projects': len(result['independent_projects'])}))
