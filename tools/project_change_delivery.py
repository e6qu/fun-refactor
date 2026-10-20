"""Use fr discovery, reviewed changes and history on the original selected projects."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time

from fr_ir.change_scope import ChangeScope
from fr_ir.context import DirectoryObjectStore
from fr_ir.investigation import TaskPlan, TaskStep
from fr_ir.investigation_delivery import run_delivery
from fr_ir.ir import TaskChange, TaskDelivery
from fr_ir.runtime import FrClient
from fr_ir.structural_change import RefactorRequest
from project_change_sources import save, digest

ROOT = Path(__file__).resolve().parents[1]
ORACLE = ROOT / 'tests/agent-eval/project-changes/oracle.py'


def run(argv, root, output, *, success):
    result = subprocess.run(argv, cwd=root, capture_output=True, text=True, timeout=120,
        env={**os.environ, 'PYTHONDONTWRITEBYTECODE': '1'})
    save(output, {'argv': argv, 'returncode': result.returncode,
                  'stdout': result.stdout, 'stderr': result.stderr})
    assert (result.returncode == 0) == success, output
    return result


def rows(report):
    data = report.to_data()
    assert data['page']['next'] is None, 'discovery is unfinished'
    return [dict(zip(data['columns'], row)) for row in data['rows']]


def target(client, name, language):
    report = client.project('find', name, '--source', '--bytes', '65536')
    extension = '.rs' if language == 'Rust' else '.py'
    found = [row for row in rows(report) if row['path'].endswith(extension)]
    assert len(found) == 1, [(r['path'], r['name']) for r in found]
    assert found[0]['source']['next_offset'] is None
    return found[0], report.to_data()


def measure(task, original, output, binary):
    started = time.monotonic()
    output.mkdir(parents=True, exist_ok=False)
    project = output / 'working'
    shutil.copytree(original, project)
    before = {str(p.relative_to(project)): digest(p) for p in project.rglob('*') if p.is_file()}
    oracle = [sys.executable, '-B', str(ORACLE), task['id'], str(project)]
    baseline = run(oracle + [task['function']], project, output / 'before.json', success=True)
    run(oracle + [task['renamed']], project, output / 'requested-before.json', success=False)
    run(['git', 'init', '-q'], project, output / 'git-init.json', success=True)
    run(['git', 'add', '.'], project, output / 'git-index.json', success=True)
    (project / '.fr').mkdir(exist_ok=True)
    (project / 'artifacts').mkdir()
    shutil.copyfile(ORACLE, project / '.fr/oracle.py')
    checks = [{'name': 'behavior', 'argv': [sys.executable, '-B', '.fr/oracle.py', task['id'], '.', task['renamed']],
        'cwd': '.', 'timeout_seconds': 30, 'identity_files': ['.fr/oracle.py'], 'covers': [json.dumps(task['behavior'])]}]
    if task['id'] == 'boltons':
        checks.append({'name': 'upstream', 'argv': [sys.executable, '-B', '-m', 'pytest', '-q', '-p', 'no:cacheprovider', 'tests/test_mathutils.py'],
            'cwd': '.', 'timeout_seconds': 120, 'covers': ['original tests/test_mathutils.py']})
        run(checks[-1]['argv'], project, output / 'upstream-before.json', success=True)
    syntax = (['rustc', '--edition=2021', '--crate-name', 'glob', '--crate-type', 'rlib',
        '--emit=metadata', 'src/lib.rs', '-o', '.fr/library.rmeta'] if task['id'] == 'glob' else
        [sys.executable, '-B', '-c', 'import ast,pathlib;[ast.parse(pathlib.Path(p).read_text()) for p in '
         '["boltons/mathutils.py","tests/test_mathutils.py"]]'])
    checks.append({'name': 'syntax', 'argv': syntax, 'cwd': '.', 'timeout_seconds': 30,
        'covers': ['Compile the complete library' if task['id'] == 'glob' else 'Parse the selected module and its original tests']})
    save(project / '.fr/checks.json', {'schema': 1, 'checks': checks})
    client = FrClient(project, executable=binary, timeout=120, max_output_bytes=2097152)
    selected, report = target(client, task['function'], task['language'])
    save(output / 'discovery.json', report)
    initial = ChangeScope.inspect(client, [selected['handle']], max_bytes=262144)
    save(output / 'scope-before-mapping.json', initial.report.to_data())
    paths = initial.report.at('/affected_paths')
    assert 0 < len(paths) <= 64
    save(project / '.fr/check-scopes.json', {'schema': 1, 'checks': [
        {'name': 'syntax', 'paths': paths}]})
    scope = ChangeScope.inspect(client, [selected['handle']], max_bytes=262144)
    save(output / 'scope.json', scope.report.to_data())
    assert scope.ready, scope.report.to_data()
    store = DirectoryObjectStore(output / 'objects')
    scope_root = scope.persist(store)
    names = [check['name'] for check in checks if check['name'] != 'syntax']
    change = scope.bind(TaskChange([], [RefactorRequest.rename(task['renamed'], documentation='update')
        .target('rename', selected['handle'])], {'files-changed': len(paths)}, ['syntax'],
        TaskDelivery(patch='artifacts/change.patch'), acceptance_checks=names))
    review = client.review(change)
    save(output / 'review.json', review.to_data())
    plan = TaskPlan(task['delivery'], ('behavior preserved',), (TaskStep.checked('outcome',
        'Does the requested public name preserve the declared behavior?', checks=tuple(names),
        satisfies=('behavior preserved',)),))
    delivered = run_delivery(plan, client, 'outcome', review, store)
    save(output / 'delivery.json', delivered.receipt.result.to_data())
    assert delivered.passed and delivered.resumed.complete
    patch = output / 'change.patch'
    shutil.copyfile(project / 'artifacts/change.patch', patch)
    after = run(oracle + [task['renamed']], project, output / 'after.json', success=True)
    assert json.loads(after.stdout) == json.loads(baseline.stdout)
    history = client.call('history').to_data()
    save(output / 'history.json', history)
    transaction = history['applied'][-1]
    save(output / 'undo.json', client.call('history', 'undo', str(transaction), '--write').to_data())
    assert all(digest(project / name) == value for name, value in before.items())
    run(oracle + [task['function']], project, output / 'after-undo.json', success=True)
    save(output / 'redo.json', client.call('history', 'redo', str(transaction), '--write').to_data())
    run(oracle + [task['renamed']], project, output / 'after-redo.json', success=True)
    receiver = output / 'receiver'
    shutil.copytree(original, receiver)
    run(['git', 'apply', '--check', str(patch.resolve())], receiver, output / 'receiver-check.json', success=True)
    run(['git', 'apply', str(patch.resolve())], receiver, output / 'receiver-apply.json', success=True)
    run([sys.executable, '-B', str(ORACLE), task['id'], str(receiver), task['renamed']],
        receiver, output / 'receiver-behavior.json', success=True)
    if task['id'] == 'boltons':
        upstream = next(check for check in checks if check['name'] == 'upstream')
        run(upstream['argv'], receiver, output / 'receiver-upstream.json', success=True)
    elapsed = time.monotonic() - started
    assert elapsed <= 180, 'original project delivery exceeds its task budget'
    result = {'passed': True, 'project': task['id'], 'seconds': elapsed, 'scope_root': scope_root,
        'receipt_root': delivered.receipt_root, 'plan_root': delivered.plan_root,
        'patch_sha256': digest(patch), 'before': json.loads(baseline.stdout), 'after': json.loads(after.stdout)}
    save(output / 'result.json', result)
    shutil.rmtree(project)
    shutil.rmtree(receiver)
    return result
