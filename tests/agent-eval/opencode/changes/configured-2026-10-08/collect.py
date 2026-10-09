#!/usr/bin/env python3
"""Freeze one admitted four-attempt pilot; collect each committed cell at most once."""
import argparse
import base64
import gzip
import hashlib
from pathlib import Path
import runpy
import subprocess
import sys
import zipfile

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[4]
sys.path.insert(0, str(REPO / 'tools'))
from agent_eval import native_checks, rehearsal_evidence, source_reviews
from agent_eval import terminal_changes as changes, terminal_change_runner as runner
from agent_eval.study import digest, encode, load, require

CANDIDATES = REPO / 'tests/agent-eval/opencode/candidates'
ADMISSION = REPO / 'tests/agent-eval/opencode/reviews/2026-10-07-packaging-scoped/admission.json'
CATALOG = REPO / 'tests/agent-eval/opencode/catalog/2026-10-07/catalog.json'
TASK = 'packaging-prerelease'


def bindings():
    paths = [Path(__file__), HERE / 'public-baseline.zip', HERE / 'public-baseline-provenance.json',
             ADMISSION, CATALOG, *[CANDIDATES / n for n in ('manifest.json', 'packaging.tar.gz',
                'packaging-prerelease-grader.json', 'public-checks.json')],
             *[REPO / 'tools' / n for n in ('terminal-change-snapshot.py', 'check-change-workstation.py',
                                           'check-terminal-changes.py')]]
    paths += [p for folder in ('hosted', 'workstation') for p in (HERE / folder).rglob('*') if p.is_file()]
    return {str(p.relative_to(REPO)): source_reviews.identity(p) for p in sorted(paths)}


def freeze(binary, client):
    require(not (HERE / 'plan.json').exists(), 'pilot already frozen')
    require(load(ADMISSION)['task_accepted_for_bounded_pilot'], 'task not admitted')
    workstation = runpy.run_path(str(REPO / 'tools/check-change-workstation.py'))
    checker = workstation['hosted'](HERE / 'hosted', binary, client)
    local = checker['report'](HERE / 'workstation')
    admission = load(HERE / 'workstation/admission.json')
    require(local == load(HERE / 'workstation/result.json') and local['headroom_admitted']
            and local['record']['status'] == 'completed' and admission['admitted'], 'workstation not admitted')
    require(admission['hosted_manifest_sha256'] == source_reviews.identity(HERE / 'hosted/provenance.json')
            and admission['result_sha256'] == digest(local), 'workstation evidence differs')
    local_plan = load(HERE / 'workstation/plan.json')['plan']
    require(local_plan['runtime'] == changes.implementation()
            and local_plan['binary_sha256'] == source_reviews.identity(binary)
            and local_plan['opencode_sha256'] == source_reviews.identity(client), 'workstation runtime differs')
    task = next(t for t in load(CANDIDATES / 'manifest.json')['tasks'] if t['id'] == TASK)
    files = rehearsal_evidence.source_bundle(task, CANDIDATES)
    archive = HERE / 'public-baseline.zip'
    baseline_provenance = load(HERE / 'public-baseline-provenance.json')
    require(source_reviews.identity(archive) == baseline_provenance['sha256'], 'baseline archive differs')
    with zipfile.ZipFile(archive) as source:
        require(source.namelist() == ['controls.json'] and source.getinfo('controls.json').file_size < 1024**2,
                'invalid baseline artifact')
        import json
        original = next(r for r in json.loads(source.read('controls.json'))['results'] if r['id'] == TASK + '/unchanged')
    public = load(CANDIDATES / 'public-checks.json')[TASK]
    grade = original['public_grade']
    require(original['repository_revision'] == task['revision'] and original['submission_sha256'] == digest(files),
            'baseline source differs')
    require(not native_checks.verify_grade(public, digest(public), native_checks.candidate_identity(files), grade),
            'original public check unexpectedly passes')
    feedback = encode({'command': public['command'], 'expected_stdout': public['cases'][0]['stdout'],
                       'expected_exit_code': 0, 'baseline_exit_code': grade['cases'][0]['container_state']['ExitCode'],
                       'baseline_stdout': base64.b64decode(grade['cases'][0]['stdout_base64']).decode(),
                       'baseline_stderr': base64.b64decode(grade['cases'][0]['stderr_base64']).decode()}).decode()
    models = [{'providerID': provider, 'modelID': model, 'configured': True, 'variant': 'low',
               'context': 32768, 'output': 2048} for provider, model in
              [('kimi-code-plan-global', 'k3'), ('zai-coding-plan', 'glm-5.3-flash')]]
    frozen, snapshots = changes.freeze([{'id': TASK, 'requirement': task['requirement'], 'files': files,
        'public_feedback': feedback, 'grader': load(CANDIDATES / task['grader'])}], models, binary, client,
        load(CATALOG), {'bindings': bindings(), 'repository': task['repository'], 'revision': task['revision'],
        'scope': 'One familiar admitted task, four attempts; not a general efficiency experiment.',
        'stop_rule': 'First resource failure or two consecutive capture failures; no retries.',
        'guard_sha256': source_reviews.identity(Path('/Users/zardoz/.codex/tools/fr-local-guard.py'))})
    (HERE / 'plan.json').write_bytes(encode(frozen))
    (HERE / 'inputs.json.gz').write_bytes(gzip.compress(encode(snapshots), mtime=0))
    snapshot = runpy.run_path(str(REPO / 'tools/terminal-change-snapshot.py'))
    snapshot['retain'](frozen, HERE / 'runner')
    print(encode({'plan_sha256': frozen['sha256'], 'cells': frozen['plan']['cells']}).decode())


def collect(cell, binary, client):
    require(not (HERE / 'stop.json').exists(), 'pilot permanently stopped')
    frozen = load(HERE / 'plan.json')
    require(frozen['plan']['provenance']['bindings'] == bindings(), 'pilot inputs changed')
    paths = [*bindings(), *[str((HERE / n).relative_to(REPO)) for n in ('plan.json', 'inputs.json.gz')],
             *[str(p.relative_to(REPO)) for p in (HERE / 'runner').rglob('*.py')]]
    for path in paths:
        committed = subprocess.run(['git', 'show', 'HEAD:' + path], cwd=REPO, capture_output=True, check=True).stdout
        require(committed == (REPO / path).read_bytes(), 'commit exact frozen inputs before calls')
    snapshots = source_reviews.read_inputs(HERE)
    record = runner.collect(frozen, snapshots, cell, HERE, HERE / 'attempts', binary, client)
    report = changes.report(frozen, snapshots, HERE / 'attempts')
    (HERE / 'collection.json').write_bytes(encode(report))
    started = [r for r in report['attempts'] if r['status'] != 'not_started']
    reason = record['process']['stop_reason'] if 'process' in record else started[-1]['process']['stop_reason']
    if reason is not None or (len(started) >= 2 and all(r['status'] == 'failed' for r in started[-2:])):
        (HERE / 'stop.json').write_bytes(encode({'cell': cell, 'reason': reason or 'two_consecutive_failures', 'resume_allowed': False}))
    print(encode({'cell': cell, 'status': record['status'], 'failure': record['failure'],
                  'process': started[-1]['process']}).decode())


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('command', choices=('freeze', 'collect'))
    parser.add_argument('--cell')
    parser.add_argument('--fr', type=Path, required=True)
    parser.add_argument('--opencode', type=Path, required=True)
    parser.add_argument('--confirm-agent-spend', action='store_true')
    args = parser.parse_args()
    if args.command == 'freeze':
        freeze(args.fr.resolve(), args.opencode.resolve())
    else:
        require(args.confirm_agent_spend and args.cell, 'select one cell and confirm spend')
        collect(args.cell, args.fr.resolve(), args.opencode.resolve())
