#!/usr/bin/env python3
"""Retain a frozen coverage/held-out cohort and its historical collector failure."""
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
    spec = importlib.util.spec_from_file_location(name.replace('-', '_'), ROOT / 'tools' / (name + '.py'))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


retain = load('retain-python-contract')
flow = load('python-repository-flow')
require = flow.require
SCHEMA = 'fr-python-repository-flow-cohort-result-1'


def download(run_id, repository):
    run = retain.api(f'repos/{repository}/actions/runs/{run_id}')
    require(run['status'] == 'completed' and run['path'] == '.github/workflows/refinement-evidence.yml'
            and run['repository']['full_name'] == repository and run['head_repository']['full_name'] == repository,
            'collection must be finished in the same repository')
    artifacts = retain.api(f'repos/{repository}/actions/runs/{run_id}/artifacts?per_page=100')['artifacts']
    candidates = [item for item in artifacts if item['name'].startswith('repository-flow-') and not item['expired']]
    require(len(candidates) == 1 and candidates[0]['size_in_bytes'] <= 16 * 1024 * 1024,
            'need one bounded current cohort artifact')
    artifact = candidates[0]
    with tempfile.TemporaryFile() as output:
        retain.command(['gh', 'api', f'repos/{repository}/actions/artifacts/{artifact["id"]}/zip'], stdout=output)
        output.seek(0)
        archive = output.read(16 * 1024 * 1024 + 1)
    require(len(archive) <= 16 * 1024 * 1024, 'artifact exceeds download admission')
    jobs = retain.api(f'repos/{repository}/actions/runs/{run_id}/jobs?filter=all&per_page=100')['jobs']
    return run, artifact, archive, jobs


def phase(run_id, name, repository):
    run, artifact, archive, jobs = download(run_id, repository)
    projects = [p for p in flow.COHORT['projects'] if p['role'] == name]
    names = ['result.json'] + [p['id'] + '.tar.gz' for p in projects]
    files = retain.checked_reports(archive, artifact['digest'], names)
    value = json.loads(files['result.json'])
    require(value['phase'] == name, 'collection phase changed')
    identity = retain.collection(run, artifact, value, repository, jobs, 'repository-flow-' + name, name)
    with tempfile.TemporaryDirectory() as temporary:
        directory = Path(temporary)
        for filename, data in files.items():
            (directory / filename).write_bytes(data)
        flow.audit(value, directory)
    return files, value, identity


def failure(run_id, repository):
    run, artifact, archive, jobs = download(run_id, repository)
    require(run['conclusion'] == 'failure' and artifact['name'] == 'repository-flow-coverage'
            and artifact['workflow_run']['id'] == run['id']
            and artifact['workflow_run']['head_sha'] == run['head_sha'], 'historical failure identity changed')
    files = retain.checked_reports(archive, artifact['digest'], ['result.json', 'toolz.tar.gz'])
    value = json.loads(files['result.json'])
    require(value['source_commit'] == run['head_sha'] and value['phase'] == 'coverage'
            and value['cohort']['budgets']['depth'] == 64 and value['summary'] == {}
            and len(value['cases']) == 1 and value['cases'][0]['invocations'][1]['exit_code'] != 0
            and 'invalid analysis budget' in json.dumps(value['cases'][0]['invocations'][1]['report']),
            'preserve the actual failed collection without promoting it')
    created = datetime.fromisoformat(artifact['created_at'])
    producers = [job for job in jobs if job['name'] == 'coverage' and job['run_id'] == run_id
                 and job['conclusion'] == 'failure'
                 and datetime.fromisoformat(job['started_at']) <= created <= datetime.fromisoformat(job['completed_at'])]
    require(len(producers) == 1, 'failure artifact producer changed')
    identity = {'run_id': run_id, 'attempt': producers[0]['run_attempt'], 'producer_job_id': producers[0]['id'],
                'source_commit': run['head_sha'], 'url': run['html_url'], 'conclusion': 'failure',
                'artifact_id': artifact['id'], 'artifact_digest': artifact['digest']}
    return files, identity


def checked_cohort(value, directory):
    require(value['schema'] == SCHEMA and value['source_bindings'] == flow.bindings(), 'cohort source changed')
    reports = {}
    for name in ('coverage', 'held-out'):
        report_path = directory / name / 'result.json'
        reports[name] = json.loads(report_path.read_bytes())
        flow.audit(reports[name], report_path.parent)
        identity = value['collections'][name]
        require(identity['source_commit'] == reports[name]['source_commit'], 'retained collection source changed')
    require(reports['coverage']['source_commit'] == reports['held-out']['source_commit']
            and reports['held-out']['coverage']['run_id'] == int(value['collections']['coverage']['run_id']),
            'held-out collection was not admitted by this frozen coverage run')
    expected = flow.summary(reports['coverage']['cases'] + reports['held-out']['cases'])
    require(value['summary'] == expected and expected['repositories'] == 3, 'cohort accounting changed')
    return expected


def admission_failure(run_id, repository):
    run = retain.api(f'repos/{repository}/actions/runs/{run_id}')
    artifacts = retain.api(f'repos/{repository}/actions/runs/{run_id}/artifacts')['artifacts']
    jobs = retain.api(f'repos/{repository}/actions/runs/{run_id}/jobs')['jobs']
    require(run['status'] == 'completed' and run['conclusion'] == 'failure'
            and run['path'] == '.github/workflows/refinement-evidence.yml'
            and run['repository']['full_name'] == repository and run['head_repository']['full_name'] == repository
            and not artifacts, 'pre-download failure identity changed')
    producers = [job for job in jobs if job['name'] == 'held-out' and job['conclusion'] == 'failure']
    require(len(producers) == 1, 'pre-download failure producer changed')
    logs = retain.command(['gh', 'run', 'view', str(run_id), '--repo', repository, '--log-failed'],
                          capture_output=True, text=True).stdout
    lines = [line.split('FileExistsError:', 1)[1].strip() for line in logs.splitlines() if 'FileExistsError:' in line]
    require(len(lines) == 1 and 'target/python-repository-flow' in lines[0], 'pre-download failure reason changed')
    return {'run_id': run_id, 'source_commit': run['head_sha'], 'url': run['html_url'],
            'attempt': producers[0]['run_attempt'], 'producer_job_id': producers[0]['id'],
            'conclusion': 'failure', 'artifacts': [], 'reason': lines[0]}


def audit(path):
    value = json.loads(path.read_bytes())
    manifest = json.loads((path.parent / 'manifest.json').read_bytes())
    expected = {'result.json', 'historical-failure/result.json', 'historical-failure/toolz.tar.gz'}
    for phase in ('coverage', 'held-out'):
        expected.add(phase + '/result.json')
        expected.update(phase + '/' + project['id'] + '.tar.gz'
                        for project in flow.COHORT['projects'] if project['role'] == phase)
    require(set(manifest['files']) == expected, 'retained artifact inventory changed')
    for name, digest in manifest['files'].items():
        file = (path.parent / name).resolve()
        require(file.is_relative_to(path.parent.resolve()) and hashlib.sha256(file.read_bytes()).hexdigest() == digest,
                'retained cohort artifact digest changed')
    require(manifest['schema'] == 'fr-python-repository-flow-evidence-1'
            and manifest['collections'] == value['collections']
            and manifest['historical_failure'] == value['historical_failure']
            and value['historical_failure']['conclusion'] == 'failure', 'retained provenance changed')
    return checked_cohort(value, path.parent)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--coverage-run', type=int)
    parser.add_argument('--held-out-run', type=int)
    parser.add_argument('--failure-run', type=int)
    parser.add_argument('--admission-failure-run', type=int)
    parser.add_argument('--prefix')
    parser.add_argument('--audit', type=Path)
    args = parser.parse_args()
    if args.audit:
        print(json.dumps(audit(args.audit), sort_keys=True))
        return
    require(os.environ.get('GITHUB_ACTIONS') == 'true', 'retain on GitHub runners')
    branch = os.environ.get('GITHUB_REF', '')
    require(branch.startswith('refs/heads/') and branch != 'refs/heads/main', 'retain into a working branch')
    require(args.coverage_run and args.held_out_run and args.failure_run and args.admission_failure_run
            and args.prefix and re.fullmatch(r'[a-zA-Z0-9][a-zA-Z0-9_-]{0,79}', args.prefix),
            'supply successful phase runs, the historical failure and a plain dated prefix')
    repository = os.environ['GITHUB_REPOSITORY']
    phases = {name: phase(run, name, repository)
              for name, run in [('coverage', args.coverage_run), ('held-out', args.held_out_run)]}
    historical_files, historical_identity = failure(args.failure_run, repository)
    admission_identity = admission_failure(args.admission_failure_run, repository)
    relative = Path('tests/agent-eval/results') / (args.prefix + '-repository-flow')
    directory = ROOT / relative
    require(not directory.exists(), 'preserve prior evidence; choose a new directory')
    directory.mkdir()
    for name, (files, _, _) in phases.items():
        (directory / name).mkdir()
        for filename, data in files.items():
            (directory / name / filename).write_bytes(data)
    (directory / 'historical-failure').mkdir()
    for filename, data in historical_files.items():
        (directory / 'historical-failure' / filename).write_bytes(data)
    reports = [item[1] for item in phases.values()]
    value = {'schema': SCHEMA, 'source_bindings': flow.bindings(),
             'collections': {name: item[2] for name, item in phases.items()},
             'historical_failure': historical_identity,
             'admission_failure': admission_identity,
             'summary': flow.summary([row for report in reports for row in report['cases']])}
    (directory / 'result.json').write_text(json.dumps(value, indent=2, sort_keys=True) + '\n')
    manifest = {'schema': 'fr-python-repository-flow-evidence-1', 'collections': value['collections'],
                'historical_failure': historical_identity,
                'files': {str(file.relative_to(directory)): hashlib.sha256(file.read_bytes()).hexdigest()
                          for file in directory.rglob('*') if file.is_file()}}
    (directory / 'manifest.json').write_text(json.dumps(manifest, indent=2, sort_keys=True) + '\n')
    result = audit(directory / 'result.json')
    subprocess.run(['python3', '-B', 'tools/test-retain-repository-flow.py'], cwd=ROOT, check=True, timeout=120,
                   env={**os.environ, 'FR_REPOSITORY_FLOW_COHORT': str(directory / 'result.json')})
    catalog_path = ROOT / 'tests/agent-eval/roadmap.json'
    catalog = json.loads(catalog_path.read_text())
    require(not any(entry['id'] == 'repository-flow' for entry in catalog['evidence']), 'cohort already retained')
    catalog['evidence'].append({'id': 'repository-flow', 'path': str(relative / 'result.json'),
                              'sha256': manifest['files']['result.json'], 'kind': 'deterministic',
                              'scope': 'Three pinned utility libraries; authored scalar adapters, runtime contrasts, '
                                       'renames, relocation, incomplete boundaries and a frozen held-out project. '
                                       'Initial probes; B.repository-flow remains open.',
                              'audit': f'python3 tools/retain-repository-flow.py --audit {relative}/result.json'})
    catalog_path.write_text(json.dumps(catalog, indent=2, ensure_ascii=False) + '\n')
    retain.command(['python3', '-B', 'tools/roadmap-status.py', '--write'])
    retain.command(['python3', '-B', 'tools/roadmap-status.py', '--check'])
    retain.command(['python3', '-B', 'tests/agent-eval/test_roadmap_status.py'])
    boundaries = {project['id']: sorted({cutoff for report in reports for row in report['cases']
                                        if row['project'] == project['id']
                                        for cutoff in row['invocations'][1]['report'].get('cutoffs', [])})
                  for project in flow.COHORT['projects']}
    print(json.dumps({'retained': str(relative), 'summary': result, 'boundaries': boundaries}, sort_keys=True))


if __name__ == '__main__':
    main()
