"""Compare complete upstream proof checkers without changing frozen study pins."""
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

from . import product_allocator as controls
from . import product_memory as memory
from .selected_plan import ROOT, encoded, require

PROTOCOL = ROOT / memory.tasks.plan.DIRECTORY / 'memory/toolchain-protocol.json'
FIRST = [('baseline', 'reference', 0), ('candidate', 'reference', 0)]
SECOND = [('candidate', 'reference', 1), ('baseline', 'reference', 1)]
NEGATIVE = [('candidate', 'unfinished', 0), ('candidate', 'wrong-proof', 0)]
FILES = ('resources.json', 'profile.json', 'profile.jsonl', 'stdout', 'stderr')


def policy():
    value = json.loads(PROTOCOL.read_text())
    require(value['schema'] == 'fr-proof-toolchain-protocol-1' and
            value['qualification_run'] == '44542024683' and
            value['versions'] == {'baseline': '4.28.0', 'candidate': '4.36.0'} and
            value['limits'] == memory.runtime.CONTROL_LIMITS and
            value['minimum_rss_reduction_fraction'] == 0.1 and
            value['checker_budget_bytes'] == 436 * 1024**2 and
            value['admitted'] is False and value['live_model_calls'] == 0,
            'preserve the frozen toolchain comparison')
    return value


def environment(root, ambient=None):
    source = os.environ if ambient is None else ambient
    env = {key: value for key, value in source.items()
           if not key.upper().startswith(('LEAN_', 'MIMALLOC_', 'ELAN_', 'LAKE_'))}
    env.update(LEAN_NUM_THREADS='1', FR_LEAN_JOBS='1')
    env['PATH'] = str(root / 'bin') + os.pathsep + env.get('PATH', '')
    return env


def schedule(rows):
    require(len(rows) >= 2, 'retain both first conditions, including failures')
    sequence = FIRST + (SECOND if all(row['passed'] for row in rows[:2]) else [])
    candidates = [row for row in rows[:len(sequence)] if row['condition'] == 'candidate']
    if candidates and all(row['passed'] for row in candidates):
        sequence += NEGATIVE
    return sequence


def unstarted(rows):
    seen = {(row['condition'], row['case'], row['repetition']) for row in rows}
    return [{'condition': c, 'case': k, 'repetition': r,
             'reason': 'first pair did not pass' if r else 'candidate reference did not pass'}
            for c, k, r in FIRST + SECOND + NEGATIVE if (c, k, r) not in seen]


def decision(rows):
    if len(rows) != 6 or not all(row['passed'] for row in rows):
        return {'outcome': 'not-demonstrated', 'admitted': False}
    peaks = {condition: [row['resources']['sampled_aggregate_rss_bytes'] for row in rows
                        if row['condition'] == condition and row['case'] == 'reference']
             for condition in ('baseline', 'candidate')}
    baseline, candidate = min(peaks['baseline']), max(peaks['candidate'])
    useful = (baseline > 0 and candidate * 10 <= baseline * 9 and
              candidate <= policy()['checker_budget_bytes'])
    return {'outcome': 'qualify-migration' if useful else 'discard', 'admitted': False,
            'minimum_baseline_rss_bytes': baseline, 'maximum_candidate_rss_bytes': candidate,
            'conservative_rss_reduction_fraction': 1 - candidate / baseline if baseline else 0}


def verify_version(output, version):
    require(re.search(r'\bversion ' + re.escape(version) + r'(?=[, )])', output),
            'checker release differs from frozen condition')


def audit(directory):
    protocol = policy()
    if directory.is_file():
        allowed = {'summary.json'} | {f'{controls.identity(*entry)}/{name}'
                  for entry in FIRST + SECOND + NEGATIVE for name in FILES}
        with zipfile.ZipFile(directory) as archive, tempfile.TemporaryDirectory() as temporary:
            members = archive.infolist()
            require(len(members) == len({m.filename for m in members}) and
                    all(m.filename in allowed for m in members) and
                    sum(m.file_size for m in members) <= 8 * 1024**2,
                    'unexpected or oversized toolchain archive')
            archive.extractall(temporary)
            return audit(Path(temporary))
    result = json.loads((directory / 'summary.json').read_text())
    require(result['schema'] == 'fr-proof-toolchain-memory-1' and result['status'] == 'complete' and
            result['protocol'] == protocol and result['protocol_sha256'] == memory.tasks.common.digest(PROTOCOL) and
            result['admitted'] is False and result['live_model_calls'] == 0 and
            result['qualification_run'] == protocol['qualification_run'], 'protocol or admission claim changed')
    require(set(result['tools']) == set(protocol['versions']), 'retain both tool identities')
    for condition, tool in result['tools'].items():
        verify_version(tool['version_output'], protocol['versions'][condition])
        require(tool['archive_sha256'] == protocol['archives'][result['platform']][condition] and
                set(tool['sha256']) == {'lean', 'lake', 'leanc'} and
                all(re.fullmatch('[a-f0-9]{64}', digest) for digest in tool['sha256'].values()),
                'tool provenance differs')
    require(result['tools']['baseline']['lake_path'] != result['tools']['candidate']['lake_path'],
            'conditions need distinct distributions')
    rows = result['cases']
    require([(r['condition'], r['case'], r['repetition']) for r in rows] == schedule(rows),
            'retain each case once and do not retry a stopped pair')
    limits = dict(protocol['limits'])
    limits['cpu_seconds'] = limits.pop('cpu_limit_seconds')
    sources = {}
    for row in rows:
        path = directory / controls.identity(row['condition'], row['case'], row['repetition'])
        resources = json.loads((path / 'resources.json').read_text())
        require(row['resources'] == resources and resources['limits'] == limits, 'resource record changed')
        require(row['command'] == [result['tools'][row['condition']]['lake_path'],
                                  '-d', 'specs', 'build', '--wfail'], 'complete package command changed')
        require(memory.profile.audit(path, resources) == row['profile'] ==
                json.loads((path / 'profile.json').read_text()), 'process attribution changed')
        for stream in ('stdout', 'stderr'):
            require(memory.tasks.common.digest(path / stream) == row[stream + '_sha256'], 'output changed')
        output = (path / 'stdout').read_text(errors='replace') + (path / 'stderr').read_text(errors='replace')
        require(row['passed'] is controls.verdict(row['case'], resources, output), 'proof verdict changed')
        require(row['protected_source_sha256'] == result['protected_source_sha256'], 'protected proof changed')
        sources.setdefault(row['case'], set()).add(row['source_tree_sha256'])
    require(all(len(values) == 1 for values in sources.values()) and
            len({next(iter(values)) for values in sources.values()}) == len(sources),
            'reference sources must match and each negative control must differ')
    require(result['decision'] == decision(rows) and result['unstarted'] == unstarted(rows),
            'decision or unstarted cases changed')
    return result


def measure(inputs, run_path, roots, output):
    require(os.environ.get('GITHUB_ACTIONS') == 'true', 'measure on GitHub runners')
    protocol = policy()
    run = json.loads(run_path.read_text())
    require(str(run['id']) == protocol['qualification_run'], 'use original qualification')
    seed, prepared = memory.qualified_inputs(inputs, run)
    require(not output.exists(), 'preserve earlier measurements')
    output.mkdir(parents=True)
    platform_name = {'Linux': 'linux', 'Darwin': 'darwin_aarch64'}[platform.system()]
    require(platform.system() != 'Darwin' or platform.machine() == 'arm64', 'require ARM macOS')
    tool_records = {}
    for condition, root in roots.items():
        # The workflow verifies each archive before extraction and retains the digest beside it.
        digest = (root / 'archive.sha256').read_text().strip()
        require(digest == protocol['archives'][platform_name][condition], 'distribution archive differs')
        probe = subprocess.run([str(root / 'bin/lean'), '--version'], env=environment(root),
                               capture_output=True, text=True, check=True, timeout=10)
        verify_version(probe.stdout, protocol['versions'][condition])
        tool_records[condition] = {'version_output': probe.stdout, 'archive_sha256': digest,
            'lake_path': str(root / 'bin/lake'),
            'sha256': {name: memory.tasks.common.digest(root / 'bin' / name) for name in ('lean', 'lake', 'leanc')}}
    result = {'schema': 'fr-proof-toolchain-memory-1', 'status': 'collecting',
        'run': os.environ['GITHUB_RUN_ID'], 'source_commit': os.environ['GITHUB_SHA'],
        'qualification_run': str(run['id']), 'qualification_commit': run['head_sha'],
        'platform': platform_name, 'machine': platform.machine(), 'tools': tool_records,
        'protocol': protocol, 'protocol_sha256': memory.tasks.common.digest(PROTOCOL),
        'seed_sha256': seed['archive_sha256'],
        'reference_patch_sha256': memory.tasks.common.digest(inputs / 'reference.patch'),
        'protected_source_sha256': hashlib.sha256(prepared['protected_source'].encode()).hexdigest(),
        'cases': [], 'admitted': False, 'live_model_calls': 0}
    memory.tasks.common.save(output / 'summary.json', result)

    def run_case(condition, case, repetition):
        row = output / controls.identity(condition, case, repetition)
        row.mkdir()
        project = memory.workspace(inputs, seed, prepared, row / 'project')
        controls.proof_control(project, case, inputs, seed)
        before = memory.tasks.common.sources(project)
        argv = memory.command('package', roots[condition], project)
        sampler = memory.profile.Sampler(row)
        with (row / 'stdout').open('wb') as stdout, (row / 'stderr').open('wb') as stderr, \
                patch.object(memory.host, 'sample', sampler):
            resources = memory.host.run(argv, b'', stdout, stderr, row, cwd=project,
                                        env=environment(roots[condition]), **protocol['limits'])
        memory.tasks.common.save(row / 'resources.json', resources)
        attribution = memory.profile.audit(row, resources)
        memory.tasks.common.save(row / 'profile.json', attribution)
        after = memory.tasks.common.sources(project)
        require(all(after.get(name) == value for name, value in before.items()), 'checker changed source')
        content = (row / 'stdout').read_text(errors='replace') + (row / 'stderr').read_text(errors='replace')
        result['cases'].append({'condition': condition, 'case': case, 'repetition': repetition,
            'command': argv, 'resources': resources, 'profile': attribution,
            'source_tree_sha256': hashlib.sha256(encoded(before)).hexdigest(),
            'protected_source_sha256': hashlib.sha256(memory.tasks.masked(
                (project / memory.tasks.PROOF).read_text()).encode()).hexdigest(),
            'passed': controls.verdict(case, resources, content),
            **{stream + '_sha256': memory.tasks.common.digest(row / stream) for stream in ('stdout', 'stderr')}})
        result['decision'] = decision(result['cases'])
        memory.tasks.common.save(output / 'summary.json', result)

    for entry in FIRST:
        run_case(*entry)
    if all(row['passed'] for row in result['cases']):
        for entry in SECOND:
            run_case(*entry)
    if all(row['passed'] for row in result['cases'] if row['condition'] == 'candidate'):
        for entry in NEGATIVE:
            run_case(*entry)
    result.update(status='complete', unstarted=unstarted(result['cases']))
    memory.tasks.common.save(output / 'summary.json', result)
    return audit(output)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='operation', required=True)
    collecting = commands.add_parser('measure')
    for name in ('inputs', 'run', 'baseline', 'candidate', 'output'):
        collecting.add_argument('--' + name, type=Path, required=True)
    commands.add_parser('audit').add_argument('directory', type=Path)
    args = parser.parse_args()
    result = (audit(args.directory.resolve()) if args.operation == 'audit' else measure(
        args.inputs.resolve(), args.run.resolve(),
        {'baseline': args.baseline.resolve(), 'candidate': args.candidate.resolve()}, args.output.resolve()))
    print(json.dumps(result['decision']))
