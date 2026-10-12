#!/usr/bin/env python3
"""Retain a successful hosted Python contract without workstation regeneration."""
import argparse
from datetime import datetime
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import zipfile

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('contract', ROOT / 'tools/python-contract.py')
contract = importlib.util.module_from_spec(spec)
spec.loader.exec_module(contract)


def command(args, **kwargs):
    return subprocess.run(args, cwd=ROOT, check=True, timeout=60, **kwargs)


def api(path):
    return json.loads(command(['gh', 'api', path], capture_output=True, text=True).stdout)


def checked_reports(archive, digest, names):
    contract.require(re.fullmatch(r'sha256:[0-9a-f]{64}', digest) is not None
                     and 'sha256:' + hashlib.sha256(archive).hexdigest() == digest,
                     'artifact archive digest changed')
    with zipfile.ZipFile(io.BytesIO(archive)) as zipped:
        contract.require(sorted(zipped.namelist()) == sorted(names)
                         and sum(item.file_size for item in zipped.infolist()) <= 64 * 1024 * 1024,
                         'artifact must contain exactly the bounded report files')
        return {name: zipped.read(name) for name in names}


def checked_report(archive, digest):
    return checked_reports(archive, digest, ['result.json'])['result.json']


def collection(run, artifact, report, repository, jobs, group='python-contract', producer=None):
    contract.require(run['status'] == 'completed' and run['conclusion'] == 'success'
                     and run['path'] == '.github/workflows/refinement-evidence.yml'
                     and run['repository']['full_name'] == repository
                     and run['head_repository']['full_name'] == repository,
                     'collection must be a successful same-repository evidence run')
    contract.require(artifact['name'] == group and artifact['expired'] is False
                     and artifact['workflow_run']['id'] == run['id']
                     and artifact['workflow_run']['head_sha'] == run['head_sha']
                     and report['source_commit'] == run['head_sha'], 'collection source identity changed')
    created = datetime.fromisoformat(artifact['created_at'])
    producers = [job for job in jobs if job['name'] == (producer or group)
                 and job['run_id'] == run['id'] and job['status'] == 'completed' and job['conclusion'] == 'success'
                 and datetime.fromisoformat(job['started_at']) <= created <= datetime.fromisoformat(job['completed_at'])]
    contract.require(len(producers) == 1, 'artifact must belong to one successful collection job')
    producer = producers[0]
    contract.require(type(producer['run_attempt']) is int and 1 <= producer['run_attempt'] <= run['run_attempt'],
                     'collection producer attempt changed')
    return {'run_id': str(run['id']), 'attempt': producer['run_attempt'], 'producer_job_id': producer['id'],
            'url': run['html_url'],
            'source_commit': run['head_sha'], 'artifact_id': artifact['id'], 'artifact_digest': artifact['digest']}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run', required=True, type=int)
    parser.add_argument('--prefix', required=True)
    args = parser.parse_args()
    contract.require(os.environ.get('GITHUB_ACTIONS') == 'true', 'retain evidence on GitHub runners')
    branch = os.environ.get('GITHUB_REF', '')
    contract.require(branch.startswith('refs/heads/') and branch != 'refs/heads/main',
                     'retain into a working branch, never main')
    contract.require(re.fullmatch(r'[a-zA-Z0-9][a-zA-Z0-9_-]{0,79}', args.prefix) is not None,
                     'supply a plain dated evidence prefix')
    repository = os.environ['GITHUB_REPOSITORY']
    run = api(f'repos/{repository}/actions/runs/{args.run}')
    contract.require(run['status'] == 'completed' and run['conclusion'] == 'success',
                     'wait for a successful collection before retaining evidence')
    artifacts = api(f'repos/{repository}/actions/runs/{args.run}/artifacts?per_page=100')['artifacts']
    candidates = [item for item in artifacts if item['name'] == 'python-contract' and not item['expired']]
    contract.require(len(candidates) == 1, 'need one current Python contract artifact')
    artifact = candidates[0]
    contract.require(artifact['size_in_bytes'] <= 16 * 1024 * 1024, 'artifact exceeds download admission')
    # Download through gh, which handles the authenticated redirect. Never extract
    # paths from the archive or trust a successful run in place of the offline audit.
    with tempfile.TemporaryFile() as output:
        command(['gh', 'api', f'repos/{repository}/actions/artifacts/{artifact["id"]}/zip'], stdout=output)
        output.seek(0)
        archive = output.read(16 * 1024 * 1024 + 1)
    contract.require(len(archive) <= 16 * 1024 * 1024, 'artifact exceeds download admission')
    data = checked_report(archive, artifact['digest'])
    value = json.loads(data)
    jobs = api(f'repos/{repository}/actions/runs/{args.run}/jobs?filter=all&per_page=100')['jobs']
    identity = collection(run, artifact, value, repository, jobs)
    result = contract.audit(value)
    relative = Path('tests/agent-eval/results') / (args.prefix + '-python-contract')
    destination = ROOT / relative
    contract.require(not destination.exists(), 'retain into a new evidence directory; preserve prior results')
    destination.mkdir()
    (destination / 'result.json').write_bytes(data)
    manifest = {'schema': 'fr-python-contract-evidence-1', 'collection': identity,
                'binary_sha256': value['binary_sha256'],
                'files': {'result.json': hashlib.sha256(data).hexdigest()}}
    (destination / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    tests = ROOT / 'tools/test-python-contract.py'
    text, count = re.subn(r"^REPORT = ROOT / '[^']+'$", f"REPORT = ROOT / '{relative}/result.json'",
                          tests.read_text(), flags=re.MULTILINE)
    contract.require(count == 1, 'expected one active contract test reference')
    tests.write_text(text)
    catalog_path = ROOT / 'tests/agent-eval/roadmap.json'
    catalog = json.loads(catalog_path.read_text())
    entries = [entry for entry in catalog['evidence'] if entry['id'] == 'python-contract']
    contract.require(len(entries) == 1, 'expected one contract evidence reference')
    entries[0].update(path=str(relative / 'result.json'), sha256=manifest['files']['result.json'],
                     audit=f'python3 tools/python-contract.py --audit {relative}/result.json',
                     scope=f"{result['rules']} declared Python rules; {result['examples']} authored programs and "
                           f"{result['cases']} original/renamed cases, including {result['refused_cases']} CLI refusals. "
                           'Finite B.contract inventory; roadmap acceptance requires separate review.')
    catalog_path.write_text(json.dumps(catalog, indent=2, ensure_ascii=False) + '\n')
    # The existing 120-second SDK wrapper deadline and all CI job deadlines stay
    # unchanged. Run corruption controls against the exact newly retained bytes.
    subprocess.run(['python3', '-B', 'tools/test-python-contract.py'], cwd=ROOT, check=True, timeout=120)
    command(['python3', '-B', 'tools/roadmap-status.py', '--write'])
    command(['python3', '-B', 'tools/roadmap-status.py', '--check'])
    command(['python3', '-B', 'tests/agent-eval/test_roadmap_status.py'])
    print(json.dumps({'retained': str(relative), 'collection': identity, 'summary': result}, sort_keys=True))


if __name__ == '__main__':
    main()
