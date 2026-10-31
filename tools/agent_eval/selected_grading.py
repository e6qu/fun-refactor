"""Check M1 changes in a network-free container, without runner credentials."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import time
import uuid

from . import selected_tasks as tasks
from .container_resources import ContainerResources, audit, limits
from .isolated_grade import DOCKER, invoke
from .selected_native import boundary
from .selected_plan import CASE_DIRECTORIES, DIRECTORY, ROOT, encoded, require

SOURCE_LIMIT = 32 * 1024**2


def stage(source, destination):
    """Copy regular source files only, after the candidate process has been stopped."""
    require(not destination.exists(), 'grade snapshot needs a new directory')
    manifest = tasks.sources(source)
    require(sum((source / name).stat().st_size for name in manifest) <= SOURCE_LIMIT,
            'grade source exceeds 32 MiB')
    destination.mkdir(parents=True, mode=0o755)
    for name, entry in manifest.items():
        require(entry['mode'] in {0o644, 0o755}, 'delivered source needs normal Git checkout modes (0644/0755)')
        path = source / name
        target = destination / name
        target.parent.mkdir(parents=True, exist_ok=True)
        descriptor = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
        with os.fdopen(descriptor, 'rb') as stream:
            data = stream.read(SOURCE_LIMIT + 1)
        require(len(data) <= SOURCE_LIMIT and hashlib.sha256(data).hexdigest() == entry['sha256'],
                'candidate changed during its private snapshot')
        target.write_bytes(data)
        target.chmod(entry['mode'])
        os.utime(target, ns=(path.stat().st_atime_ns, path.stat().st_mtime_ns))
    require(tasks.sources(destination) == manifest, 'grade snapshot differs from delivered source')
    return {'sha256': hashlib.sha256(encoded(manifest)).hexdigest(), 'files': manifest}


def build(output):
    output.mkdir(parents=True)
    requirements = [line for line in (ROOT / '.github/actions/native-tools/python-wheels.txt').read_text().splitlines()
                    if any('/' + name + '-' in line for name in ('pytest', 'pluggy', 'packaging', 'iniconfig', 'pygments'))]
    require(len(requirements) == 5, 'the private grader requires its exact five pinned Python wheels')
    (output / 'requirements.txt').write_text('\n'.join(requirements) + '\n')
    shutil.copyfile(ROOT / DIRECTORY / 'Grader.Dockerfile', output / 'Dockerfile')
    with (output / 'build.stdout').open('wb') as stdout, (output / 'build.stderr').open('wb') as stderr:
        result = subprocess.run(DOCKER + ['build', '--progress=plain', '--tag', 'fr-m1-private-grader', str(output)],
                                stdout=stdout, stderr=stderr, timeout=120)
    require(result.returncode == 0, 'private grader image did not build; inspect build.stderr')
    image = subprocess.check_output(DOCKER + ['image', 'inspect', '--format', '{{.Id}}', 'fr-m1-private-grader'],
                                    timeout=10).decode().strip()
    require(image.startswith('sha256:') and len(image) == 71, 'missing immutable private grader image')
    tasks.save(output / 'image.json', {'image': image})
    return image


def command(image, name, mounts, task, milestone):
    require(milestone in {'M1', 'M2', 'M3', 'M3-feedback', 'M3-construction'}, 'choose an explicit private-grading study')
    arguments = ['create', '--name', name, '--pull=never', '--init', '--network=none', '--read-only',
        '--cap-drop=ALL', '--security-opt=no-new-privileges', '--user=65534:65534',
        '--cpus=0.5', '--memory=512m', '--memory-swap=512m', '--pids-limit=64', '--log-driver=none',
        '--no-healthcheck', '--workdir=/workspace',
        '--tmpfs=/workspace:rw,exec,nosuid,nodev,size=256m,uid=65534,gid=65534,mode=0700',
        '--tmpfs=/tmp:rw,exec,nosuid,nodev,size=128m,mode=1777',
        '--tmpfs=/review/target:rw,exec,nosuid,nodev,size=64m,uid=65534,gid=65534,mode=0700']
    if milestone in {'M3', 'M3-feedback', 'M3-construction'}:
        arguments += ['--env=LEAN_NUM_THREADS=1', '--env=FR_LEAN_JOBS=1']
    for source, destination in mounts:
        require(',' not in str(source) and source.exists(), 'grader mount needs an existing path without commas')
        arguments += ['--mount=type=bind,src=' + str(source) + ',dst=' + destination + ',readonly,bind-recursive=disabled']
    return arguments + [image, 'python3', '-B', '-m', 'agent_eval.selected_grade_probe',
                        '--milestone', milestone, '--task', task]


def grade(image, task, original, prepared, binary, rust, patch, output, *, milestone, tools):
    require(os.environ.get('GITHUB_ACTIONS') == 'true', 'grade on GitHub under the workstation rules')
    output.mkdir(parents=True, exist_ok=False)
    started = time.monotonic()
    report = {'task': task, 'milestone': milestone, 'passed': False, 'image': image, 'live_model_calls': 0,
        'scope': 'Private container checks and exact patch delivery; no live agent outcome.'}
    budget = {**limits(), 'memory_bytes': 512 * 1024**2, 'cpu_seconds': 60}
    name = 'fr-m1-grade-' + uuid.uuid4().hex
    with tempfile.TemporaryDirectory(prefix='fr-m1-grade-') as temporary:
        inputs = Path(temporary)
        inputs.chmod(0o755)
        report['original'] = stage(original, inputs / 'original')
        tasks.save(inputs / 'project.json', prepared)
        tasks.save(inputs / 'delivery.json', {'provided': patch is not None})
        if patch is not None:
            require(patch.is_file() and not patch.is_symlink() and patch.stat().st_size <= 1024**2,
                    'delivered patch must be a regular file of at most 1 MiB')
            shutil.copyfile(patch, inputs / 'change.patch')
            report['submitted_patch_sha256'] = tasks.digest(patch)
        else:
            (inputs / 'change.patch').write_bytes(b'')
            report['submitted_patch_sha256'] = None
        mounts = [(inputs / 'original', '/inputs/original'),
            (inputs / 'project.json', '/inputs/project.json'), (inputs / 'delivery.json', '/inputs/delivery.json'),
            (inputs / 'change.patch', '/inputs/change.patch'),
            (rust, '/opt/rust'),
            (ROOT / 'tools', '/review/tools'), (ROOT / 'sdk/python/src', '/review/sdk/python/src'),
            (ROOT / DIRECTORY, '/review/' + str(DIRECTORY)),
            (ROOT / 'tests/agent-eval/strsim-0.11.1.crate', '/review/tests/agent-eval/strsim-0.11.1.crate'),
            (ROOT / 'tests/agent-eval/unknown-target', '/review/tests/agent-eval/unknown-target'),
            (ROOT / 'tests/agent-eval/project-changes', '/review/tests/agent-eval/project-changes'),
            (ROOT / 'tests/agent-eval/python-repositories/boltons.tar.gz',
             '/review/tests/agent-eval/python-repositories/boltons.tar.gz')]
        if milestone in {'M1', 'M3', 'M3-feedback', 'M3-construction'}:
            require(not tools and binary is not None, 'proof grading requires its explicit fr and Lean tool paths')
            mounts += [(binary, '/opt/tool/bin/fr'), (Path('/opt/lean'), '/opt/lean')]
            if milestone in {'M3', 'M3-feedback', 'M3-construction'}:
                mounts += [(ROOT / 'tests/agent-eval/proof-writing', '/review/tests/agent-eval/proof-writing')]
            if milestone in {'M3-feedback', 'M3-construction'}:
                directory = CASE_DIRECTORIES[milestone]
                mounts += [(ROOT / directory, '/review/' + str(directory))]
        else:
            require(milestone == 'M2' and binary is None and set(tools) == {'node', 'typescript_root', 'registry', 'target_cache'},
                    'supply the explicit M2 checker and baseline-cache settings')
            mounts += [(tools['node'], '/opt/node/bin/node'), (tools['typescript_root'], '/opt/typescript'),
                (tools['registry'], '/review/target/cargo-home/registry'),
                (ROOT / 'tests/agent-eval/project-edits', '/review/tests/agent-eval/project-edits'),
                (ROOT / 'tests/agent-eval/regex', '/review/tests/agent-eval/regex')]
            if task == 'regex-escape-len':
                cache = tools['target_cache']
                receipt = json.loads((cache / 'cache.json').read_text())
                require(receipt['image'] == image and receipt['original_sha256'] == report['original']['sha256'] and
                        receipt['source_mtime_ns'] == prepared['source_mtime_ns'] and
                        receipt['rustc_sha256'] == tasks.digest(rust / 'bin/rustc'), 'baseline compiler cache inputs differ')
                mounts += [(cache / 'target', '/inputs/prepared-target'), (cache / 'cache.json', '/inputs/target-cache.json')]
            else:
                require(tools['target_cache'] is None, 'only the regex task needs a prepared dependency cache')
        with boundary.isolated_budget(output, budget) as slice_id:
            scope = ContainerResources(budget, slice_id, output).start()
            try:
                created, stdout, stderr = scope.invoke(DOCKER + command(image, name, mounts, task, milestone), b'', output, 10, 65536)
                report['creation'] = {'process': created, 'stderr': stderr.decode(errors='replace')}
                require(created['exit_code'] == 0 and not created['stop_reason'], 'private grader container creation failed')
                process, stdout, stderr = scope.invoke(DOCKER + ['start', '--attach', name], b'', output, 120, 1024**2)
                report['execution'] = {'process': process, 'stderr': stderr.decode(errors='replace')}
                (output / 'grader.stdout').write_bytes(stdout)
                (output / 'grader.stderr').write_bytes(stderr)
                require(process['exit_code'] == 0 and not process['stop_reason'], 'private grader stopped; inspect its stdout and stderr')
                report['verdict'] = json.loads(stdout)
                require('harness_error' not in report['verdict'], 'private grader reported an internal error')
                require(report['verdict']['submitted_patch_sha256'] == report['submitted_patch_sha256'],
                        'grader examined a different submitted patch')
                scope.check()
            except Exception as error:
                report['error'] = type(error).__name__ + ': ' + str(error)
            finally:
                cleanup, stdout, stderr = invoke(DOCKER + ['rm', '--force', name], b'', output, 10, 65536)
                report['cleanup'] = {'process': cleanup, 'stderr': stderr.decode(errors='replace')}
                report['resources'] = scope.finish()
                report['elapsed_seconds'] = time.monotonic() - started
            try:
                checked = audit(report['resources'], budget)
                require('error' not in report, report.get('error', ''))
                require(cleanup['exit_code'] == 0 and not cleanup['stop_reason'] and checked['complete'], 'private grader cleanup incomplete')
                require(checked['stop_reason'] is None, 'private grader exceeded its frozen control budget')
                report['passed'] = report['verdict']['passed']
                return report
            finally:
                tasks.save(output / 'result.json', report)


def controls(binary, output):
    require(os.environ.get('GITHUB_ACTIONS') == 'true', 'run private grading controls on GitHub')
    output.mkdir(parents=True, exist_ok=False)
    rust = Path(subprocess.check_output(['rustc', '--print', 'sysroot'], timeout=10).decode().strip())
    image = build(output / 'image')
    summary = {'passed': False, 'live_model_calls': 0, 'image': image, 'controls': {
        task + '-' + variant: {'status': 'unstarted'} for task in tasks.TASKS
        for variant in ('unchanged', 'incomplete', 'reference', 'missing-patch', 'wrong-patch')}}
    try:
        for task in tasks.TASKS:
            with tempfile.TemporaryDirectory(prefix='fr-m1-private-control-') as temporary:
                directory = Path(temporary)
                original = directory / 'original'
                prepared = tasks.prepare(task, original, binary, directory / 'preparation')
                tasks.save(output / (task + '-project.json'), prepared)
                for variant in ('unchanged', 'incomplete', 'reference', 'missing-patch', 'wrong-patch'):
                    identity = task + '-' + variant
                    summary['controls'][identity] = {'status': 'running'}
                    candidate = directory / variant
                    tasks.checkout(original, candidate)
                    actions = directory / (variant + '-actions')
                    actions.mkdir()
                    if variant != 'unchanged':
                        tasks.prescribed_change(task, 'reference' if variant in {'missing-patch', 'wrong-patch'} else variant,
                                                candidate, binary, actions)
                    if variant == 'unchanged':
                        path = candidate / ({'boltons-rename': 'boltons/mathutils.py', 'unicode-dice': 'src/lib.rs',
                                             'access-proof': tasks.PROOF_PATH}[task])
                        text = path.read_text()
                        if task == 'access-proof':
                            require(text.count('  sorry\n') == 1, 'proof control needs its original one-line proof gap')
                        path.write_text(text.replace('  sorry\n', '  sorry -- unchanged proof\n', 1) if task == 'access-proof'
                                        else text + ('\n# unchanged implementation\n' if task == 'boltons-rename'
                                                     else '\n// unchanged implementation\n'))
                    if variant == 'incomplete' and task == 'access-proof':
                        path = candidate / tasks.PROOF_PATH
                        text = path.read_text()
                        require(text.count('  sorry\n') == 1, 'incomplete control needs its original proof gap')
                        path.write_text(text.replace('  sorry\n', '  cases staff\n', 1))
                    patch = actions / 'change.patch'
                    patch.write_bytes(tasks.comparison_grade.BASE.git(candidate, 'diff', '--binary', 'HEAD').stdout
                                      if variant != 'wrong-patch' else b'not a Git patch\n')
                    result = grade(image, task, original, prepared, binary, rust,
                                   None if variant == 'missing-patch' else patch, output / identity, milestone='M1', tools={})
                    require(result['passed'] == (variant == 'reference'), 'unexpected private grading outcome: ' + identity)
                    if variant in {'missing-patch', 'wrong-patch'}:
                        require(result['verdict']['stage'] == 'delivery', 'invalid patch was not rejected at delivery')
                    if variant in {'unchanged', 'incomplete'}:
                        require('changed' in result['verdict'] and not result['verdict']['changed']['passed'],
                                'negative implementation control did not reach the independent code checks')
                        if task == 'access-proof':
                            require(result['verdict']['changed']['stage'] in {'strict proof check', 'Lean package check'},
                                    'negative proof control did not reach proof validation')
                    summary['controls'][identity] = {'status': 'passed', 'accepted_change': result['passed']}
        summary['passed'] = True
        return summary
    except Exception as error:
        for row in summary['controls'].values():
            if row['status'] == 'running':
                row.update(status='failed', error=type(error).__name__ + ': ' + str(error))
        summary['error'] = type(error).__name__ + ': ' + str(error)
        raise
    finally:
        tasks.save(output / 'summary.json', summary)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--fr', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    print(json.dumps(controls(args.fr.resolve(), args.output.resolve())))
