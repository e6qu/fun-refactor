#!/usr/bin/env python3
"""Retain freshly audited route and repair artifacts on their working branch."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]


def load(name):
    spec = importlib.util.spec_from_file_location(name.replace('-', '_'), ROOT / 'tools' / (name + '.py'))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


retain = load('retain-python-contract')
require = retain.contract.require
GROUPS = {
    'workflow-routes': {
        'schema': 'fr-workflow-route-evidence-1',
        'suffix': 'workflows',
        'reports': {'result.json': 'workflow-route-context',
                    'guide-body-context.json': 'guide-body-context',
                    'recovery-delivery-context.json': 'recovery-delivery-context',
                    'agent-guide-context.json': 'agent-guide-context'},
        'tests': {'test-workflow-route-context': 'result.json',
                  'test-guide-body-context': 'guide-body-context.json',
                  'test-recovery-delivery-context': 'recovery-delivery-context.json'},
    },
    'source-repair': {
        'schema': 'fr-source-repair-evidence-1',
        'suffix': 'source-repair',
        'reports': {'result.json': 'source-repair-context'},
        'tests': {'test-source-repair-context': 'result.json'},
    },
}


def collect(group, run_id, repository):
    run = retain.api(f'repos/{repository}/actions/runs/{run_id}')
    require(run['status'] == 'completed' and run['conclusion'] == 'success',
            'wait for a successful collection before retention')
    artifacts = retain.api(f'repos/{repository}/actions/runs/{run_id}/artifacts?per_page=100')['artifacts']
    candidates = [item for item in artifacts if item['name'] == group and not item['expired']]
    require(len(candidates) == 1, 'need one current artifact for the requested group')
    artifact = candidates[0]
    require(artifact['size_in_bytes'] <= 16 * 1024 * 1024, 'artifact exceeds download admission')
    with tempfile.TemporaryFile() as output:
        retain.command(['gh', 'api', f'repos/{repository}/actions/artifacts/{artifact["id"]}/zip'], stdout=output)
        output.seek(0)
        archive = output.read(16 * 1024 * 1024 + 1)
    require(len(archive) <= 16 * 1024 * 1024, 'artifact exceeds download admission')
    reports = retain.checked_reports(archive, artifact['digest'], list(GROUPS[group]['reports']))
    jobs = retain.api(f'repos/{repository}/actions/runs/{run_id}/jobs?filter=all&per_page=100')['jobs']
    primary = json.loads(reports['result.json'])
    identity = retain.collection(run, artifact, primary, repository, jobs, group)
    for name, evaluator in GROUPS[group]['reports'].items():
        value = json.loads(reports[name])
        # The older scalar guide format has source bindings but no collection
        # fields. Its exact bytes belong to the same verified producer archive.
        if name != 'agent-guide-context.json':
            require(value['source_commit'] == identity['source_commit']
                    and value['binary_sha256'] == primary['binary_sha256'], 'report family source or binary changed')
        load(evaluator).audit(value)
    return reports, identity, primary['binary_sha256']


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--routes-run', required=True, type=int)
    parser.add_argument('--repair-run', required=True, type=int)
    parser.add_argument('--prefix', required=True)
    args = parser.parse_args()
    require(os.environ.get('GITHUB_ACTIONS') == 'true', 'retain evidence on GitHub runners')
    branch = os.environ.get('GITHUB_REF', '')
    require(branch.startswith('refs/heads/') and branch != 'refs/heads/main', 'retain into a working branch')
    require(re.fullmatch(r'[a-zA-Z0-9][a-zA-Z0-9_-]{0,79}', args.prefix) is not None,
            'supply a plain dated evidence prefix')
    repository = os.environ['GITHUB_REPOSITORY']
    collections = {group: collect(group, run, repository)
                   for group, run in [('workflow-routes', args.routes_run), ('source-repair', args.repair_run)]}
    require(len({identity['source_commit'] for _, identity, _ in collections.values()}) == 1,
            'collect the workflow family from one source commit')
    for group, (reports, identity, binary) in collections.items():
        settings = GROUPS[group]
        relative = Path('tests/agent-eval/results') / (args.prefix + '-' + settings['suffix'])
        destination = ROOT / relative
        require(not destination.exists(), 'preserve prior results; use a new evidence directory')
        destination.mkdir()
        for name, data in reports.items():
            (destination / name).write_bytes(data)
        manifest = {'schema': settings['schema'], 'collection': identity, 'binary_sha256': binary,
                    'files': {name: hashlib.sha256(data).hexdigest() for name, data in reports.items()}}
        (destination / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
        for test, name in settings['tests'].items():
            path = ROOT / 'tools' / (test + '.py')
            text, count = re.subn(r'^REPORT = ROOT / [\'\"][^\'\"]+[\'\"]$',
                                  f"REPORT = ROOT / '{relative / name}'", path.read_text(), flags=re.MULTILINE)
            require(count == 1, 'expected exactly one retained report reference')
            path.write_text(text)
            subprocess.run(['python3', '-B', str(path)], cwd=ROOT, check=True, timeout=120)
        print(json.dumps({'retained': str(relative), 'collection': identity}, sort_keys=True))


if __name__ == '__main__':
    main()
