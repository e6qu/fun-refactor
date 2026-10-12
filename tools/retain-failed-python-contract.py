#!/usr/bin/env python3
"""Preserve a failed authored collection and replay its original validator."""
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

ROOT = Path(__file__).resolve().parents[1]


def load(name):
    spec = importlib.util.spec_from_file_location(name, ROOT / 'tools' / (name + '.py'))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


retain = load('retain-python-contract')
frozen = load('frozen-repository-flow')
require = retain.contract.require


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run', required=True, type=int)
    parser.add_argument('--prefix', required=True)
    args = parser.parse_args()
    require(os.environ.get('GITHUB_ACTIONS') == 'true', 'retain failures on GitHub runners')
    require(os.environ.get('GITHUB_REF', '').startswith('refs/heads/')
            and os.environ['GITHUB_REF'] != 'refs/heads/main', 'retain into a working branch')
    require(re.fullmatch(r'[a-zA-Z0-9][a-zA-Z0-9_-]{0,79}', args.prefix), 'supply a plain dated prefix')
    repository = os.environ['GITHUB_REPOSITORY']
    run = retain.api(f'repos/{repository}/actions/runs/{args.run}')
    require(run['status'] == 'completed' and run['conclusion'] == 'failure'
            and run['repository']['full_name'] == repository
            and run['head_repository']['full_name'] == repository
            and run['path'] == '.github/workflows/refinement-evidence.yml', 'need a failed same-repository collection')
    artifacts = retain.api(f'repos/{repository}/actions/runs/{args.run}/artifacts?per_page=100')['artifacts']
    matching = [item for item in artifacts if item['name'] == 'python-contract' and not item['expired']]
    require(len(matching) == 1 and matching[0]['size_in_bytes'] <= 16 * 1024 * 1024, 'need one bounded failed artifact')
    artifact = matching[0]
    require(artifact['workflow_run']['id'] == args.run
            and artifact['workflow_run']['head_sha'] == run['head_sha'], 'failure artifact source changed')
    jobs = retain.api(f'repos/{repository}/actions/runs/{args.run}/jobs?filter=all&per_page=100')['jobs']
    created = datetime.fromisoformat(artifact['created_at'])
    producers = [job for job in jobs if job['name'] == 'python-contract' and job['run_id'] == args.run
                 and job['status'] == 'completed' and job['conclusion'] == 'failure'
                 and datetime.fromisoformat(job['started_at']) <= created <= datetime.fromisoformat(job['completed_at'])]
    require(len(producers) == 1 and type(producers[0]['run_attempt']) is int
            and 1 <= producers[0]['run_attempt'] <= run['run_attempt'], 'failed producer identity changed')
    with tempfile.TemporaryFile() as output:
        retain.command(['gh', 'api', f'repos/{repository}/actions/artifacts/{artifact["id"]}/zip'], stdout=output)
        output.seek(0)
        archive = output.read(16 * 1024 * 1024 + 1)
    require(len(archive) <= 16 * 1024 * 1024, 'failed artifact exceeds download admission')
    data = retain.checked_report(archive, artifact['digest'])
    value = json.loads(data)
    require(value['source_commit'] == run['head_sha']
            and value['summary']['contract_complete'] is False and value['summary']['errors'],
            'failed collection cannot claim acceptance')
    destination = ROOT / 'tests/agent-eval/results' / (args.prefix + '-python-contract-failure')
    require(not destination.exists(), 'preserve prior failures')
    retain.command(['git', 'fetch', '--depth=1', 'origin', run['head_sha']], capture_output=True)
    paths = sorted(set(value['source_bindings']) | {'tools', 'sdk/python/src'})
    source = subprocess.check_output(['git', 'archive', '--format=tar.gz', run['head_sha'], '--', *paths],
                                     cwd=ROOT, timeout=60)
    with tempfile.TemporaryDirectory(prefix='fr-failed-contract-') as directory:
        root = Path(directory)
        frozen.unpack(source, root)
        report = root / 'failed.json'
        report.write_bytes(data)
        replay = subprocess.run(['python3', '-B', str(root / 'tools/python-contract.py'), '--audit', str(report)],
                                cwd=root, capture_output=True, text=True, timeout=120)
        require(replay.returncode != 0 and any(error.split(': ', 1)[-1] in replay.stderr
                                             for error in value['summary']['errors']),
                'original source must reproduce the retained failure')
    destination.mkdir()
    (destination / 'result.json').write_bytes(data)
    (destination / 'source.tar.gz').write_bytes(source)
    manifest = {'schema': 'fr-failed-python-contract', 'admitted': False,
                'run_id': str(args.run), 'attempt': producers[0]['run_attempt'],
                'producer_job_id': producers[0]['id'], 'source_commit': run['head_sha'],
                'url': run['html_url'], 'artifact_id': artifact['id'], 'artifact_digest': artifact['digest'],
                'errors': value['summary']['errors'],
                'files': {'result.json': hashlib.sha256(data).hexdigest(),
                          'source.tar.gz': hashlib.sha256(source).hexdigest()}}
    (destination / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    print(json.dumps({'retained_failure': str(destination.relative_to(ROOT)), 'errors': manifest['errors']}))


if __name__ == '__main__':
    main()
