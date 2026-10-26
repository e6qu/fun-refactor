"""Private M1 checking process; candidate programs run only inside this container."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import time

from . import selected_tasks as tasks
from .selected_plan import encoded, require


def restore_dependency_cache(source, destination):
    def copy(path, target):
        relative = Path(path).relative_to(source)
        # regex-automata depends on the changed regex-syntax package and must rebuild.
        if (relative.parts[:2] == ('debug', 'deps') and
                relative.suffix in {'.rlib', '.rmeta', '.so'} and
                not relative.name.startswith('libregex_automata-')):
            Path(target).symlink_to(path)
            return target
        return shutil.copy2(path, target)

    shutil.copytree(source, destination, copy_function=copy)


def check_boundary(milestone):
    status = dict(line.split(':', 1) for line in Path('/proc/self/status').read_text().splitlines())
    require(os.geteuid() == 65534 and int(status['CapEff'].strip(), 16) == 0 and
            status['NoNewPrivs'].strip() == '1', 'private checks need an unprivileged process with no capabilities')
    require({name for _, name in socket.if_nameindex()} == {'lo'}, 'private grader has an external network interface')
    require(not Path('/var/run/docker.sock').exists(), 'private grader can see the runner Docker socket')
    readonly = ['/inputs/original', '/inputs/project.json', '/inputs/delivery.json', '/inputs/change.patch',
                '/review/tools', '/opt/rust']
    readonly += (['/opt/lean', '/opt/tool/bin/fr'] if milestone in {'M1', 'M3'} else
                 ['/opt/node/bin/node', '/opt/typescript', '/review/target/cargo-home/registry'])
    require(all(os.statvfs(path).f_flag & os.ST_RDONLY for path in readonly), 'private source or tool mount is writable')
    return {'uid': os.geteuid(), 'effective_capabilities': 0, 'new_privileges_allowed': False,
            'network_interfaces': ['lo'], 'docker_socket_visible': False, 'readonly_mounts': readonly}


def run(task, milestone):
    require(Path('/.dockerenv').is_file(), 'the private grader requires its container')
    require(not any(name.startswith(('GITHUB_', 'ACTIONS_')) for name in os.environ),
            'runner credentials and environment must stay outside the private grader')
    original = Path('/inputs/original')
    prepared = json.loads(Path('/inputs/project.json').read_text())
    if milestone == 'M2':
        from . import project_edits_tasks as cases
        require(task in cases.TASKS and prepared['requirement'] == cases.TASKS[task], 'private M2 task requirements differ')
    elif milestone == 'M3':
        from . import proof_writing_tasks as cases
        require(task in cases.CASES and prepared['property'] == cases.property_for(task,
                {'object_digest': prepared['property']['task_digest']}), 'private M3 fixed property differs')
    else:
        require(milestone == 'M1' and task in tasks.TASKS, 'choose one explicit private task')
    require(prepared['task'] == task and tasks.sources(original) == prepared['original'],
            'private original differs from the prepared task')
    root = Path('/workspace')
    original_sources = tasks.sources(original)
    changed = root / 'changed'
    shutil.copytree(original, changed)
    tasks.comparison_grade.BASE.initialize(changed)
    delivery = json.loads(Path('/inputs/delivery.json').read_text())
    patch = Path('/inputs/change.patch').read_bytes()
    require(len(patch) <= 1024**2, 'delivered patch exceeds 1 MiB')
    result = {'passed': False, 'stage': 'delivery', 'undo_exact': False, 'redo_exact': False,
              'submitted_patch_sha256': hashlib.sha256(patch).hexdigest() if delivery['provided'] else None}
    if not delivery['provided'] or not patch:
        return {**result, 'reason': 'the agent did not deliver a nonempty patch'}
    inspected = tasks.comparison_grade.BASE.git(changed, 'apply', '--numstat', '-z', '-', data=patch, check=False)
    if inspected.returncode != 0:
        return {**result, 'reason': 'the delivered patch cannot be read', 'stderr': inspected.stderr.decode(errors='replace')}
    entries = [entry.split(b'\t', 2) for entry in inspected.stdout.split(b'\0') if entry]
    if any(len(entry) != 3 or not entry[2] for entry in entries):
        return {**result, 'reason': 'unsupported patch path record'}
    try:
        paths = [entry[2].decode() for entry in entries]
    except UnicodeError:
        return {**result, 'reason': 'patch paths must be UTF-8'}
    if milestone == 'M2':
        permitted = set(prepared['original']) | {path for path in paths if task == 'regex-escape-len'
            and path.startswith(('src/', 'regex-syntax/src/')) and path.endswith('.rs')
            and all(part not in {'.', '..'} for part in Path(path).parts)}
    elif milestone == 'M3':
        permitted = {prepared['spec']}
    else:
        permitted = ({'src/lib.rs'} if task == 'unicode-dice' else
                     {tasks.PROOF_PATH} if task == 'access-proof' else set(prepared['rename_paths']))
    if not paths or len(paths) != len(set(paths)) or not set(paths) <= permitted:
        return {**result, 'reason': 'patch changes paths outside the task', 'patch_paths': paths}
    applicable = tasks.comparison_grade.BASE.git(changed, 'apply', '--check', '-', data=patch, check=False)
    if applicable.returncode != 0:
        return {**result, 'reason': 'the delivered patch does not apply to the original',
                'stderr': applicable.stderr.decode(errors='replace')}
    tasks.comparison_grade.BASE.git(changed, 'apply', '-', data=patch)
    candidate_sources = tasks.sources(changed)
    if milestone == 'M2' and task == 'regex-escape-len':
        require(os.statvfs('/inputs/prepared-target').f_flag & os.ST_RDONLY, 'prepared compiler cache must be read-only')
        cache = json.loads(Path('/inputs/target-cache.json').read_text())
        require(cache['original_sha256'] == hashlib.sha256(encoded(prepared['original'])).hexdigest(),
                'compiler cache belongs to another original source')
        restore_dependency_cache(Path('/inputs/prepared-target'), changed / 'target')
        modified = max(time.time_ns(), cache['newest_output_mtime_ns'] + 1000000000)
        for path in paths:
            if (changed / path).is_file():
                os.utime(changed / path, ns=(modified, modified))
    binary = Path('/opt/tool/bin/fr')
    if milestone == 'M2':
        checkers = {'python': '/opt/python/bin/python3', 'cargo': '/opt/rust/bin/cargo', 'rustc': '/opt/rust/bin/rustc',
                    'node': '/opt/node/bin/node', 'typescript': '/opt/typescript/bin/tsc'}
        result['checker_versions'] = {name: subprocess.check_output([path, '--version'], text=True, timeout=10).strip()
                                     for name, path in checkers.items() if name != 'typescript'}
        result['checker_versions']['typescript'] = subprocess.check_output(
            [checkers['node'], checkers['typescript'], '--version'], text=True, timeout=10).strip()
        settings = json.loads((cases.ROOT / cases.DIRECTORY / 'checkers.json').read_text())
        require(result['checker_versions']['python'] == 'Python ' + settings['independent_python'] and
                result['checker_versions']['rustc'].startswith('rustc ' + settings['independent_rust'] + ' ') and
                result['checker_versions']['node'] == 'v' + settings['node'] and
                result['checker_versions']['typescript'] == 'Version ' + settings['typescript']['version'],
                'private compiler versions differ from their explicit pins')
        verdict = cases.check(task, changed, prepared, checkers, root / 'changed-checks')
    elif milestone == 'M3':
        settings = json.loads((cases.ROOT / cases.DIRECTORY / 'checkers.json').read_text())
        result['checker_versions'] = {name: subprocess.check_output([name, '--version'], text=True, timeout=10).strip()
                                      for name in ('lean', 'lake', 'rustc', 'python3')}
        require(result['checker_versions']['lean'].startswith('Lean (version ' + settings['lean'] + ',') and
                result['checker_versions']['rustc'].startswith('rustc ' + settings['hosted_rust'] + ' '),
                'private proof checker differs from the pinned Lean and Rust versions')
        verdict = cases.check(changed, prepared, binary, root / 'changed-checks')
    else:
        verdict = tasks.check(task, changed, prepared, binary, root / 'changed-checks')
    result.update(stage='source checks', changed=verdict)
    require(tasks.sources(changed) == candidate_sources, 'candidate checks changed the delivered source')
    if not verdict['passed']:
        return result
    receiver = root / 'receiver'
    shutil.copytree(original, receiver)
    tasks.comparison_grade.BASE.initialize(receiver)
    tasks.comparison_grade.BASE.git(receiver, 'apply', '-', data=patch)
    require(tasks.sources(receiver) == candidate_sources, 'receiver differs from the submitted patch')
    if milestone == 'M2':
        result['receiver'] = {'passed': True, 'source_identity_verified': True,
            'checked_source_sha256': hashlib.sha256(encoded(candidate_sources)).hexdigest(),
            'scope': 'All language checks ran in the first fresh patch receiver. This second copy has identical source bytes and modes.'}
    elif milestone == 'M3':
        result['receiver'] = cases.check(receiver, prepared, binary, root / 'receiver-checks')
    else:
        result['receiver'] = tasks.check(task, receiver, prepared, binary, root / 'receiver-checks')
    require(tasks.sources(receiver) == candidate_sources, 'receiver checks changed the delivered source')
    tasks.comparison_grade.BASE.git(receiver, 'apply', '--reverse', '-', data=patch)
    require(tasks.sources(receiver) == original_sources, 'patch undo did not restore exact original files')
    result['undo_exact'] = True
    tasks.comparison_grade.BASE.git(receiver, 'apply', '-', data=patch)
    require(tasks.sources(receiver) == candidate_sources, 'patch redo did not restore the reviewed source')
    result['redo_exact'] = True
    result['stage'] = 'complete'
    result['passed'] = result['receiver']['passed']
    return result


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--milestone', choices=('M1', 'M2', 'M3'), required=True)
    parser.add_argument('--task', required=True)
    args = parser.parse_args()
    try:
        result = {'isolation': check_boundary(args.milestone), **run(args.task, args.milestone)}
    except Exception as error:
        result = {'passed': False, 'harness_error': type(error).__name__ + ': ' + str(error)}
    print(json.dumps(result, allow_nan=False))
    raise SystemExit(int('harness_error' in result))
