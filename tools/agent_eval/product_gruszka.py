"""Evaluate complete source-to-Gruszka checking; never infer native admission."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import subprocess
import sys
import tempfile
from unittest.mock import patch
import zipfile

from . import product_toolchain_memory as comparison
from . import product_allocator as controls
from . import product_memory as memory
from .selected_plan import ROOT, encoded, require

PROTOCOL = ROOT / memory.tasks.plan.DIRECTORY / 'memory/gruszka-protocol.json'
FIRST = [('baseline', 'reference', 0), ('gruszka', 'reference', 0)]
SECOND = [('gruszka', 'reference', 1), ('baseline', 'reference', 1)]
NEGATIVE = [('gruszka', 'unfinished', 0), ('gruszka', 'wrong-proof', 0)]
FILES = ('resources.json', 'profile.json', 'profile.jsonl', 'stdout', 'stderr')


def policy():
    value = json.loads(PROTOCOL.read_text())
    require(value['schema'] == 'fr-gruszka-evaluation-1' and value['lean_version'] == '4.28.0'
            and value['qualification_run'] == '44542024683'
            and value['limits'] == memory.runtime.CONTROL_LIMITS and value['workers'] == 1
            and value['checker_budget_bytes'] == 436 * 1024**2
            and value['minimum_rss_reduction_fraction'] == 0.1
            and value['admitted'] is False and value['live_model_calls'] == 0,
            'preserve the Gruszka protocol and original admission limits')
    return value


def environment(lean, work, ambient=None):
    env = comparison.environment(lean, ambient)
    for key in list(env):
        if key in {'LD_LIBRARY_PATH', 'DYLD_LIBRARY_PATH', 'DYLD_INSERT_LIBRARIES', 'LD_PRELOAD'} or key.startswith(('SK_', 'SUSZ_', 'GRUSZKA_')):
            del env[key]
    env.update(LEAN_SYSROOT=str(lean), LEAN_PATH=str(work / 'specs/.lake/build/lib/lean'),
               TMPDIR=str(work), XDG_CACHE_HOME=str(work / '.cache'))
    return env


def pipeline_commands(project, lean, binaries):
    p = policy()
    export = str(project / 'proof.susz')
    return [
        ('package', [str(lean / 'bin/lake'), '-d', 'specs', 'build', '--wfail']),
        ('export', [str(binaries / 'susz-olean'), '-L', str(project / 'specs/.lake/build/lib/lean'),
                    '-L', str(lean / 'lib/lean'), p['module'], '-o', export,
                    '--jobs', '1', '--threads', '1', '--no-index']),
        ('kernel', [str(binaries / 'gruszka'), export, '1']),
        ('axioms', [str(binaries / 'susz'), 'axioms', export, '--root', p['theorem'], '--threads', '1']),
    ]


def check_axioms(output):
    names = output.splitlines()
    require(all(name in policy()['allowed_axioms'] for name in names), 'unapproved proof axiom')
    return names


def pipeline(project, lean, binaries):
    for stage, argv in pipeline_commands(project, lean, binaries):
        print('FR_GRUSZKA_STAGE ' + stage, flush=True)
        # The parent bounds the whole process tree, output and cumulative time.
        # Only short checker verdicts and axiom names need interpretation here.
        if stage in {'kernel', 'axioms'}:
            result = subprocess.run(argv, cwd=project, stdout=subprocess.PIPE, text=True)
            print(result.stdout, end='', flush=True)
        else:
            result = subprocess.run(argv, cwd=project)
        if result.returncode:
            return result.returncode
        if stage == 'kernel':
            require(result.stdout.splitlines()[-1:] == ['accepted'], 'require full Gruszka acceptance')
        if stage == 'axioms':
            check_axioms(result.stdout)
        print('FR_GRUSZKA_DONE ' + stage, flush=True)
    return 0


def schedule(rows):
    sequence = FIRST[:]
    if len(rows) >= 2 and all(row['passed'] for row in rows[:2]):
        sequence += SECOND
    candidates = [row for row in rows[:len(sequence)] if row['condition'] == 'gruszka']
    if candidates and all(row['passed'] for row in candidates):
        sequence += NEGATIVE
    return sequence


def verdict(row, output):
    passed = controls.verdict(row['case'], row['resources'], output)
    if row['condition'] == 'gruszka' and row['case'] == 'reference':
        passed = passed and all('FR_GRUSZKA_DONE ' + stage in output
                                for stage in ('package', 'export', 'kernel', 'axioms'))
    return passed


def decision(rows):
    result = {'outcome': 'not-demonstrated', 'admitted': False, 'migration_qualified': False,
              'reason': 'Gruszka consumes compiled proofs; Lean source elaboration remains required.'}
    if len(rows) != 6 or not all(row['passed'] and controls.guarded_finish(row['resources']) for row in rows):
        return result
    baseline = min(row['resources']['sampled_aggregate_rss_bytes'] for row in rows
                   if row['condition'] == 'baseline')
    candidate = max(row['resources']['sampled_aggregate_rss_bytes'] for row in rows
                    if row['condition'] == 'gruszka' and row['case'] == 'reference')
    useful = baseline > 0 and candidate * 10 <= baseline * 9 and candidate <= policy()['checker_budget_bytes']
    result.update(outcome='investigate-reduction' if useful else 'no-usable-reduction',
                  minimum_baseline_rss_bytes=baseline, maximum_candidate_rss_bytes=candidate)
    return result


def audit(directory):
    if directory.is_file():
        allowed = {'gruszka-evaluation/summary.json', 'gruszka-tools/bin/provenance.json'}
        allowed |= {'gruszka-evaluation/' + controls.identity(*entry) + '/' + name
                    for entry in FIRST + SECOND + NEGATIVE for name in FILES}
        with zipfile.ZipFile(directory) as archive, tempfile.TemporaryDirectory() as temporary:
            members = archive.infolist()
            require(len(members) == len({m.filename for m in members}) and
                    all(m.filename in allowed for m in members) and
                    sum(m.file_size for m in members) <= 8 * 1024**2,
                    'unexpected or oversized Gruszka archive')
            archive.extractall(temporary)
            result = audit(Path(temporary) / 'gruszka-evaluation')
            require(result['tools'] == json.loads((Path(temporary) / 'gruszka-tools/bin/provenance.json').read_text()),
                    'separate tool provenance differs')
            return result
    result = json.loads((directory / 'summary.json').read_text())
    require(result['schema'] == 'fr-gruszka-evaluation-result-1' and result['status'] == 'complete'
            and result['protocol'] == policy() and result['protocol_sha256'] == memory.tasks.common.digest(PROTOCOL)
            and result['admitted'] is False and result['live_model_calls'] == 0,
            'protocol or admission claim changed')
    rows = result['cases']
    tools = result['tools']
    require(tools['sources'] == policy()['sources'] and
            set(tools['binaries']) == set(tools['lockfiles']) == set(policy()['sources']) and
            all(re.fullmatch('[a-f0-9]{64}', value) for table in ('binaries', 'lockfiles') for value in tools[table].values()),
            'tool provenance differs')
    require([(r['condition'], r['case'], r['repetition']) for r in rows] == schedule(rows),
            'missing, repeated or reordered measurement')
    limits = dict(policy()['limits'])
    limits['cpu_seconds'] = limits.pop('cpu_limit_seconds')
    sources = {}
    for row in rows:
        path = directory / controls.identity(row['condition'], row['case'], row['repetition'])
        resources = json.loads((path / 'resources.json').read_text())
        argv = row['command']
        if row['condition'] == 'baseline':
            require(len(argv) == 5 and Path(argv[0]).name == 'lake' and
                    argv[1:] == ['-d', 'specs', 'build', '--wfail'], 'package command changed')
        else:
            require(len(argv) == 11 and argv[1:5] == ['-B', '-m', 'agent_eval.product_gruszka', 'pipeline'] and
                    argv[5] == '--project' and argv[7] == '--lean' and argv[9] == '--binaries', 'pipeline command changed')
        require(resources == row['resources'] and resources['limits'] == limits, 'resource record changed')
        require(memory.profile.audit(path, resources) == row['profile'] == json.loads((path / 'profile.json').read_text()),
                'process attribution changed')
        for stream in ('stdout', 'stderr'):
            require(memory.tasks.common.digest(path / stream) == row[stream + '_sha256'], 'output changed')
        output = (path / 'stdout').read_text() + (path / 'stderr').read_text()
        require(row['passed'] is verdict(row, output), 'verdict changed')
        require(row['protected_source_sha256'] == result['protected_source_sha256'], 'protected proof changed')
        sources.setdefault(row['case'], set()).add(row['source_tree_sha256'])
    require(all(len(values) == 1 for values in sources.values()) and
            len({next(iter(values)) for values in sources.values()}) == len(sources),
            'conditions used different sources or identical negative controls')
    require(result['decision'] == decision(rows), 'decision changed')
    expected = [list(entry) for entry in FIRST + SECOND + NEGATIVE
                if entry not in [(r['condition'], r['case'], r['repetition']) for r in rows]]
    require(result['unstarted'] == expected, 'unstarted cases changed')
    return result


def measure(inputs, run_path, lean, binaries, output):
    require(os.environ.get('GITHUB_ACTIONS') == 'true', 'evaluate on GitHub runners')
    protocol = policy()
    run = json.loads(run_path.read_text())
    require(str(run['id']) == protocol['qualification_run'], 'use original qualification')
    seed, prepared = memory.qualified_inputs(inputs, run)
    require(not output.exists(), 'preserve earlier measurements')
    output.mkdir(parents=True)
    platform_name = {'Linux': 'linux', 'Darwin': 'darwin_aarch64'}[platform.system()]
    require(platform.system() != 'Darwin' or platform.machine() == 'arm64', 'require ARM macOS')
    require((lean / 'archive.sha256').read_text().strip() == protocol['archives'][platform_name], 'Lean archive differs')
    version = subprocess.check_output([str(lean / 'bin/lean'), '--version'], text=True)
    comparison.verify_version(version, protocol['lean_version'])
    provenance = json.loads((binaries / 'provenance.json').read_text())
    require(provenance['sources'] == protocol['sources'], 'candidate sources differ')
    for name in protocol['sources']:
        require(memory.tasks.common.digest(binaries / name) == provenance['binaries'][name], 'candidate binary differs')
    result = {'schema': 'fr-gruszka-evaluation-result-1', 'status': 'collecting',
              'run': os.environ['GITHUB_RUN_ID'], 'source_commit': os.environ['GITHUB_SHA'],
              'protocol': protocol, 'protocol_sha256': memory.tasks.common.digest(PROTOCOL),
              'platform': platform_name, 'machine': platform.machine(), 'tools': provenance,
              'lean_version': version, 'lean_archive_sha256': protocol['archives'][platform_name],
              'lean_binaries': {name: memory.tasks.common.digest(lean / 'bin' / name) for name in ('lean', 'lake', 'leanc')},
              'seed_sha256': seed['archive_sha256'],
              'reference_patch_sha256': memory.tasks.common.digest(inputs / 'reference.patch'),
              'protected_source_sha256': hashlib.sha256(prepared['protected_source'].encode()).hexdigest(),
              'cases': [], 'admitted': False, 'live_model_calls': 0}
    memory.tasks.common.save(output / 'summary.json', result)
    index = 0
    while index < len(schedule(result['cases'])):
        condition, case, repetition = schedule(result['cases'])[index]
        row_dir = output / controls.identity(condition, case, repetition)
        row_dir.mkdir()
        project = memory.workspace(inputs, seed, prepared, row_dir / 'project')
        controls.proof_control(project, case, inputs, seed)
        before = memory.tasks.common.sources(project)
        env = environment(lean, project)
        env['PYTHONPATH'] = str(ROOT / 'tools')
        argv = ([str(lean / 'bin/lake'), '-d', 'specs', 'build', '--wfail'] if condition == 'baseline' else
                [sys.executable, '-B', '-m', 'agent_eval.product_gruszka', 'pipeline', '--project', str(project),
                 '--lean', str(lean), '--binaries', str(binaries)])
        with (row_dir / 'stdout').open('wb') as stdout, (row_dir / 'stderr').open('wb') as stderr, \
                patch.object(memory.host, 'sample', memory.profile.Sampler(row_dir)):
            resources = memory.host.run(argv, b'', stdout, stderr, row_dir, cwd=project, env=env, **protocol['limits'])
        memory.tasks.common.save(row_dir / 'resources.json', resources)
        attribution = memory.profile.audit(row_dir, resources)
        memory.tasks.common.save(row_dir / 'profile.json', attribution)
        after = memory.tasks.common.sources(project)
        require(all(after.get(name) == value for name, value in before.items()), 'checker changed source')
        row = {'condition': condition, 'case': case, 'repetition': repetition, 'command': argv,
               'resources': resources, 'profile': attribution,
               'source_tree_sha256': hashlib.sha256(encoded(before)).hexdigest(),
               'protected_source_sha256': hashlib.sha256(memory.tasks.masked((project / memory.tasks.PROOF).read_text()).encode()).hexdigest(),
               **{stream + '_sha256': memory.tasks.common.digest(row_dir / stream) for stream in ('stdout', 'stderr')}}
        row['passed'] = verdict(row, (row_dir / 'stdout').read_text() + (row_dir / 'stderr').read_text())
        result['cases'].append(row)
        result['decision'] = decision(result['cases'])
        memory.tasks.common.save(output / 'summary.json', result)
        index += 1
    seen = [(r['condition'], r['case'], r['repetition']) for r in result['cases']]
    result.update(status='complete', unstarted=[list(e) for e in FIRST + SECOND + NEGATIVE if e not in seen])
    memory.tasks.common.save(output / 'summary.json', result)
    return audit(output)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='operation', required=True)
    collecting = commands.add_parser('measure')
    for name in ('inputs', 'run', 'lean', 'binaries', 'output'):
        collecting.add_argument('--' + name, type=Path, required=True)
    child = commands.add_parser('pipeline')
    for name in ('project', 'lean', 'binaries'):
        child.add_argument('--' + name, type=Path, required=True)
    commands.add_parser('audit').add_argument('directory', type=Path)
    args = parser.parse_args()
    if args.operation == 'pipeline':
        sys.exit(pipeline(args.project, args.lean, args.binaries))
    result = (audit(args.directory) if args.operation == 'audit' else
              measure(args.inputs, args.run, args.lean, args.binaries, args.output))
    print(json.dumps(result['decision']))
