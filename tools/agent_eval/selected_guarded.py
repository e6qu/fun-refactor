"""Observe a guarded algorithm attempt without being paused by its CPU throttle."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import time

from . import bounded_host as host, client_memory_profile, selected_local as local
from .selected_plan import ALGORITHM_STUDIES, ROOT, require

GUARD = Path('/Users/zardoz/.codex/tools/fr-local-guard.py')


class IndependentSampler:
    def __init__(self, directory):
        self.directory = directory
        self.started = host.elapsed_time()
        self.written = 0
        self.peak = {'rss_bytes': 0, 'processes': []}

    def __call__(self, group):
        raw = subprocess.run(['ps', '-axo', 'pid=,ppid=,pgid=,rss=,time=,comm='],
                             capture_output=True, text=True, check=True, timeout=2).stdout
        rows = client_memory_profile.processes(raw, group)
        memory = sum(row['rss_bytes'] for row in rows)
        cpu = {row['pid']: row['cpu_seconds'] for row in rows}
        data = json.dumps([host.elapsed_time() - self.started, memory, list(cpu.items())],
                          separators=(',', ':'), allow_nan=False).encode() + b'\n'
        require(self.written + len(data) <= client_memory_profile.MAX_TRACE, 'independent resource trace exceeds budget')
        with (self.directory / 'samples.jsonl').open('ab') as stream:
            stream.write(data)
        self.written += len(data)
        if memory > self.peak['rss_bytes']:
            self.peak = {'rss_bytes': memory, 'processes': rows}
        return memory, cpu


def contract():
    return {'guard': str(GUARD), 'guard_sha256': local.hash_file(GUARD),
            'observer': 'outside the throttled workload; ps descendants, 100 ms target interval',
            'scope': 'Guard, controller, CLI and workers, including preparation and accounting.'}


def attach(record, resources, monitor):
    record['complete_attempt_resources'] = resources
    record['complete_attempt_resources_sha256'] = local.hash_file(monitor / 'resources.json')
    record['elapsed_seconds'] = resources['elapsed_seconds']
    record['suspended_seconds'] = resources['suspended_seconds']
    record['complete_attempt_deadline_missed'] = resources['elapsed_seconds'] > local.LIMITS['wall_seconds']
    if record['status'] == 'started':
        record.update(status='failed', finished_at=time.time())
        record['controller_interrupted'] = True
        if resources['stop_reason'] != 'wall_seconds':
            record['collection_errors'].append({'stage': 'complete attempt', 'error': 'Guarded controller interrupted.'})
    if resources['stop_reason'] is not None or resources['exit_code'] != 0 or record['complete_attempt_deadline_missed']:
        record['status'] = 'failed'
    return record


def attempt(plan_path, attempt_id):
    require(sys.platform == 'darwin', 'the workstation guard is local to macOS')
    plan = json.loads(plan_path.read_text())
    milestone = plan['schedule']['milestone']
    require(milestone in ALGORITHM_STUDIES and plan['resource_monitor'] == contract(),
            'freeze this guard and independent observer before calls')
    require(plan['implementation'] == local.implementation_bindings(milestone), 'monitor or study code changed')
    require(attempt_id in {row['id'] for row in plan['schedule']['attempts']}, 'unknown frozen attempt')
    monitor = plan_path.parent / 'attempt-monitors' / attempt_id
    monitor.mkdir(parents=True, exist_ok=False)
    project_parent = Path('/private/tmp') / ('fr-' + milestone.lower() + '-' + plan['sha256'][:12]) / attempt_id
    project_parent.mkdir(parents=True, exist_ok=False)
    command = ['python3', str(GUARD), sys.executable, '-B', '-m', 'agent_eval.selected_local',
               'attempt', '--plan', str(plan_path), '--id', attempt_id]
    env = dict(os.environ, PYTHONPATH=str(ROOT / 'tools') + os.pathsep + str(ROOT / 'sdk/python/src'),
               PYTHONDONTWRITEBYTECODE='1')
    os.nice(15)
    previous = host.sample
    sampler = IndependentSampler(monitor)
    host.sample = sampler
    try:
        with (monitor / 'stdout').open('xb') as stdout, (monitor / 'stderr').open('xb') as stderr:
            awake_started = time.monotonic()
            resources = host.run(command, b'', stdout, stderr, project_parent,
                                 env=env, cwd=ROOT, **local.LIMITS)
            resources['suspended_seconds'] = max(0, resources['elapsed_seconds'] - (time.monotonic() - awake_started))
    finally:
        host.sample = previous
    local.tasks.save(monitor / 'resources.json', resources)
    local.tasks.save(monitor / 'peak.json', sampler.peak)
    output = plan_path.parent / 'attempts' / attempt_id
    record_path = output / 'result.json'
    require(record_path.is_file(), 'guard refused or controller failed before starting; inspect private monitor records')
    record = json.loads(record_path.read_text())
    local.tasks.save(output / 'controller-result.json', record)
    record = attach(record, resources, monitor)
    stream = output / 'native.stdout'
    record['native_stdout_sha256'] = local.hash_file(stream) if stream.is_file() else None
    local.tasks.save(record_path, record)
    return {key: record[key] for key in ('id', 'status', 'elapsed_seconds', 'patch_delivered')}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--plan', type=Path, required=True)
    parser.add_argument('--id', required=True)
    args = parser.parse_args()
    print(json.dumps(attempt(args.plan.resolve(), args.id)), flush=True)
