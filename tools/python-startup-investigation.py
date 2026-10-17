"""Compare observed Python startup with unchanged, incomplete analyzer reports."""
import argparse
import hashlib
import importlib.util
import json
import math
import os
from pathlib import Path
import platform
import random
import subprocess
import sys
import tempfile

from agent_eval.bounded_host import run

ROOT = Path(__file__).resolve().parents[1]
PLAN_PATH = ROOT / 'tests/agent-eval/python-startup/plan.json'
PLAN = json.loads(PLAN_PATH.read_text())
LIMITS = dict(wall_seconds=20, rss_bytes=512 * 1024**2, disk_bytes=16 * 1024**2,
              cpu_limit_seconds=10, transcript_bytes=1024**2)
SOURCES = ('tools/python-startup-investigation.py', 'tools/python-startup-worker.py',
           'tools/test-python-startup.py', 'tools/agent_eval/bounded_host.py',
           'tools/agent_eval/study.py', '.github/workflows/python-startup.yml',
           'tests/agent-eval/python-startup/plan.json', 'docs/python-package-startup.md')
spec = importlib.util.spec_from_file_location('repository_flow', ROOT / 'tools/python-repository-flow.py')
flow = importlib.util.module_from_spec(spec)
spec.loader.exec_module(flow)


def require(condition, message):
    if not condition:
        raise ValueError(message)


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def save(path, value):
    temporary = path.with_suffix('.tmp')
    temporary.write_text(json.dumps(value, indent=2, sort_keys=True) + '\n')
    temporary.replace(path)


def bindings():
    return dict(flow.bindings(), **{name: digest(ROOT / name) for name in SOURCES})


def reference():
    directory = ROOT / PLAN['report_directory']
    require(digest(directory / 'result.json') == PLAN['report_sha256'], 'saved analyzer report changed')
    value = json.loads((directory / 'result.json').read_bytes())
    selected = {}
    for helper in PLAN['helpers']:
        rows = [row for row in value['declaration_cases'] if row['helper'] == helper['id']
                and row['context'] == 'full-package' and row['variant'] == 'original' and row['case'] == 'positive']
        require(len(rows) == 1, 'saved helper selection must be unique')
        row = rows[0]
        report = row['invocations'][1]['report']
        require(row['assessment']['status'] == 'incomplete' and report['complete'] is False,
                'the recorded package boundary changed')
        selected[helper['id']] = report
    return directory, value, selected


def checked_observation(observation, helper, traced, scope):
    require(set(observation) == {'module', 'function', 'arguments', 'returned', 'defining_path', 'traced',
            'import_seconds', 'call_seconds', 'modules', 'files', 'events', 'event_limit', 'execution'},
            'observation fields changed')
    require(observation['module'] == helper['module'] and observation['function'] == helper['function']
            and observation['arguments'] == helper['arguments']
            and json.dumps(observation['returned']) == json.dumps(helper['expected'])
            and observation['defining_path'] == helper['path'] and observation['traced'] is traced,
            'helper identity or ordinary Python result changed')
    require(type(observation['events']) is int and observation['event_limit'] == 200000
            and 0 <= observation['events'] <= observation['event_limit'], 'trace event budget changed')
    require(isinstance(observation['modules'], dict) and observation['modules']
            and isinstance(observation['files'], dict) and observation['files'], 'missing package source inventory')
    for path, identity in observation['files'].items():
        candidate = Path(path)
        require(not candidate.is_absolute() and '..' not in candidate.parts and candidate.suffix == '.py',
                'observed path escapes the pinned source')
        file = scope / candidate
        require(identity == {'bytes': file.stat().st_size, 'sha256': digest(file)}, 'observed source identity changed')
    require(set(observation['modules'].values()) <= set(observation['files'])
            and observation['modules'].get(helper['module']) == helper['path'], 'imported module selection changed')
    require(type(observation['execution']) is list, 'execution inventory must be a list')
    seen = set()
    for item in observation['execution']:
        require(set(item) == {'phase', 'path', 'lines', 'module_lines', 'calls'}
                and item['phase'] in ('import', 'call') and item['path'] in observation['files'],
                'malformed executed file')
        key = item['phase'], item['path']
        require(key not in seen, 'duplicate executed file')
        seen.add(key)
        line_count = len((scope / item['path']).read_bytes().splitlines())
        for name in ('lines', 'module_lines'):
            lines = item[name]
            require(type(lines) is list and all(type(line) is int and 1 <= line <= line_count for line in lines)
                    and lines == sorted(set(lines)), 'invalid observed source lines')
        require(set(item['module_lines']) <= set(item['lines']) and isinstance(item['calls'], dict)
                and all(type(count) is int and count > 0 for count in item['calls'].values()),
                'invalid call or module-line inventory')
    require((observation['events'] > 0 and bool(observation['execution'])) if traced else
            (observation['events'] == 0 and observation['execution'] == []), 'missing trace or unexpected instrumentation')
    for name in ('import_seconds', 'call_seconds'):
        require(type(observation[name]) in (int, float) and 0 <= observation[name] <= LIMITS['wall_seconds'],
                'invalid measured duration')
    return observation


def comparison(observation, report):
    recorded = set(report['inputs']['modules']['files'])
    observed = set(observation['files'])
    import_execution = {item['path']: item for item in observation['execution'] if item['phase'] == 'import'}
    shown = []
    for site in report['initialization']['sites']:
        occurrence = site['occurrence']
        path = occurrence['path']
        location = occurrence['location']['range']
        lines = import_execution.get(path, {}).get('module_lines', [])
        shown.append({'path': path, 'start_line': location['start']['line'], 'reasons': site['reasons'],
                      'module_line_observed': any(location['start']['line'] <= line <= location['end']['line']
                                                  for line in lines)})
    return {'analysis_complete': report['complete'], 'analysis_cutoffs': report['cutoffs'],
            'analyzer_files_read': len(recorded), 'observed_source_files': len(observed),
            'observed_source_bytes': sum(item['bytes'] for item in observation['files'].values()),
            'observed_but_unread': sorted(observed - recorded),
            'read_but_unobserved': sorted(recorded - observed),
            'unchecked_statements': sum(item['statements'] for item in report['initialization']['by_file'].values()),
            'shown_unchecked_sites': shown, 'unshown_unchecked_sites': report['initialization']['omitted'],
            'observation_scope': 'Only these imports and integer calls in this Python environment. Unobserved code remains unchecked.'}


def checked_process(measurement, stdout):
    require(measurement['stop_reason'] is None and measurement['exit_code'] == 0
            and measurement['process_exit_code'] == 0 and measurement['monitor_error'] is None
            and measurement['launch_error'] is None and measurement['timed_out'] is False,
            'observation stopped or failed; retain the attempt and investigate')
    require(measurement['limits'] == {'wall_seconds': LIMITS['wall_seconds'], 'rss_bytes': LIMITS['rss_bytes'],
            'disk_bytes': LIMITS['disk_bytes'], 'cpu_seconds': LIMITS['cpu_limit_seconds'],
            'transcript_bytes': LIMITS['transcript_bytes']}, 'host observation limits changed')
    for field, limit in (('elapsed_seconds', 'wall_seconds'), ('sampled_aggregate_rss_bytes', 'rss_bytes'),
                         ('sampled_disk_growth_bytes', 'disk_bytes'), ('sampled_cpu_seconds', 'cpu_limit_seconds'),
                         ('transcript_bytes', 'transcript_bytes')):
        number = measurement[field]
        require(type(number) in (int, float) and math.isfinite(number) and 0 <= number <= LIMITS[limit],
                'measurement exceeds its unchanged resource limits')
    return json.loads(stdout)


def schedule():
    cells = [(helper['id'], traced, repetition) for helper in PLAN['helpers']
             for traced in (False, True) for repetition in range(PLAN['repetitions'])]
    random.Random(PLAN['seed']).shuffle(cells)
    return cells


def prepare(directory):
    source, saved, reports = reference()
    scopes = {}
    for project in saved['cohort']['projects']:
        name = project['id']
        require(name in PLAN['archives'], 'unexpected project in startup investigation')
        archive = source / (name + '.tar.gz')
        require(digest(archive) == PLAN['archives'][name], 'pinned package archive changed')
        target = directory / name
        require(flow.snapshot(archive.read_bytes(), project, target) == saved['snapshots'][name]['files'],
                'pinned package inventory changed')
        scopes[name] = target / project['python_root']
    require(set(scopes) == set(PLAN['archives']), 'missing pinned package')
    return scopes, reports


def audit(directory):
    result = json.loads((directory / 'result.json').read_bytes())
    require(set(result) == {'source_commit', 'source_files', 'plan', 'python', 'platform', 'cpu_model',
            'logical_cpus', 'limits', 'measurement_scope', 'attempts', 'finished'}, 'collection fields changed')
    require(result['source_files'] == bindings() and result['plan'] == PLAN and result['limits'] == LIMITS,
            'collection inputs changed')
    require(result['finished'] is True and len(result['attempts']) == len(schedule()), 'collection did not finish')
    observations = {}
    with tempfile.TemporaryDirectory(prefix='fr-startup-audit-') as temporary:
        scopes, reports = prepare(Path(temporary))
        for index, (row, cell) in enumerate(zip(result['attempts'], schedule(), strict=True)):
            helper_id, traced, repetition = cell
            helper = next(item for item in PLAN['helpers'] if item['id'] == helper_id)
            require(set(row) == {'helper', 'traced', 'repetition', 'measurement', 'comparison', 'error',
                                'stdout_sha256', 'stderr_sha256'}
                    and (row['helper'], row['traced'], row['repetition']) == cell and row['error'] is None,
                    'missing, reordered or failed observation')
            stdout_path = directory / f'attempt-{index:02d}.stdout'
            stderr_path = directory / f'attempt-{index:02d}.stderr'
            require(digest(stdout_path) == row['stdout_sha256'] and digest(stderr_path) == row['stderr_sha256'],
                    'raw observation output changed')
            raw = stdout_path.read_text()
            observation = checked_observation(checked_process(row['measurement'], raw), helper, traced,
                                              scopes[helper['project']])
            require(row['comparison'] == comparison(observation, reports[helper_id]), 'comparison changed')
            observations[cell] = observation
    for helper in PLAN['helpers']:
        inventory = [value['modules'] for key, value in observations.items() if key[0] == helper['id']]
        require(all(item == inventory[0] for item in inventory), 'tracing or repeated imports changed loaded modules')
    return result


def collect(output):
    require(os.environ.get('GITHUB_ACTIONS') == 'true' and sys.platform == 'linux', 'collection belongs on GitHub runners')
    output.mkdir(parents=True, exist_ok=False)
    scopes, reports = prepare(output / 'upstream')
    result = {'source_commit': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
              'source_files': bindings(), 'plan': PLAN, 'python': sys.version, 'platform': platform.platform(),
              'cpu_model': next(line.split(':', 1)[1].strip() for line in Path('/proc/cpuinfo').read_text().splitlines()
                                if line.startswith('model name')), 'logical_cpus': os.cpu_count(), 'limits': LIMITS,
              'measurement_scope': 'Fresh Python process per import and three-call batch. Host elapsed includes startup, observation, output and monitoring. Import/call timers exclude inventory and serialization. Sampled CPU/RSS exclude the monitor. Checkout, dependency installation and source admission are outside these timers.',
              'attempts': [], 'finished': False}
    save(output / 'result.json', result)
    for index, (helper_id, traced, repetition) in enumerate(schedule()):
        helper = next(item for item in PLAN['helpers'] if item['id'] == helper_id)
        scope = scopes[helper['project']].resolve()
        command = [sys.executable, '-B', str(ROOT / 'tools/python-startup-worker.py'), '--root', str(scope),
                   '--module', helper['module'], '--function', helper['function'],
                   '--arguments', json.dumps(helper['arguments'])] + (['--traced'] if traced else [])
        env = {'PATH': os.environ['PATH'], 'HOME': str(output.resolve()), 'LANG': 'C.UTF-8',
               'PYTHONHASHSEED': '0', 'PYTHONDONTWRITEBYTECODE': '1'}
        stdout_path, stderr_path = output / f'attempt-{index:02d}.stdout', output / f'attempt-{index:02d}.stderr'
        with stdout_path.open('wb') as stdout, stderr_path.open('wb') as stderr:
            measurement = run(command, b'', stdout, stderr, output, env=env, cwd=scope, **LIMITS)
        row = {'helper': helper_id, 'traced': traced, 'repetition': repetition,
               'measurement': measurement, 'comparison': None, 'error': None,
               'stdout_sha256': digest(stdout_path), 'stderr_sha256': digest(stderr_path)}
        result['attempts'].append(row)
        save(output / 'result.json', result)
        try:
            observation = checked_observation(checked_process(measurement, stdout_path.read_text()), helper, traced, scope)
            row['comparison'] = comparison(observation, reports[helper_id])
        except (ValueError, KeyError, TypeError, OSError) as error:
            row['error'] = str(error)
        save(output / 'result.json', result)
        print(f'{index + 1}/{len(schedule())}: {helper_id}, trace={traced}, error={row["error"]}', flush=True)
    result['finished'] = all(row['error'] is None for row in result['attempts'])
    save(output / 'result.json', result)
    audit(output)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    selection = parser.add_mutually_exclusive_group(required=True)
    selection.add_argument('--output', type=Path)
    selection.add_argument('--audit', type=Path)
    args = parser.parse_args()
    if args.audit:
        result = audit(args.audit)
        print(f'Checked all {len(result["attempts"])} observations; package analysis remains unfinished.')
    else:
        collect(args.output.resolve())
