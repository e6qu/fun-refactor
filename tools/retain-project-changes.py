"""Retain and replay original project changes, including every declared failed run."""
import argparse
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'sdk/python/src'))
from project_change_sources import bindings, digest, save
from project_change_gate import FREEZE, capture, retained_tools, attempts
from project_change_records import audit_delivery, audit_findings


def unpack_record(directory, scalar, destination):
    identity = json.loads((directory / 'identity.json').read_text())
    data = scalar.files((directory / 'artifact.zip').read_bytes(), identity['collection']['artifact_digest'])
    for name, content in data.items():
        path = destination / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(content)
    return data


def audit(directory, binary, alternate):
    report = json.loads((directory / 'result.json').read_text())
    assert report['schema'] == 'fr-project-changes' and report['passed'] is True
    assert report['source_bindings'] == bindings(ROOT)
    assert report['freeze_sha256'] == digest(FREEZE) == digest(directory / 'freeze.json')
    frozen = json.loads(FREEZE.read_text())
    assert digest(binary) == frozen['production_sha256'] and digest(alternate) == frozen['alternate_sha256']
    assert report['saved_findings'] == {'complete': True, 'projects': 3, 'changes_per_project': 7}
    assert report['deliveries'] == {'Rust': {'complete': True}, 'Python': {'complete': True}}
    assert report['unread'] == {'project': 'humanize', 'qualified': True, 'first_evaluation': True}
    inventory = json.loads((directory / 'attempts.json').read_text())
    opened = [row['run']['id'] for row in inventory if row.get('opened_unread_archive')]
    identity = json.loads((directory / 'held-out/identity.json').read_text())
    assert opened and min(opened) == int(identity['collection']['run_id']), 'this was not the first unread evaluation'
    assert report['failures'] and len(report['failures']) == len(set(report['failures']))
    assert set(frozen['development_failures']) <= set(report['failures']), 'a recorded failure disappeared'
    for name, expected in report['files'].items():
        path = directory / name
        assert path.resolve().is_relative_to(directory.resolve()) and digest(path) == expected
    tasks = json.loads((ROOT / 'tests/agent-eval/project-changes/tasks.json').read_text())
    scalar = retained_tools()
    for phase in ('delivery', 'findings'):
        identity = json.loads((directory / phase / 'identity.json').read_text())
        assert identity['collection'] == frozen['successful_runs'][phase]
    with tempfile.TemporaryDirectory(prefix='fr-audit-project-milestone-') as temporary:
        base = Path(temporary)
        records = {}
        for phase in ('delivery', 'findings', 'held-out'):
            records[phase] = unpack_record(directory / phase, scalar, base / phase)
        held = json.loads(records['held-out']['result.json'])
        assert held['passed'] is True and held['phase'] == 'held-out'
        assert held['implementation'] == frozen and held['unread_project_opened'] is True
        assert [row['project'] for row in held['projects']] == ['humanize']
        for task in tasks['projects']:
            phase = 'held-out' if task['role'] == 'unread' else 'findings'
            audit_findings(task, base / phase / task['id'], tasks['limits'], binary, alternate)
            if task['delivery'] is not None:
                audit_delivery(task, base / 'delivery' / task['id'], tasks['limits'], binary)
    for run_id in report['failures']:
        metadata = json.loads((directory / 'failures' / str(run_id) / 'metadata.json').read_text())
        assert metadata['run']['id'] == run_id and metadata['run']['status'] == 'completed'
        assert metadata['run']['conclusion'] in ('failure', 'cancelled')
    return {'passed': True, 'projects': 3, 'change_controls': 21, 'delivered_languages': ['Rust', 'Python']}


def retain(args):
    assert os.environ.get('GITHUB_ACTIONS') == 'true'
    assert re.fullmatch(r'[a-zA-Z0-9][a-zA-Z0-9_-]{0,79}', args.prefix)
    directory = ROOT / 'tests/agent-eval/results' / (args.prefix + '-project-changes')
    directory.mkdir(parents=True, exist_ok=False)
    frozen = json.loads(FREEZE.read_text())
    assert frozen['input_bindings'] == bindings(ROOT)
    repository = os.environ['GITHUB_REPOSITORY']
    held, _ = capture(args.run, 'project-change-held-out', directory / 'held-out', repository, 'result.json')
    assert held['phase'] == 'held-out' and held['implementation'] == frozen
    assert held['passed'] is True and held['unread_project_opened'] is True
    for phase in ('delivery', 'findings'):
        shutil.copytree(FREEZE.parent / 'qualification' / phase, directory / phase)
    shutil.copyfile(FREEZE, directory / 'freeze.json')
    scalar = retained_tools()
    failures = json.loads(args.failures)
    assert failures and all(type(run_id) is int and run_id > 0 for run_id in failures)
    assert len(set(failures)) == len(failures)
    inventory = attempts(repository, os.environ['GITHUB_REF_NAME'], args.run)
    expected_failures = [row['run']['id'] for row in inventory if row['run']['conclusion'] in ('failure', 'cancelled')]
    assert set(failures) == set(expected_failures), 'retain every project-change failure: ' + json.dumps(expected_failures)
    for row in inventory:
        executed = any(step['name'] == 'Run the first reserved unread-project evaluation'
            and step['conclusion'] in ('success', 'failure') for job in row['jobs'] for step in job['steps'])
        row['opened_unread_archive'] = False
        if not executed:
            continue
        candidates = scalar.retain.api(f'repos/{repository}/actions/runs/{row["run"]["id"]}/artifacts?per_page=100')['artifacts']
        for artifact in candidates:
            if artifact['name'] == 'project-change-held-out':
                archive = scalar.retain.download_artifact(repository, artifact['id'])
                data = scalar.files(archive, artifact['digest'])
                row['opened_unread_archive'] = 'humanize/source/source.tar.gz' in data
    assert min(row['run']['id'] for row in inventory if row['opened_unread_archive']) == args.run
    save(directory / 'attempts.json', inventory)
    for run_id in failures:
        run = scalar.retain.api(f'repos/{repository}/actions/runs/{run_id}')
        assert run['status'] == 'completed' and run['conclusion'] in ('failure', 'cancelled')
        assert run['repository']['full_name'] == repository and run['run_attempt'] == 1
        assert run['path'] == '.github/workflows/refinement-evidence.yml'
        output = directory / 'failures' / str(run_id)
        artifacts = scalar.retain.api(f'repos/{repository}/actions/runs/{run_id}/artifacts?per_page=100')['artifacts']
        save(output / 'metadata.json', {'run': run, 'artifacts': artifacts})
        for artifact in artifacts:
            assert not artifact['expired'] and artifact['size_in_bytes'] <= 16 * 1024**2
            data = scalar.retain.download_artifact(repository, artifact['id'])
            scalar.files(data, artifact['digest'])
            (output / (str(artifact['id']) + '.zip')).write_bytes(data)
        with (output / 'failure.log').open('wb') as stream:
            subprocess.run(['gh', 'run', 'view', str(run_id), '--log-failed'], cwd=ROOT,
                stdout=stream, check=True, timeout=30)
        assert (output / 'failure.log').stat().st_size <= 2097152
    shutil.rmtree(directory / 'held-out/files')
    save(directory / 'result.json', {'schema': 'fr-project-changes', 'passed': True,
        'source_bindings': bindings(ROOT), 'freeze_sha256': digest(FREEZE),
        'saved_findings': {'complete': True, 'projects': 3, 'changes_per_project': 7},
        'deliveries': {'Rust': {'complete': True}, 'Python': {'complete': True}},
        'unread': {'project': 'humanize', 'qualified': True, 'first_evaluation': True},
        'failures': failures, 'files': {str(path.relative_to(directory)): digest(path)
            for path in sorted(directory.rglob('*')) if path.is_file()},
        'scope': 'Deterministic discovery across three original projects and checked Rust/Python renames. Indexed uses and the executed behavior cases only; no agent-cost, general-language or proof claim.'})
    result = audit(directory, args.fr, args.alternate)
    catalog_path = ROOT / 'tests/agent-eval/roadmap.json'
    catalog = json.loads(catalog_path.read_text())
    relative = str((directory / 'result.json').relative_to(ROOT))
    assert not any(entry['id'] == 'project-changes' for entry in catalog['evidence'])
    catalog['evidence'].append({'id': 'project-changes', 'path': relative,
        'sha256': digest(directory / 'result.json'), 'kind': 'deterministic',
        'audit': f'python3 tools/retain-project-changes.py --audit {relative} --fr target/project-change-checkers/production-fr --alternate target/project-change-checkers/alternate-fr',
        'scope': 'Three original projects, seven change controls each, and checked Rust and Python rename delivery.'})
    for obligation in catalog['obligations']:
        pointers = {'C.repository-mutations': ['/saved_findings/complete', '/unread/qualified'],
            'D.structural': ['/deliveries/Rust/complete', '/deliveries/Python/complete']}.get(obligation['id'])
        if pointers is not None:
            obligation.update(state='demonstrated', blockers=[], next='Keep the original project checks, undo and patch replay passing.',
                requires=[{'evidence': 'project-changes', 'assertions': [{'pointer': pointer, 'equals': True} for pointer in pointers]}])
    save(catalog_path, catalog)
    print(json.dumps(result))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run', type=int)
    parser.add_argument('--failures')
    parser.add_argument('--prefix')
    parser.add_argument('--audit', type=Path)
    parser.add_argument('--fr', type=Path, required=True)
    parser.add_argument('--alternate', type=Path, required=True)
    args = parser.parse_args()
    if args.audit is not None:
        print(json.dumps(audit(args.audit, args.fr, args.alternate)))
    else:
        assert args.run is not None and args.failures is not None and args.prefix is not None
        retain(args)
