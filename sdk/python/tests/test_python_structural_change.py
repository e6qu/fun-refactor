"""Run reviewed Python renames through original imports and independent execution."""
import json
from pathlib import Path
import subprocess
import sys

import pytest

from fr_ir.change_scope import ChangeScope
from fr_ir.ir import IrError, TaskChange, TaskDelivery
from fr_ir.runtime import FrClient, FrRuntimeError
from fr_ir.structural_change import RefactorRequest

FR = Path(__file__).resolve().parents[3] / 'target/debug/fr'


def test_file_scoped_discovery_ignores_neighboring_declarations_and_edits(tmp_path):
    selected = tmp_path / 'selected.py'
    selected.write_text('def price():\n    return 3\n')
    neighbor = tmp_path / 'neighbor.py'
    neighbor.write_text('def price():\n    return 100\n')
    client = FrClient(selected, executable=FR, timeout=20, max_output_bytes=65536)
    before = client.project('find', 'price', '--source', '--bytes', '65536')
    assert before.definition_target().path == 'selected.py'
    neighbor.write_text('def changed():\n    return 200\n')
    assert client.project('find', 'price', '--source', '--bytes', '65536').to_data() == before.to_data()
    assert client.project('find', 'changed').at('/rows') == []


def fixture(tmp_path, text):
    root = tmp_path / 'project'
    root.mkdir()
    (root / '.fr').mkdir()
    (root / 'artifacts').mkdir()
    (root / 'helper.py').write_text(text)
    (root / 'caller.py').write_text('from helper import price as charge\ndef total():\n    return charge(3)\n')
    (root / '.fr/checks.json').write_text(json.dumps({'schema': 1, 'checks': [{
        'name': 'behavior', 'argv': [sys.executable, '-I', '-B', '-c',
            "import sys;sys.path.insert(0,'.');import caller;assert caller.total()==6"],
        'cwd': '.', 'timeout_seconds': 10, 'covers': ['original import and aliased caller behavior'],
    }]}))
    (root / '.fr/check-scopes.json').write_text(json.dumps({'schema': 1, 'checks': [
        {'name': 'behavior', 'paths': ['helper.py', 'caller.py']},
    ]}))
    return root, FrClient(root, executable=FR, timeout=20, max_output_bytes=1048576)


def change(client, policy):
    handle = client.project('find', 'price', '--in', 'helper.py').definition_target().handle
    scope = ChangeScope.inspect(client, [handle])
    if not scope.ready:
        data = scope.report.to_data()
        pytest.fail(json.dumps({'unresolved': data['unresolved'], 'cutoffs': data['cutoffs'],
            'weak_references': [row for row in data['references'] if row['confidence'] not in ('exact', 'import-qualified')],
            'unmapped_paths': data['unmapped_paths'], 'missing_mapped_paths': data['missing_mapped_paths'],
            'coverage': data['coverage']}, indent=2), pytrace=False)
    request = RefactorRequest.rename('line_price', documentation=policy)
    return scope.bind(TaskChange([], [request.target('rename', handle)],
        {'files-changed': 2, 'paths-changed': ['caller.py', 'helper.py']}, ['behavior'],
        TaskDelivery(patch='artifacts/rename.patch'), acceptance_checks=['behavior']))


@pytest.mark.parametrize('policy', ['update', 'preserve'])
def test_python_import_alias_and_documentation_policy_execute_and_replay(tmp_path, policy):
    original = '"""Use price to calculate an item."""\ndef price(value):\n    """price(3) returns 6."""\n    return value * 2\n'
    root, client = fixture(tmp_path, original)
    before = {name: (root / name).read_bytes() for name in ('helper.py', 'caller.py')}
    review = client.review(change(client, policy))
    assert all((root / name).read_bytes() == source for name, source in before.items())
    result = client.execute(review)
    assert result.passed
    acceptance = [stage for stage in result.at('/workflow/stages') if stage['stage'] == 'check-acceptance']
    assert acceptance and all(stage['status'] == 'passed' and stage['result']['passed'] is True
        for stage in acceptance)
    if policy == 'update':
        expected = original.replace('price', 'line_price')
    else:
        expected = original.replace('def price(', 'def line_price(')
    assert (root / 'helper.py').read_text() == expected
    assert (root / 'caller.py').read_text() == (
        'from helper import line_price as charge\ndef total():\n    return charge(3)\n')
    patch = root / 'artifacts/rename.patch'
    subprocess.run(['git', 'apply', '--reverse', str(patch)], cwd=root, check=True)
    assert all((root / name).read_bytes() == source for name, source in before.items())
    subprocess.run(['git', 'apply', '--check', str(patch)], cwd=root, check=True)
    subprocess.run(['git', 'apply', str(patch)], cwd=root, check=True)
    subprocess.run([sys.executable, '-I', '-B', '-c',
        "import sys;sys.path.insert(0,'.');import caller;assert caller.total()==6"], cwd=root, check=True)


@pytest.mark.parametrize('policy', ['update', 'preserve'])
@pytest.mark.parametrize('text', [
    'def price(value):\n    return value * 2\nlookup = "price"\n',
    'def price(value):\n    return value * 2\nraw = b"price"\n',
    'def price(value):\n    return value * 2\nif True:\n    "price"\n',
    'def price(value):\n    f"price"\n    return value * 2\n',
])
def test_documentation_policy_cannot_hide_executable_string_mentions(tmp_path, text, policy):
    root, client = fixture(tmp_path, text)
    with pytest.raises(FrRuntimeError, match='textual|unresolved'):
        client.review(change(client, policy))
    assert (root / 'helper.py').read_text() == text


def test_rename_policy_is_explicit_and_signature_policy_is_not_accepted():
    with pytest.raises(TypeError):
        RefactorRequest.rename('price')
    with pytest.raises(IrError):
        RefactorRequest('rename', 'price', None)
    with pytest.raises(IrError):
        RefactorRequest('remove-parameter', 0, 'update')


@pytest.mark.parametrize('parameters,call,index', [
    ('value, /, *, unused=0', 'charge(3, unused=0)', 1),
    ('unused, /, value', 'charge(0, 3)', 0),
    ('value, *unused', 'charge(3, 0)', 1),
    ('value, **unused', 'charge(3, extra=0)', 1),
    ('unused, value', 'charge(*[0, 3])', 0),
    ('value, *, unused=0', 'charge(3, **{"unused": 0})', 1),
])
def test_python_signature_delivery_preserves_import_aliases_and_replays(tmp_path, parameters, call, index):
    root, client = fixture(tmp_path, f'# keep this explanation\ndef price({parameters}):\n    return value * 2\n')
    (root / 'caller.py').write_text(f'from helper import price as charge\ndef total():\n    return {call}\n')
    before = {name: (root / name).read_bytes() for name in ('helper.py', 'caller.py')}
    handle = client.project('find', 'price', '--in', 'helper.py').definition_target().handle
    scope = ChangeScope.inspect(client, [handle])
    assert scope.ready, scope.report.to_data()
    request = RefactorRequest.remove_parameter(index)
    task = scope.bind(TaskChange([], [request.target('signature', handle)],
        {'files-changed': 2, 'paths-changed': ['caller.py', 'helper.py']}, ['behavior'],
        TaskDelivery(patch='artifacts/signature.patch'), acceptance_checks=['behavior']))
    review = client.review(task)
    assert all((root / name).read_bytes() == text for name, text in before.items())
    result = client.execute(review)
    assert result.passed, result.to_data()
    assert (root / 'helper.py').read_text().startswith('# keep this explanation\n')
    assert 'from helper import price as charge' in (root / 'caller.py').read_text()
    patch = root / 'artifacts/signature.patch'
    subprocess.run(['git', 'apply', '--reverse', str(patch)], cwd=root, check=True)
    assert all((root / name).read_bytes() == text for name, text in before.items())
    subprocess.run(['git', 'apply', str(patch)], cwd=root, check=True)
    subprocess.run([sys.executable, '-I', '-B', '-c',
        "import sys;sys.path.insert(0,'.');import caller;assert caller.total()==6"], cwd=root, check=True)


def test_python_signature_refusal_does_not_write_either_file(tmp_path):
    root, client = fixture(tmp_path, 'def price(value, *, unused=0):\n    return value * 2\n')
    (root / 'caller.py').write_text('from helper import price as charge\ndef effect():\n    print("effect")\n    return 0\ndef total():\n    return charge(3, unused=effect())\n')
    before = {name: (root / name).read_bytes() for name in ('helper.py', 'caller.py')}
    handle = client.project('find', 'price', '--in', 'helper.py').definition_target().handle
    scope = ChangeScope.inspect(client, [handle])
    task = scope.bind(TaskChange([], [RefactorRequest.remove_parameter(1).target('signature', handle)],
        {'files-changed': 2, 'paths-changed': ['caller.py', 'helper.py']}, ['behavior'],
        TaskDelivery(patch='artifacts/signature.patch'), acceptance_checks=['behavior']))
    with pytest.raises(FrRuntimeError, match='evaluation|literal|effects'):
        client.review(task)
    assert all((root / name).read_bytes() == text for name, text in before.items())
