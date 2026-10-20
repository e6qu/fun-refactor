"""Audit original sources, saved discoveries and independently replayed deliveries."""
import json
from pathlib import Path
import shutil
import tempfile
import sys
import re

from fr_ir.context import DirectoryObjectStore
from fr_ir.investigation import TaskPlan, StepState
from fr_ir.investigation_delivery import DeliveryReceipt
from fr_ir.runtime import FrClient
from project_change_sources import unpack, digest
from project_change_delivery import run, ORACLE, target
from project_change_findings import discovery


def read(root, name):
    return json.loads((root / name).read_text())


def original(task, retained, root, limits):
    source = read(retained, 'source/source.json')
    assert source['repository'] == task['repository'] and source['revision'] == task['revision']
    archive = retained / 'source/source.tar.gz'
    assert digest(archive) == source['archive_sha256']
    unpack(archive, root, limits)
    actual = {str(path.relative_to(root)): digest(path) for path in root.rglob('*') if path.is_file()}
    assert actual == source['files'], 'original project inventory changed'
    assert source['licenses'] and all(name in actual for name in source['licenses'])


def audit_findings(task, retained, limits, binary, alternate):
    report = read(retained, 'findings/result.json')
    assert report['passed'] is True and report['project'] == task['id']
    expected = ['rename', 'move', 'delete', 'competing-import', 'project-configuration', 'question-settings', 'analyzer']
    assert [item['case'] for item in report['cases']] == expected
    assert report['seconds'] <= limits['task_seconds']
    base = retained / 'findings'
    baseline = read(base, 'baseline-plan.json')
    store = DirectoryObjectStore(base / 'objects')
    before = TaskPlan.restore(store, baseline['root'])
    assert all(step.state == StepState.SATISFIED for step in before.steps)
    with tempfile.TemporaryDirectory(prefix='fr-audit-project-changes-') as temporary:
        root = Path(temporary) / 'project'
        original(task, retained, root, limits)
        assert digest(root / baseline['independent_path']) == baseline['independent_sha256']
        for case in report['cases']:
            name = case['case']
            assert case['passed'] is True and case['identical_saved_and_fresh'] is True
            assert case['independent_retained'] is True
            executable = alternate if name == 'analyzer' else binary
            assert case['binary_sha256'] == digest(executable)
            saved = read(base, name + '/saved/observation.json')
            assert saved == read(base, name + '/fresh/observation.json')
            resumed = read(base, name + '/saved/resumed.json')
            assert resumed['invalidated'] == ['selected']
            plan_record = read(base, name + '/saved/plan.json')
            after = TaskPlan.restore(store, plan_record['root'])
            assert plan_record['independent_retained'] is True
            assert after.steps[1] == before.steps[1]
            assert all(step.state == StepState.SATISFIED for step in after.steps)
            for route in ('saved', 'fresh'):
                measured = read(base, name + '/' + route + '-process.json')['measurement']
                assert measured['exit_code'] == 0 and measured['stop_reason'] is None
                assert measured['limits'] == {'wall_seconds': 20, 'rss_bytes': 512 * 1024**2,
                    'disk_bytes': 16 * 1024**2, 'cpu_seconds': 10, 'transcript_bytes': 1024**2}
                assert measured['elapsed_seconds'] < 20 and measured['sampled_cpu_seconds'] <= 10
                assert measured['sampled_aggregate_rss_bytes'] <= 512 * 1024**2
                assert measured['sampled_disk_growth_bytes'] <= 16 * 1024**2
            shutil.rmtree(root)
            unpack(retained / 'source/source.tar.gz', root, limits)
            changes = read(base, name + '/source-changes.json')
            for path in changes['deleted']:
                destination = root / path
                assert destination.resolve().is_relative_to(root.resolve())
                destination.unlink()
            for path, expected_digest in changes['changed'].items():
                source = base / name / 'changed-sources' / path
                destination = root / path
                assert destination.resolve().is_relative_to(root.resolve()) and digest(source) == expected_digest
                destination.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(source, destination)
            assert digest(root / baseline['independent_path']) == baseline['independent_sha256']
            question = read(base, name + '/request.json')
            fresh = discovery(FrClient(root, executable=executable, timeout=20,
                max_output_bytes=2097152), question)
            assert equivalent(fresh, saved), (task['id'], name, 'independent replay differs')
    return True


def equivalent(actual, recorded, path=()):
    if isinstance(actual, dict) and isinstance(recorded, dict):
        assert actual.keys() == recorded.keys()
        for key in actual:
            pair = (actual[key], recorded[key])
            if key == 'root':
                assert all(isinstance(value, str) and Path(value).is_absolute() for value in pair)
                continue
            if key in ('revision', 'input_digest') or (key == 'digest' and path[-1:] == ('dependency',)):
                assert all(isinstance(value, str) and re.fullmatch('[0-9a-f]{64}', value) for value in pair)
                continue
            if key in ('handle', 'id') and all(isinstance(value, str) and value.startswith(('frp1:', 'fro1:')) for value in pair):
                assert actual[key].split(':')[0] == recorded[key].split(':')[0]
                continue
            if not equivalent(actual[key], recorded[key], path + (key,)):
                return False
        return True
    if isinstance(actual, list) and isinstance(recorded, list):
        return len(actual) == len(recorded) and all(equivalent(a, b, path) for a, b in zip(actual, recorded))
    return actual == recorded


def audit_delivery(task, retained, limits, binary):
    base = retained / 'delivery'
    report = read(base, 'result.json')
    assert report['passed'] is True and report['project'] == task['id']
    assert report['seconds'] <= limits['task_seconds']
    assert report['before'] == report['after']
    assert read(base, 'requested-before.json')['returncode'] != 0
    store = DirectoryObjectStore(base / 'objects')
    receipt = DeliveryReceipt.restore(store, report['receipt_root'])
    assert receipt.passed
    assert all(step.state == StepState.SATISFIED for step in TaskPlan.restore(store, report['plan_root']).steps)
    patch = base / 'change.patch'
    assert digest(patch) == report['patch_sha256']
    for name in ('before', 'after', 'after-undo', 'after-redo', 'receiver-behavior', 'receiver-check', 'receiver-apply'):
        assert read(base, name + '.json')['returncode'] == 0
    if task['id'] == 'boltons':
        for name in ('upstream-before', 'receiver-upstream'):
            assert read(base, name + '.json')['returncode'] == 0
    with tempfile.TemporaryDirectory(prefix='fr-audit-original-delivery-') as temporary:
        work = Path(temporary)
        root = work / 'receiver'
        original(task, retained, root, limits)
        before = run([sys.executable, '-B', str(ORACLE), task['id'], str(root), task['function']],
            root, work / 'baseline.json', success=True)
        assert json.loads(before.stdout) == report['before']
        run(['git', 'apply', '--check', str(patch.resolve())], root, work / 'check.json', success=True)
        run(['git', 'apply', str(patch.resolve())], root, work / 'apply.json', success=True)
        client = FrClient(root, executable=binary, timeout=20, max_output_bytes=2097152)
        target(client, task['renamed'], task['language'])
        run([sys.executable, '-B', str(ORACLE), task['id'], str(root), task['renamed']],
            root, work / 'behavior.json', success=True)
        if task['id'] == 'boltons':
            run([sys.executable, '-B', '-m', 'pytest', '-q', '-p', 'no:cacheprovider', 'tests/test_mathutils.py'],
                root, work / 'upstream.json', success=True)
        run(['git', 'apply', '--reverse', str(patch.resolve())], root, work / 'reverse.json', success=True)
        assert {str(path.relative_to(root)): digest(path) for path in root.rglob('*') if path.is_file()
                and path.relative_to(root).parts[0] != '.fr'} == read(retained, 'source/source.json')['files']
    return True
