"""Compare one allocator setting without changing proof checks or admitting live work."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import subprocess
import tempfile
from unittest.mock import patch
import zipfile

from . import product_memory as memory
from .selected_plan import ROOT, encoded, require

PROTOCOL = ROOT / memory.tasks.plan.DIRECTORY / 'memory/allocator-protocol.json'
CONDITIONS = {'baseline': {}, 'immediate-purge': {'MIMALLOC_PURGE_DELAY': '0'}}
FIRST = [('baseline', 'reference', 0), ('immediate-purge', 'reference', 0)]
SECOND = [('immediate-purge', 'reference', 1), ('baseline', 'reference', 1)]
NEGATIVE = [('immediate-purge', 'unfinished', 0), ('immediate-purge', 'wrong-proof', 0)]


def policy():
    value = json.loads(PROTOCOL.read_text())
    require(value['schema'] == 'fr-proof-allocator-protocol-1' and
            value['qualification_run'] == '44542024683' and value['lean_version'] == '4.28.0' and
            value['conditions'] == CONDITIONS and value['limits'] == memory.runtime.CONTROL_LIMITS and
            value['first_pair'] == [row[0] for row in FIRST] and
            value['second_pair'] == [row[0] for row in SECOND] and
            value['negative_controls'] == [row[1] for row in NEGATIVE] and
            value['repeat_only_after_both_first_cases_finish'] is True and
            value['minimum_rss_reduction_fraction'] == 0.1 and
            value['admitted'] is False and value['live_model_calls'] == 0,
            'preserve the frozen allocator comparison and resource limits')
    return value


def environment(condition, lean_root, ambient=None):
    require(condition in CONDITIONS, 'unknown allocator condition')
    source = os.environ if ambient is None else ambient
    env = {key: value for key, value in source.items() if not key.upper().startswith('MIMALLOC_')}
    env.update(LEAN_NUM_THREADS='1', FR_LEAN_JOBS='1', **CONDITIONS[condition])
    env['PATH'] = str(lean_root / 'bin') + os.pathsep + env.get('PATH', '')
    return env


def identity(condition, case, repetition):
    return f'{condition}-{case}-{repetition}'


def guarded_finish(resources):
    limits = resources['limits']
    bounded = all(type(resources[name]) in (int, float) and 0 <= resources[name] <= limits[limit]
                  for name, limit in [('elapsed_seconds', 'wall_seconds'),
                      ('sampled_aggregate_rss_bytes', 'rss_bytes'), ('sampled_cpu_seconds', 'cpu_seconds'),
                      ('sampled_disk_growth_bytes', 'disk_bytes'), ('transcript_bytes', 'transcript_bytes')])
    return (bounded and resources['exit_code'] in (0, 1) and
            resources['process_exit_code'] == resources['exit_code'] and
            resources['timed_out'] is False and all(resources[key] is None for key in
            ('stop_reason', 'monitor_error', 'launch_error')))


def verdict(case, resources, output):
    if not guarded_finish(resources):
        return False
    if case == 'reference':
        return resources['exit_code'] == 0
    if resources['exit_code'] != 1:
        return False
    if case == 'unfinished':
        return 'sorry' in output and ('warning:' in output or 'error:' in output)
    require(case == 'wrong-proof', 'unknown proof control')
    return 'error:' in output and 'True.intro' in output and 'type mismatch' in output.lower()


def proof_control(project, case, inputs, seed):
    proof = project / memory.tasks.PROOF
    original = proof.read_text()
    if case == 'unfinished':
        with tempfile.TemporaryDirectory() as temporary:
            destination = Path(temporary) / 'original'
            memory.local.unpack(inputs / 'original.tar.gz', seed, destination)
            proof.write_bytes((destination / memory.tasks.PROOF).read_bytes())
    elif case == 'wrong-proof':
        start, end = list(re.finditer(r'^  -- fr:proof-(?:begin|end) clamp_bounds\n', original, re.M))
        proof.write_text(original[:start.end()] + '  exact True.intro\n' + original[end.start():])
    else:
        require(case == 'reference', 'unknown proof control')
    require(memory.tasks.masked(proof.read_text()) == memory.tasks.masked(original),
            'preserve imports, program, theorem and proof markers')
    if case != 'reference':
        require(proof.read_text() != original, 'negative control must change the proof')


def decision(rows):
    positive = [row for row in rows if row['case'] == 'reference']
    if (len(rows) != 6 or len(positive) != 4 or not all(row['passed'] for row in rows)):
        return {'outcome': 'not-demonstrated', 'admitted': False,
                'reason': 'Require both complete pairs and both rejected proof controls.'}
    peaks = {condition: [row['resources']['sampled_aggregate_rss_bytes'] for row in positive
                        if row['condition'] == condition] for condition in CONDITIONS}
    baseline, candidate = min(peaks['baseline']), max(peaks['immediate-purge'])
    reduction = 1 - candidate / baseline if baseline else 0
    return {'outcome': 'candidate' if baseline > 0 and candidate * 10 <= baseline * 9 else 'discard', 'admitted': False,
            'minimum_baseline_rss_bytes': baseline, 'maximum_candidate_rss_bytes': candidate,
            'conservative_rss_reduction_fraction': reduction,
            'reason': 'Checker-only comparison; complete native admission remains required.'}


def expected_schedule(rows):
    require(len(rows) >= 2, 'retain both first conditions including their failures')
    first_passed = all(row['passed'] for row in rows[:2])
    sequence = FIRST + (SECOND if first_passed else [])
    candidate = [row for row in rows[:len(sequence)] if row['condition'] == 'immediate-purge']
    if candidate and all(row['passed'] for row in candidate):
        sequence += NEGATIVE
    return sequence


def unstarted(rows):
    seen = {(row['condition'], row['case'], row['repetition']) for row in rows}
    result = []
    for condition, case, repetition in FIRST + SECOND + NEGATIVE:
        if (condition, case, repetition) not in seen:
            reason = ('first pair did not pass' if repetition == 1 else
                      'candidate reference check did not pass')
            result.append({'condition': condition, 'case': case, 'repetition': repetition, 'reason': reason})
    return result


def audit(directory):
    protocol = policy()
    if directory.is_file():
        ids = [identity(*row) for row in FIRST + SECOND + NEGATIVE]
        allowed = {'summary.json'} | {f'{name}/{file}' for name in ids for file in
                   ('resources.json', 'profile.json', 'profile.jsonl', 'stdout', 'stderr')}
        allowed |= {f'probe-{condition}.{stream}' for condition in CONDITIONS for stream in ('stdout', 'stderr')}
        with zipfile.ZipFile(directory) as archive, tempfile.TemporaryDirectory() as temporary:
            members = archive.infolist()
            # The first hosted upload retained its enclosing output directory.
            prefix = 'proof-allocator/' if 'proof-allocator/summary.json' in archive.namelist() else ''
            require(len(members) == len({member.filename for member in members}) and
                    all(member.filename in {prefix + name for name in allowed} for member in members) and
                    sum(member.file_size for member in members) <= 8 * 1024**2,
                    'unexpected or oversized allocator archive')
            archive.extractall(temporary)
            return audit(Path(temporary) / prefix)
    result = json.loads((directory / 'summary.json').read_text())
    require(result['schema'] == 'fr-product-proof-allocator-1' and result['status'] == 'complete' and result['admitted'] is False and
            result['live_model_calls'] == 0 and result['protocol'] == protocol and
            result['protocol_sha256'] == memory.tasks.common.digest(PROTOCOL),
            'allocator diagnostics cannot change the protocol or grant admission')
    require(result['qualification_run'] == protocol['qualification_run'], 'qualification run changed')
    require(set(result['probes']) == set(CONDITIONS), 'retain both allocator probes')
    for condition, probe in result['probes'].items():
        for stream in ('stdout', 'stderr'):
            path = directory / f'probe-{condition}.{stream}'
            require(path.stat().st_size <= 128 * 1024 and
                    memory.tasks.common.digest(path) == probe[stream + '_sha256'], 'allocator probe changed')
        verify_probe(condition, (directory / f'probe-{condition}.stdout').read_text(),
                     (directory / f'probe-{condition}.stderr').read_text())
    rows = result['cases']
    require([(row['condition'], row['case'], row['repetition']) for row in rows] == expected_schedule(rows),
            'retain every case and never repeat a stopped pair')
    limits = dict(protocol['limits'])
    limits['cpu_seconds'] = limits.pop('cpu_limit_seconds')
    positive_sources = set()
    for row in rows:
        path = directory / identity(row['condition'], row['case'], row['repetition'])
        resources = json.loads((path / 'resources.json').read_text())
        require(row['environment'] == CONDITIONS[row['condition']] and row['resources'] == resources and
                resources['limits'] == limits, 'condition, resource record or limits changed')
        require(row['command'] == [result['lake_path'], '-d', 'specs', 'build', '--wfail'],
                'preserve the complete original package-check command')
        require(memory.profile.audit(path, resources) == row['profile'] ==
                json.loads((path / 'profile.json').read_text()), 'retained process attribution changed')
        for stream in ('stdout', 'stderr'):
            require(memory.tasks.common.digest(path / stream) == row[stream + '_sha256'], 'checker output changed')
        output = (path / 'stdout').read_text(errors='replace') + (path / 'stderr').read_text(errors='replace')
        require(row['passed'] is verdict(row['case'], resources, output), 'proof verdict changed')
        require(row['protected_source_sha256'] == result['protected_source_sha256'], 'protected proof changed')
        if row['case'] == 'reference':
            positive_sources.add(row['source_tree_sha256'])
        else:
            require(row['source_tree_sha256'] not in positive_sources, 'negative control must change the proof')
    require(len(positive_sources) == 1, 'both conditions must check identical sources')
    require(result['unstarted'] == unstarted(rows), 'retain every unstarted case and its reason')
    require(result['decision'] == decision(rows), 'comparison decision differs from retained outcomes')
    return result


def verify_probe(condition, stdout, stderr):
    require('version 4.28.0' in stdout, 'measure the pinned Lean release')
    expected = '10' if condition == 'baseline' else '0'
    require(re.search(r"option 'purge_delay':\s*" + expected + r'\b', stderr),
            'pinned runtime must confirm the allocator setting')


def measure(inputs, run_path, lean_root, output):
    require(os.environ.get('GITHUB_ACTIONS') == 'true', 'run allocator comparisons on GitHub')
    protocol = policy()
    run = json.loads(run_path.read_text())
    require(str(run['id']) == protocol['qualification_run'], 'use the frozen qualification run')
    seed, prepared = memory.qualified_inputs(inputs, run)
    require(not output.exists(), 'preserve prior measurements in a fresh output directory')
    output.mkdir(parents=True)
    probes = {}
    for condition in CONDITIONS:
        env = environment(condition, lean_root)
        env['MIMALLOC_VERBOSE'] = '1'
        probe = subprocess.run([str(lean_root / 'bin/lean'), '--version'], env=env,
                               capture_output=True, text=True, timeout=10, check=True)
        for stream in ('stdout', 'stderr'):
            content = getattr(probe, stream)
            require(len(content.encode()) <= 128 * 1024, 'allocator probe exceeds output limit')
            (output / f'probe-{condition}.{stream}').write_text(content)
        verify_probe(condition, probe.stdout, probe.stderr)
        probes[condition] = {stream + '_sha256': memory.tasks.common.digest(output / f'probe-{condition}.{stream}')
                             for stream in ('stdout', 'stderr')}
    result = {'schema': 'fr-product-proof-allocator-1', 'status': 'collecting', 'run': os.environ['GITHUB_RUN_ID'],
        'source_commit': os.environ['GITHUB_SHA'], 'qualification_run': str(run['id']),
        'qualification_commit': run['head_sha'], 'platform': platform.system(), 'machine': platform.machine(),
        'protocol': protocol, 'protocol_sha256': memory.tasks.common.digest(PROTOCOL),
        'tool_sha256': {name: memory.tasks.common.digest(lean_root / 'bin' / name) for name in ('lean', 'lake', 'leanc')},
        'lake_path': str(lean_root / 'bin/lake'), 'seed_sha256': seed['archive_sha256'],
        'reference_patch_sha256': memory.tasks.common.digest(inputs / 'reference.patch'),
        'protected_source_sha256': hashlib.sha256(prepared['protected_source'].encode()).hexdigest(),
        'probes': probes, 'cases': [], 'admitted': False, 'live_model_calls': 0,
        'scope': 'Fresh complete package checks with identical source and pinned tools. No native client; '
                 'no OS cache reset. Immediate purging is the only measured environment change.'}
    def run_case(condition, case, repetition):
        row = output / identity(condition, case, repetition)
        row.mkdir()
        project = memory.workspace(inputs, seed, prepared, row / 'project')
        proof_control(project, case, inputs, seed)
        before = memory.tasks.common.sources(project)
        command = memory.command('package', lean_root, project)
        sampler = memory.profile.Sampler(row)
        with (row / 'stdout').open('wb') as stdout, (row / 'stderr').open('wb') as stderr, \
                patch.object(memory.host, 'sample', sampler):
            resources = memory.host.run(command, b'', stdout, stderr, row, cwd=project,
                                        env=environment(condition, lean_root), **protocol['limits'])
        memory.tasks.common.save(row / 'resources.json', resources)
        attribution = memory.profile.audit(row, resources)
        memory.tasks.common.save(row / 'profile.json', attribution)
        after = memory.tasks.common.sources(project)
        require(all(after.get(name) == value for name, value in before.items()), 'checker changed protected source')
        output_text = (row / 'stdout').read_text(errors='replace') + (row / 'stderr').read_text(errors='replace')
        record = {'condition': condition, 'case': case, 'repetition': repetition, 'command': command,
                  'environment': CONDITIONS[condition], 'resources': resources, 'profile': attribution,
                  'source_tree_sha256': hashlib.sha256(encoded(before)).hexdigest(),
                  'protected_source_sha256': hashlib.sha256(memory.tasks.masked((project / memory.tasks.PROOF).read_text()).encode()).hexdigest(),
                  'passed': verdict(case, resources, output_text),
                  **{stream + '_sha256': memory.tasks.common.digest(row / stream) for stream in ('stdout', 'stderr')}}
        result['cases'].append(record)
        result['decision'] = decision(result['cases'])
        memory.tasks.common.save(output / 'summary.json', result)
        return record
    for entry in FIRST:
        run_case(*entry)
    if all(row['passed'] for row in result['cases']):
        for entry in SECOND:
            run_case(*entry)
    candidate = [row for row in result['cases'] if row['condition'] == 'immediate-purge']
    if all(row['passed'] for row in candidate):
        for entry in NEGATIVE:
            run_case(*entry)
    result['status'] = 'complete'
    result['unstarted'] = unstarted(result['cases'])
    memory.tasks.common.save(output / 'summary.json', result)
    return audit(output)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='operation', required=True)
    collecting = commands.add_parser('measure')
    for name in ('inputs', 'run', 'lean-root', 'output'):
        collecting.add_argument('--' + name, type=Path, required=True)
    checking = commands.add_parser('audit')
    checking.add_argument('directory', type=Path)
    args = parser.parse_args()
    result = (audit(args.directory.resolve()) if args.operation == 'audit' else
              measure(args.inputs.resolve(), args.run.resolve(), args.lean_root.resolve(), args.output.resolve()))
    print(json.dumps(result['decision']))
