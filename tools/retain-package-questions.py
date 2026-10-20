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


def audit(path):
    receipt = json.loads((path.parent / 'receipt.json').read_text())
    require(set(receipt['files']) == {'artifact.zip', 'result.json'}, 'unexpected retained package files')
    for name, expected in receipt['files'].items():
        require(hashlib.sha256((path.parent / name).read_bytes()).hexdigest() == expected,
                'retained package file changed: ' + name)
    data = scalar.files((path.parent / 'artifact.zip').read_bytes(), receipt['artifact_digest'])
    require(data['result.json'] == path.read_bytes(), 'saved package result differs from its original artifact')
    value = json.loads(data['result.json'])
    require(value['schema'] == 'fr-original-package-questions' and value['phase'] == 'held-out'
            and value['source_bindings'] == questions.bindings() and value['plan'] == questions.PLAN
            and receipt['source_commit'] == value['source_commit'] and receipt['conclusion'] == 'success',
            'package completion no longer matches its declared inputs')
    frozen = json.loads((ROOT / 'tests/agent-eval/package-questions/freeze.json').read_text())
    questions.verify_analyzer(ROOT, frozen)
    require(value['implementation'] == frozen and value['limits'] == questions.LIMITS,
            'package implementation or resource limits changed')
    coverage = json.loads(data['coverage-result.json'])
    require(coverage['schema'] == 'fr-original-package-questions'
            and coverage['source_bindings'] == value['source_bindings'],
            'familiar and unfamiliar records used different evaluation inputs')
    coverage_run = json.loads(data['coverage-run.json'])['run']
    questions.checked_coverage(coverage_run, coverage, value['source_commit'], frozen, frozen['repository'])
    outcome = questions.checked_completion(value, coverage, frozen)
    for row in value['projects']:
        if row['complete'] is not True or row['error'] is not None:
            continue
        prefix = row['project']['id'] + '/'
        require(hashlib.sha256(data[prefix + 'source.tar.gz']).hexdigest() == row['archive_sha256'],
                'original package archive changed')
        for name, expected in row['source_files'].items():
            require(hashlib.sha256(data[prefix + 'upstream/' + name]).hexdigest() == expected,
                    'original package source changed: ' + name)
        for cell in row['cases']:
            name = cell['variant'] + '-' + cell['case']
            observations = [(name, cell['analysis'])] + [(f'{name}-python-{index}', item['observation'])
                            for index, item in enumerate(cell['runtime'])]
            for label, observation in observations:
                prefix = row['project']['id'] + '/' + label + '/'
                require(json.loads(data[prefix + 'observation.json']) == observation,
                        'package observation differs from its original record')
                for stream in ('stdout', 'stderr'):
                    require(hashlib.sha256(data[prefix + stream]).hexdigest() == observation[stream + '_sha256'],
                            'package worker output changed')
                require(json.loads(data[prefix + 'stdout']) == observation['result'],
                        'package answer differs from its worker output')
    return outcome


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
    parser.add_argument('--audit', type=Path)
    parser.add_argument('--coverage-run', type=int)
    parser.add_argument('--held-out-run', type=int)
    parser.add_argument('--prefix')
    args = parser.parse_args()
    if args.audit is not None:
        require(args.coverage_run is None and args.held_out_run is None and args.prefix is None,
                'choose audit or collection retention explicitly')
        print(json.dumps(audit(args.audit)))
        return
    require(args.coverage_run is not None and args.held_out_run is not None and args.prefix is not None,
            'declare both collection runs and a new dated prefix')
    require(os.environ.get('GITHUB_ACTIONS') == 'true', 'retain package evidence on GitHub runners')
    branch = os.environ.get('GITHUB_REF', '')
    require(branch.startswith('refs/heads/') and branch != 'refs/heads/main', 'retain into a working branch')
    require(re.fullmatch(r'[a-zA-Z0-9][a-zA-Z0-9_-]{0,79}', args.prefix), 'supply a plain dated prefix')
    require(args.coverage_run > 0 and args.held_out_run > 0 and args.coverage_run != args.held_out_run,
            'declare distinct coverage and unfamiliar-project collections')
    repository = os.environ['GITHUB_REPOSITORY']
    frozen = json.loads((ROOT / 'tests/agent-eval/package-questions/freeze.json').read_text())
    questions.verify_analyzer(ROOT, frozen)
    current = questions.bindings()
    require(frozen['collector_inputs'] == {name: current[name] for name in questions.COLLECTOR_INPUTS},
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
                and value['limits'] == questions.LIMITS and value['schema'] == 'fr-original-package-questions'
                and value['source_bindings'] == questions.bindings(), 'collection differs from the recorded comparison')
        collected.append((phase, run, artifact, producer, archive, data, value))
    _, coverage_run, _, _, _, _, coverage = collected[0]
    _, held_run, _, _, _, held_data, held = collected[1]
    questions.checked_coverage(coverage_run, coverage, held['source_commit'], frozen, repository)
    require(held['coverage_run'] == str(args.coverage_run)
            and json.loads(held_data['coverage-result.json']) == coverage,
            'unfamiliar-project collection did not use the declared familiar-project results')
    outcome = questions.milestone_outcome(held['projects'], coverage, 'held-out', frozen)
    require(all(held[key] == expected for key, expected in outcome.items()),
            'unfamiliar-project outcome differs from its recorded questions')
    if held_run['conclusion'] == 'success':
        questions.checked_completion(held, coverage, frozen)
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
    if held['milestones_complete']:
        catalog_path = ROOT / 'tests/agent-eval/roadmap.json'
        catalog = json.loads(catalog_path.read_text())
        evidence_id = 'original-package-questions'
        relative = f'tests/agent-eval/results/{args.prefix}-package-questions-held-out/result.json'
        entry = {'id': evidence_id, 'path': relative, 'kind': 'deterministic',
                 'audit': f'python3 tools/retain-package-questions.py --audit {relative}',
                 'scope': 'Three independent original packages, including the predeclared unread leap-year question; finite value routes, not general Python or agent savings.',
                 'sha256': hashlib.sha256(held_data['result.json']).hexdigest(),
                 'receipt_sha256': hashlib.sha256((ROOT / relative).with_name('receipt.json').read_bytes()).hexdigest()}
        require(not any(row['id'] == evidence_id for row in catalog['evidence']), 'retain the first milestone completion separately')
        catalog['evidence'].append(entry)
        obligation, = [row for row in catalog['obligations'] if row['id'] == 'B.repository-flow']
        obligation['requires'] = [{'evidence': evidence_id, 'assertions': [
            {'pointer': '/milestone_1_demonstrated', 'equals': True},
            {'pointer': '/milestone_2_demonstrated', 'equals': True},
            {'pointer': '/milestones_complete', 'equals': True},
            {'pointer': '/independent_projects', 'equals': held['independent_projects']},
        ]}]
        obligation['next'] = ('Keep the three original-package questions reproducible. '
                              'Broader project changes, effects and total agent costs remain separate requirements.')
        catalog_path.write_text(json.dumps(catalog, indent=2, ensure_ascii=False) + '\n')
        audit(ROOT / relative)
        scalar.retain.command(['python3', '-B', 'tools/roadmap-status.py', '--write'])
        scalar.retain.command(['python3', '-B', 'tools/roadmap-status.py', '--check'])
    with Path(os.environ['GITHUB_OUTPUT']).open('a') as output:
        output.write('milestones_complete=' + str(held['milestones_complete']).lower() + '\n')


if __name__ == '__main__':
    main()
