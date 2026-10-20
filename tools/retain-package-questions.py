#!/usr/bin/env python3
"""Save the first frozen package comparisons, including unsuccessful attempts."""
import argparse
from datetime import datetime
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[1]


def load(name):
    spec = importlib.util.spec_from_file_location(name, ROOT / 'tools' / (name + '.py'))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


scalar = load('retain-scalar-evidence')
questions = load('package-questions')
require = scalar.require


def checked_collection(run, artifact, jobs, data, repository, phase):
    require(run['status'] == 'completed' and run['conclusion'] in ('success', 'failure')
            and run['run_attempt'] == 1
            and run['path'] == '.github/workflows/refinement-evidence.yml'
            and run['event'] == 'workflow_dispatch'
            and run['repository']['full_name'] == repository
            and run['head_repository']['full_name'] == repository,
            'retain a completed same-repository package collection')
    require(artifact['name'] == 'package-questions' and artifact['expired'] is False
            and artifact['workflow_run']['id'] == run['id']
            and artifact['workflow_run']['head_sha'] == run['head_sha'],
            'artifact belongs to a different package collection')
    producers = [job for job in jobs if job['name'] == 'package-questions'
                 and job['run_id'] == run['id'] and job['status'] == 'completed'
                 and job['conclusion'] == run['conclusion']
                 and job['run_attempt'] == run['run_attempt']
                 and datetime.fromisoformat(job['started_at']) <= datetime.fromisoformat(artifact['created_at'])
                 <= datetime.fromisoformat(job['completed_at'])]
    require(len(producers) == 1, 'need one matching package collection job')
    value = json.loads(data['result.json'])
    require(value['source_commit'] == run['head_sha'] and value['phase'] == phase,
            'package report source or phase differs from its collection')
    require(not value['milestones_complete'] or run['conclusion'] == 'success',
            'a failed collection cannot establish completed milestones')
    return value, producers[0]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--coverage-run', required=True, type=int)
    parser.add_argument('--held-out-run', required=True, type=int)
    parser.add_argument('--prefix', required=True)
    args = parser.parse_args()
    require(os.environ.get('GITHUB_ACTIONS') == 'true', 'retain package evidence on GitHub runners')
    branch = os.environ.get('GITHUB_REF', '')
    require(branch.startswith('refs/heads/') and branch != 'refs/heads/main', 'retain into a working branch')
    require(re.fullmatch(r'[a-zA-Z0-9][a-zA-Z0-9_-]{0,79}', args.prefix), 'supply a plain dated prefix')
    require(args.coverage_run > 0 and args.held_out_run > 0 and args.coverage_run != args.held_out_run,
            'declare distinct coverage and unfamiliar-project collections')
    repository = os.environ['GITHUB_REPOSITORY']
    frozen = json.loads((ROOT / 'tests/agent-eval/package-questions/freeze.json').read_text())
    questions.flow.verify_analyzer(ROOT, frozen)
    require(frozen['plan_sha256'] == questions.flow.sha((ROOT / 'tools/package-question-projects.json').read_bytes())
            and frozen['collector_sha256'] == questions.flow.sha((ROOT / 'tools/package-questions.py').read_bytes()),
            'retain against the original frozen collector and project plan')
    collected = []
    for phase, run_id in [('coverage', args.coverage_run), ('held-out', args.held_out_run)]:
        run = scalar.retain.api(f'repos/{repository}/actions/runs/{run_id}')
        artifacts = scalar.retain.api(f'repos/{repository}/actions/runs/{run_id}/artifacts?per_page=100')['artifacts']
        matches = [item for item in artifacts if item['name'] == 'package-questions' and not item['expired']]
        require(len(matches) == 1 and matches[0]['size_in_bytes'] <= 16 * 1024**2,
                'need one bounded original package artifact')
        artifact = matches[0]
        archive = scalar.retain.download_artifact(repository, artifact['id'])
        data = scalar.files(archive, artifact['digest'])
        jobs = scalar.retain.api(f'repos/{repository}/actions/runs/{run_id}/jobs?filter=all&per_page=100')['jobs']
        value, producer = checked_collection(run, artifact, jobs, data, repository, phase)
        require(value['implementation'] == frozen and value['plan'] == questions.PLAN
                and value['limits'] == questions.LIMITS, 'collection differs from the recorded comparison')
        collected.append((phase, run, artifact, producer, archive, data, value))
    _, coverage_run, _, _, _, _, coverage = collected[0]
    _, held_run, _, _, _, held_data, held = collected[1]
    questions.checked_coverage(coverage_run, coverage, held['source_commit'], frozen)
    require(held['coverage_run'] == str(args.coverage_run)
            and json.loads(held_data['coverage-result.json']) == coverage,
            'unfamiliar-project collection did not use the declared familiar-project results')
    scalar.retain.command(['git', 'fetch', '--no-tags', '--depth=1', 'origin', held_run['head_sha']],
                          capture_output=True)
    scalar.verify_sources(ROOT, {held_run['head_sha']})
    for phase, run, artifact, producer, archive, data, value in collected:
        directory = ROOT / 'tests/agent-eval/results' / (args.prefix + '-package-questions-' + phase)
        require(not directory.exists(), 'preserve previous package attempts; choose a new prefix')
        directory.mkdir(parents=True)
        (directory / 'artifact.zip').write_bytes(archive)
        (directory / 'result.json').write_bytes(data['result.json'])
        receipt = {'run_id': run['id'], 'run_url': run['html_url'], 'attempt': run['run_attempt'],
                   'source_commit': run['head_sha'], 'conclusion': run['conclusion'],
                   'producer_job_id': producer['id'], 'artifact_id': artifact['id'],
                   'artifact_digest': artifact['digest'],
                   'files': {name: hashlib.sha256(content).hexdigest() for name, content in
                             [('artifact.zip', archive), ('result.json', data['result.json'])]},
                   'scope': 'Original collection record, including failures; retention does not rerun the analysis.'}
        (directory / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
        print(json.dumps({'phase': phase, 'conclusion': run['conclusion'],
                          'milestones_complete': value['milestones_complete']}), flush=True)


if __name__ == '__main__':
    main()
