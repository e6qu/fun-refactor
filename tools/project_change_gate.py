"""Record successful familiar checks before admitting the reserved unread project."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import shutil

from project_change_sources import bindings, save
from project_change_sources import digest
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'sdk/python/src'))
from project_change_records import audit_delivery, audit_findings

ROOT = Path(__file__).resolve().parents[1]
FREEZE = ROOT / 'tests/agent-eval/project-changes/freeze.json'


def retained_tools():
    spec = importlib.util.spec_from_file_location('scalar_retention', ROOT / 'tools/retain-scalar-evidence.py')
    scalar = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(scalar)
    return scalar


def capture(run_id, name, destination, repository, result_name):
    scalar = retained_tools()
    retain = scalar.retain
    run = retain.api(f'repos/{repository}/actions/runs/{run_id}')
    assert run['status'] == 'completed' and run['conclusion'] == 'success'
    assert run['run_attempt'] == 1 and run['event'] == 'workflow_dispatch'
    artifacts = retain.api(f'repos/{repository}/actions/runs/{run_id}/artifacts?per_page=100')['artifacts']
    matches = [item for item in artifacts if item['name'] == name and not item['expired']]
    assert len(matches) == 1 and matches[0]['size_in_bytes'] <= 16 * 1024**2
    artifact = matches[0]
    archive = retain.download_artifact(repository, artifact['id'])
    data = scalar.files(archive, artifact['digest'])
    result = json.loads(data[result_name])
    jobs = retain.api(f'repos/{repository}/actions/runs/{run_id}/jobs?filter=all&per_page=100')['jobs']
    identity = retain.collection(run, artifact, result, repository, jobs, name, 'project-changes')
    assert result['source_bindings'] == bindings(ROOT), 'collection inputs differ from the committed procedure'
    destination.mkdir(parents=True, exist_ok=False)
    (destination / 'artifact.zip').write_bytes(archive)
    save(destination / 'identity.json', {'collection': identity, 'run': run, 'artifact': artifact,
        'producer': next(job for job in jobs if job['id'] == identity['producer_job_id'])})
    for name, content in data.items():
        target = destination / 'files' / name
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(content)
    return result, identity


def freeze(args):
    assert os.environ.get('GITHUB_ACTIONS') == 'true', 'freeze on a GitHub runner'
    assert not FREEZE.exists(), 'preserve the original implementation freeze'
    assert not subprocess.check_output(['git', 'status', '--porcelain', '--', *bindings(ROOT)],
        cwd=ROOT, text=True).strip(), 'commit all evaluation inputs before freezing'
    repository = os.environ['GITHUB_REPOSITORY']
    destination = ROOT / 'tests/agent-eval/project-changes/qualification'
    development, development_identity = capture(args.development_run, 'project-change-inspection',
        destination / 'development', repository, 'scope.json')
    assert development['passed'] is True and development['source_clean'] is True
    delivery, delivery_identity = capture(args.delivery_run, 'project-change-delivery',
        destination / 'delivery', repository, 'result.json')
    findings, findings_identity = capture(args.findings_run, 'project-change-findings',
        destination / 'findings', repository, 'result.json')
    checkers, checkers_identity = capture(args.findings_run, 'project-change-checkers',
        destination / 'checkers', repository, 'checkers.json')
    for phase, result in [('delivery', delivery), ('findings', findings)]:
        assert result['passed'] is True and result['phase'] == phase
        assert result['unread_project_opened'] is False
        assert [row['project'] for row in result['projects']] == ['glob', 'boltons']
        assert all(row[phase]['passed'] is True for row in result['projects'])
    assert delivery['binary_sha256'] == findings['binary_sha256'] == checkers['production']['sha256']
    assert checkers['production']['sha256'] != checkers['alternate']['sha256']
    assert checkers['control']['production']['accepted'] is True
    assert checkers['control']['alternate']['accepted'] is False
    assert digest(args.fr) == checkers['production']['sha256']
    assert digest(args.alternate) == checkers['alternate']['sha256']
    tasks = json.loads((ROOT / 'tests/agent-eval/project-changes/tasks.json').read_text())
    for task in tasks['projects']:
        if task['role'] != 'development':
            continue
        audit_delivery(task, destination / 'delivery/files' / task['id'], tasks['limits'], args.fr)
        audit_findings(task, destination / 'findings/files' / task['id'], tasks['limits'], args.fr, args.alternate)
    assert len({development_identity['source_commit'], delivery_identity['source_commit'],
                findings_identity['source_commit']}) == 1, 'qualify one committed implementation and procedure'
    save(FREEZE, {'implementation_commit': development_identity['source_commit'],
        'input_bindings': bindings(ROOT), 'production_sha256': checkers['production']['sha256'],
        'alternate_sha256': checkers['alternate']['sha256'], 'repository': repository,
        'successful_runs': {'development': development_identity, 'delivery': delivery_identity,
                            'findings': findings_identity, 'checkers': checkers_identity},
        'unread_project': 'humanize', 'unread_implementation_opened': False})
    for directory in destination.iterdir():
        shutil.rmtree(directory / 'files')


def admit(binary, alternate):
    frozen = json.loads(FREEZE.read_text())
    assert frozen['input_bindings'] == bindings(ROOT), 'implementation or procedure changed after qualification'
    assert hashlib.sha256(binary.read_bytes()).hexdigest() == frozen['production_sha256']
    assert hashlib.sha256(alternate.read_bytes()).hexdigest() == frozen['alternate_sha256']
    assert frozen['unread_implementation_opened'] is False and frozen['unread_project'] == 'humanize'
    return frozen


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--runs', required=True, help='JSON map with development, delivery and findings run IDs')
    parser.add_argument('--fr', type=Path, required=True)
    parser.add_argument('--alternate', type=Path, required=True)
    args = parser.parse_args()
    runs = json.loads(args.runs)
    assert set(runs) == {'development', 'delivery', 'findings'}
    assert len(set(runs.values())) == 3 and all(type(value) is int and value > 0 for value in runs.values())
    args.development_run, args.delivery_run, args.findings_run = (runs[key] for key in ('development', 'delivery', 'findings'))
    freeze(args)
