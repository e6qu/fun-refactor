"""Private M1 checking process; candidate programs run only inside this container."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import socket

from . import selected_tasks as tasks
from .selected_plan import require


def check_boundary():
    status = dict(line.split(':', 1) for line in Path('/proc/self/status').read_text().splitlines())
    require(os.geteuid() == 65534 and int(status['CapEff'].strip(), 16) == 0 and
            status['NoNewPrivs'].strip() == '1', 'private checks need an unprivileged process with no capabilities')
    require({name for _, name in socket.if_nameindex()} == {'lo'}, 'private grader has an external network interface')
    require(not Path('/var/run/docker.sock').exists(), 'private grader can see the runner Docker socket')
    readonly = ['/inputs/original', '/inputs/project.json', '/inputs/delivery.json', '/inputs/change.patch',
                '/review/tools', '/opt/rust', '/opt/lean', '/opt/tool/bin/fr']
    require(all(os.statvfs(path).f_flag & os.ST_RDONLY for path in readonly), 'private source or tool mount is writable')
    return {'uid': os.geteuid(), 'effective_capabilities': 0, 'new_privileges_allowed': False,
            'network_interfaces': ['lo'], 'docker_socket_visible': False, 'readonly_mounts': readonly}


def run(task):
    require(Path('/.dockerenv').is_file(), 'the private grader requires its container')
    require(not any(name.startswith(('GITHUB_', 'ACTIONS_')) for name in os.environ),
            'runner credentials and environment must stay outside the private grader')
    original = Path('/inputs/original')
    prepared = json.loads(Path('/inputs/project.json').read_text())
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
    paths = [entry.split(b'\t', 2)[2].decode() for entry in inspected.stdout.split(b'\0') if entry]
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
    binary = Path('/opt/tool/bin/fr')
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
    parser.add_argument('--task', choices=list(tasks.TASKS), required=True)
    args = parser.parse_args()
    try:
        result = {'isolation': check_boundary(), **run(args.task)}
    except Exception as error:
        result = {'passed': False, 'harness_error': type(error).__name__ + ': ' + str(error)}
    print(json.dumps(result, allow_nan=False))
    raise SystemExit(int('harness_error' in result))
