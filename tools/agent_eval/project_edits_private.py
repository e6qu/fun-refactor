"""Check the four M2 cases inside the shared private grader before live calls."""
import argparse
import json
import os
from pathlib import Path
import tempfile
import subprocess
import tarfile
import time
import uuid

from . import project_edits_tasks as cases
from . import selected_grading as grading
from . import selected_tasks as common
from .selected_plan import require
from .selected_plan import encoded


def prepare_cache(image, original, prepared, rust, registry, output):
    """Compile only frozen original source, then remove the two packages an agent changes."""
    output.mkdir(parents=True, exist_ok=False)
    target = output / 'target'
    target.mkdir(mode=0o777)
    target.chmod(0o777)
    script = '''
from pathlib import Path
import shutil, subprocess
project = Path('/workspace/changed')
shutil.copytree('/inputs/original', project, dirs_exist_ok=True, ignore=shutil.ignore_patterns('.git', 'target'))
subprocess.run(['/opt/rust/bin/cargo', 'test', '-p', 'regex', '-p', 'regex-syntax', '--lib', '--no-run', '--locked', '--offline'], cwd=project, check=True)
subprocess.run(['/opt/rust/bin/cargo', 'clean', '-p', 'regex', '-p', 'regex-syntax'], cwd=project, check=True)
'''
    name = 'fr-m2-dependencies-' + uuid.uuid4().hex
    argv = grading.DOCKER + ['run', '--name', name, '--rm', '--network=none', '--read-only', '--cap-drop=ALL',
        '--security-opt=no-new-privileges', '--user=65534:65534', '--cpus=2', '--memory=512m', '--memory-swap=512m',
        '--pids-limit=64', '--env=CARGO_BUILD_JOBS=1',
        '--tmpfs=/workspace/changed:rw,exec,nosuid,nodev,size=64m,uid=65534,gid=65534,mode=0700',
        '--tmpfs=/tmp:rw,exec,nosuid,nodev,size=64m,mode=1777',
        '--tmpfs=/review/target:rw,exec,nosuid,nodev,size=64m,uid=65534,gid=65534,mode=0700']
    for source, destination, readonly in [(original, '/inputs/original', True), (rust, '/opt/rust', True),
            (registry, '/review/target/cargo-home/registry', True), (target, '/workspace/changed/target', False)]:
        require(',' not in str(source) and source.exists(), 'cache preparation needs explicit existing mounts')
        argv += ['--mount=type=bind,src=' + str(source) + ',dst=' + destination + (',readonly' if readonly else '')]
    argv += [image, 'python3', '-I', '-B', '-c', script]
    started = time.monotonic()
    print('Preparing unchanged regex dependencies; candidate limits remain separate and unchanged.', flush=True)
    try:
        result = subprocess.run(argv, capture_output=True, timeout=120)
    except subprocess.TimeoutExpired as error:
        (output / 'preparation.stdout').write_bytes(error.stdout or b'')
        (output / 'preparation.stderr').write_bytes(error.stderr or b'')
        raise
    finally:
        cleanup = subprocess.run(grading.DOCKER + ['rm', '--force', name], capture_output=True, timeout=10)
        require(cleanup.returncode == 0 or b'No such container' in cleanup.stderr, 'baseline compiler container cleanup failed')
    (output / 'preparation.stdout').write_bytes(result.stdout)
    (output / 'preparation.stderr').write_bytes(result.stderr)
    require(result.returncode == 0, 'unchanged dependency preparation failed; inspect preparation.stderr')
    fingerprints = target / 'debug/.fingerprint'
    require(not any(path.name.rsplit('-', 1)[0] in {'regex', 'regex-syntax'} for path in fingerprints.iterdir()),
            'prepared cache must not retain the two workspace packages under change')
    files = [path for path in target.rglob('*') if path.is_file()]
    require(not any(path.is_symlink() for path in target.rglob('*')), 'prepared compiler cache must contain no symlinks')
    require(files and sum(path.stat().st_size for path in files) <= 512 * 1024**2, 'prepared target exceeds 512 MiB')
    archive = output / 'cache.tar.gz'
    with tarfile.open(archive, 'w:gz', dereference=True) as stream:
        stream.add(target, arcname='target')
    registry_archive = output / 'registry.tar.gz'
    with tarfile.open(registry_archive, 'w:gz', dereference=True) as stream:
        stream.add(registry, arcname='registry')
    receipt = {'image': image, 'original_sha256': cases.digest_bytes(encoded(prepared['original'])),
        'rustc_sha256': common.digest(rust / 'bin/rustc'), 'source_mtime_ns': prepared['source_mtime_ns'],
        'newest_output_mtime_ns': max(path.stat().st_mtime_ns for path in files),
        'cache_archive_sha256': common.digest(archive), 'files': len(files),
        'registry_archive_sha256': common.digest(registry_archive),
        'expanded_bytes': sum(path.stat().st_size for path in files), 'elapsed_seconds': time.monotonic() - started,
        'removed_workspace_packages': ['regex', 'regex-syntax'],
        'scope': 'Shared compiler preparation from unchanged public source. No agent change or supplied solution enters this cache. Candidate crates rebuild under their original limits.'}
    common.save(output / 'cache.json', receipt)
    return output


def controls(inputs, rust, tools, output):
    require(os.environ.get('GITHUB_ACTIONS') == 'true', 'run private M2 checks on GitHub')
    output.mkdir(parents=True, exist_ok=False)
    image = grading.build(output / 'image')
    variants = ('unchanged', 'incomplete', 'reference', 'missing-patch', 'wrong-patch')
    summary = {'passed': False, 'live_model_calls': 0, 'image': image,
        'controls': {task + '-' + variant: {'status': 'unstarted'} for task in cases.TASKS for variant in variants},
        'scope': 'Supplied M2 changes checked without runner credentials, network or writable tool mounts.'}
    try:
        for task in cases.TASKS:
            original = inputs / task / 'original'
            prepared = json.loads((inputs / task / 'preparation/project.json').read_text())
            require(prepared['task'] == task and prepared['requirement'] == cases.TASKS[task] and
                    common.sources(original) == prepared['original'], 'private control task source differs')
            case_tools = {**tools, 'target_cache': None}
            if task == 'regex-escape-len':
                case_tools['target_cache'] = prepare_cache(image, original, prepared, rust, tools['registry'],
                                                         output / 'regex-target-cache')
            for variant in variants:
                identity = task + '-' + variant
                summary['controls'][identity] = {'status': 'running'}
                print(json.dumps({'task': task, 'variant': variant, 'status': 'checking'}), flush=True)
                with tempfile.TemporaryDirectory(prefix='fr-m2-private-control-') as temporary:
                    root = Path(temporary)
                    project = root / 'project'
                    common.checkout(original, project)
                    if variant not in {'unchanged', 'wrong-patch', 'missing-patch'}:
                        cases.prescribed_change(task, variant, project)
                    patch = root / 'change.patch'
                    patch.write_bytes(b'not a Git patch\n' if variant == 'wrong-patch' else
                                      common.comparison_grade.BASE.git(project, 'diff', '--binary', 'HEAD').stdout)
                    result = grading.grade(image, task, original, prepared, None, rust,
                        None if variant == 'missing-patch' else patch, output / identity, milestone='M2', tools=case_tools)
                    require(result['passed'] == (variant == 'reference'), 'unexpected private M2 outcome: ' + identity)
                    if variant in {'unchanged', 'missing-patch', 'wrong-patch'}:
                        require(result['verdict']['stage'] == 'delivery', 'invalid delivery reached candidate execution')
                    elif variant == 'incomplete':
                        require(result['verdict']['stage'] == 'source checks' and not result['verdict']['changed']['passed'],
                                'incomplete change did not reach independent validation')
                    else:
                        require(result['verdict']['undo_exact'] and result['verdict']['redo_exact'],
                                'accepted reference lacks exact reversal')
                        summary.setdefault('checker_versions', {})[task] = result['verdict']['checker_versions']
                    summary['controls'][identity] = {'status': 'passed', 'accepted_change': result['passed']}
        summary['passed'] = True
        return summary
    except Exception as error:
        summary['harness_error'] = type(error).__name__ + ': ' + str(error)
        raise
    finally:
        common.save(output / 'summary.json', summary)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('inputs', 'rust_root', 'node', 'typescript_root', 'registry', 'output'):
        parser.add_argument('--' + name.replace('_', '-'), type=Path, required=True)
    args = parser.parse_args()
    tools = {name: getattr(args, name).resolve() for name in ('node', 'typescript_root', 'registry')}
    print(json.dumps(controls(args.inputs.resolve(), args.rust_root.resolve(), tools,
                              args.output.resolve())))
