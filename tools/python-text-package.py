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
        subprocess.run(['python3', '-B', 'tools/python-text-package.py', 'audit', '--directory', str(records)],
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
    require(result['files'] == {name: digest(directory / name) for name in ('artifact.zip', 'receipt.json')},
        'original package archive or collection identity changed')
    require({path.name for path in directory.iterdir()} == {'result.json', 'receipt.json', 'artifact.zip'},
        'qualified package inventory changed')
    receipt = json.loads((directory / 'receipt.json').read_text())
    require(receipt['run_id'] == str(FIRST_RUN) and receipt['source_commit'] == FIRST_SOURCE
        and receipt['workflow'] == WORKFLOW, 'original package receipt changed')
    data = scalar.files((directory / 'artifact.zip').read_bytes(), receipt['artifact_digest'])
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
    result = {'schema': 'fr-qualified-text-package', 'complete': True, 'source_bindings': bindings(),
        'original_source_commit': FIRST_SOURCE, 'independent_projects': original['independent_projects'],
        'original_unread': {'project': SELECTION['repository'], 'qualified': True, 'first_evaluation_run': FIRST_RUN},
        'milestone_1_demonstrated': True, 'milestone_2_demonstrated': True, 'milestones_complete': True,
        'claim': 'Audit the first Stringcase evaluation in its original committed procedure. The current native checker and Python client must match that first evaluation exactly. Boltons, dateutil and JDcal were familiar maintenance checks.',
        'files': {name: digest(directory / name) for name in ('artifact.zip', 'receipt.json')}}
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
    args = parser.parse_args()
    result = retain(args.run, args.prefix) if args.command == 'retain' else audit(args.directory)
    print(json.dumps({'complete': result['complete'], 'projects': len(result['independent_projects'])}))
