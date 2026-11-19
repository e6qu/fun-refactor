"""Attribute proof-check memory on hosted runners without granting admission."""
import argparse
import json
import os
from pathlib import Path
import platform
import subprocess
from unittest.mock import patch

from . import bounded_host as host, client_memory_profile as profile
from . import product_runtime as runtime, product_tasks as tasks, selected_local as local
from .selected_plan import ROOT, require

CASES = ('package', 'direct-proof', 'std-import', 'init-import')
REPETITIONS = 2


def qualified_inputs(inputs, run):
    """Bind downloaded controls to the successful workflow and unchanged fixture."""
    require(run['status'] == 'completed' and run['conclusion'] == 'success' and
            run['path'] == '.github/workflows/product-validation.yml' and
            run['head_repository']['full_name'] == 'e6qu/fun-refactor',
            'use a successful product qualification from this repository')
    seed = json.loads((inputs / 'seed.json').read_text())
    prepared = json.loads((inputs / 'project.json').read_text())
    controls = json.loads((inputs / 'controls.json').read_text())
    require(seed['task'] == prepared['task'] == controls['task'] == 'clamp-proof' and
            seed['original'] == prepared['original'] and
            controls['passed'] is True and controls['live_calls'] == seed['live_model_calls'] == 0 and
            controls['seed_sha256'] == seed['archive_sha256'] == tasks.common.digest(inputs / 'original.tar.gz'),
            'proof seed or qualification differs')
    require(controls['controls']['reference']['passed'] is True and
            controls['controls']['fresh_receiver']['passed'] is True and
            all(controls['controls'][name]['passed'] is False for name in
                ('unchanged', 'unfinished', 'changed-program', 'changed-claim')),
            'require the positive and negative proof controls')
    fixture = ROOT / tasks.plan.DIRECTORY / 'initial/fixtures/Clamp.lean'
    require(seed['original'][tasks.PROOF]['sha256'] == tasks.common.digest(fixture),
            'the qualified proof fixture changed')
    return seed, prepared


def workspace(inputs, seed, prepared, destination):
    local.unpack(inputs / 'original.tar.gz', seed, destination)
    tasks.common.comparison_grade.BASE.git(destination, 'init', '-q')
    tasks.common.comparison_grade.BASE.git(destination, 'apply', str((inputs / 'reference.patch').resolve()))
    require(tasks.boundaries(destination, prepared) == [tasks.PROOF],
            'reference must change only the original proof region')
    # Derive the expected reference independently of the downloaded patch.
    expected = destination.parent / 'expected'
    local.unpack(inputs / 'original.tar.gz', seed, expected)
    tasks.reference(expected, prepared)
    require(tasks.common.sources(destination) == tasks.common.sources(expected),
            'the reference patch differs from the reviewed qualification answer')
    return destination


def command(case, lean_root, project):
    require(case in CASES, 'unknown diagnostic case')
    if case == 'package':
        return runtime.public_checks('clamp-proof', {'lake': {'path': str(lean_root / 'bin/lake')}})[0]['argv']
    lean = [str(lean_root / 'bin/lean'), '-j1', '-DwarningAsError=true']
    if case == 'direct-proof':
        return lean + [tasks.PROOF]
    source = project / 'Import.lean'
    source.write_text('import ' + ('Std' if case == 'std-import' else 'Init') +
                      '\nexample : True := by trivial\n')
    return lean + ['Import.lean']


def completed(resources):
    return (resources['exit_code'] == 0 and all(resources[key] is None for key in
            ('stop_reason', 'monitor_error', 'launch_error')))


def measure(inputs, run_path, lean_root, output):
    require(os.environ.get('GITHUB_ACTIONS') == 'true', 'run proof memory diagnostics on GitHub')
    run = json.loads(run_path.read_text())
    seed, prepared = qualified_inputs(inputs, run)
    require(not output.exists(), 'keep previous measurements; use a fresh output directory')
    output.mkdir(parents=True)
    version = subprocess.check_output([str(lean_root / 'bin/lean'), '--version'], text=True, timeout=10).strip()
    require('version 4.28.0' in version, 'measure the pinned Lean release')
    env = dict(os.environ, LEAN_NUM_THREADS='1', FR_LEAN_JOBS='1')
    env['PATH'] = str(lean_root / 'bin') + os.pathsep + env['PATH']
    result = {'schema': 'fr-product-proof-memory-1', 'run': os.environ['GITHUB_RUN_ID'],
        'source_commit': os.environ['GITHUB_SHA'], 'qualification_run': str(run['id']),
        'qualification_commit': run['head_sha'], 'platform': platform.system(),
        'machine': platform.machine(), 'lean_version': version,
        'lean_sha256': tasks.common.digest(lean_root / 'bin/lean'),
        'seed_sha256': seed['archive_sha256'],
        'reference_patch_sha256': tasks.common.digest(inputs / 'reference.patch'),
        'limits': runtime.CONTROL_LIMITS, 'live_model_calls': 0, 'admitted': False,
        'scope': 'Checker-only diagnostics, without a native client. Fresh package outputs, not an OS cache reset. '
                 'Import controls do not replace the protected task. A stopped case is retained, never retried.',
        'cases': []}
    tasks.common.save(output / 'summary.json', result)
    for repetition in range(REPETITIONS):
        for case in CASES:
            row = output / (case + '-' + str(repetition))
            row.mkdir()
            project = workspace(inputs, seed, prepared, row / 'project')
            before = tasks.common.sources(project)
            argv = command(case, lean_root, project)
            sampler = profile.Sampler(row)
            with (row / 'stdout').open('wb') as stdout, (row / 'stderr').open('wb') as stderr, \
                    patch.object(host, 'sample', sampler):
                resources = host.run(argv, b'', stdout, stderr, row, env=env, cwd=project,
                                     **runtime.CONTROL_LIMITS)
            tasks.common.save(row / 'resources.json', resources)
            measured = profile.audit(row, resources)
            tasks.common.save(row / 'profile.json', measured)
            after = tasks.common.sources(project)
            require(all(after.get(name) == value for name, value in before.items()),
                    'a diagnostic changed protected task source')
            record = {'case': case, 'repetition': repetition, 'completed': completed(resources),
                'resources': resources, 'profile': measured,
                'stdout_sha256': tasks.common.digest(row / 'stdout'),
                'stderr_sha256': tasks.common.digest(row / 'stderr')}
            result['cases'].append(record)
            tasks.common.save(output / 'summary.json', result)
            # Do not rerun a refused case. Other controls answer independent questions.
            if not record['completed']:
                result.setdefault('stopped_cases', []).append(case)
        if result.get('stopped_cases'):
            break
    return result


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('inputs', 'run', 'lean-root', 'output'):
        parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args()
    result = measure(args.inputs.resolve(), args.run.resolve(), args.lean_root.resolve(), args.output.resolve())
    print(json.dumps({'cases': len(result['cases']), 'completed': sum(row['completed'] for row in result['cases']),
                      'admitted': result['admitted'], 'live_model_calls': result['live_model_calls']}))
