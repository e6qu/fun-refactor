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


def test_python_import_alias_and_documentation_rename_executes_and_replays(tmp_path):
    original = '"""Use price to calculate an item."""\ndef price(value):\n    """price(3) returns 6."""\n    return value * 2\n'
    root, client = fixture(tmp_path, original)
    before = {name: (root / name).read_bytes() for name in ('helper.py', 'caller.py')}
    result = client.execute(client.review(change(client, 'update')))
    assert result.at('/checks/passed') is True
    assert 'price as charge' not in (root / 'caller.py').read_text().replace('line_price as charge', '')
    assert 'line_price(3)' in (root / 'helper.py').read_text()
    assert 'return charge(3)' in (root / 'caller.py').read_text()
    patch = root / 'artifacts/rename.patch'
    subprocess.run(['git', 'apply', '--reverse', str(patch)], cwd=root, check=True)
    assert all((root / name).read_bytes() == source for name, source in before.items())
    subprocess.run(['git', 'apply', '--check', str(patch)], cwd=root, check=True)
    subprocess.run(['git', 'apply', str(patch)], cwd=root, check=True)
    subprocess.run([sys.executable, '-I', '-B', '-c',
        "import sys;sys.path.insert(0,'.');import caller;assert caller.total()==6"], cwd=root, check=True)


@pytest.mark.parametrize('text', [
    'def price(value):\n    return value * 2\nlookup = "price"\n',
    'def price(value):\n    return value * 2\nraw = b"price"\n',
    'def price(value):\n    return value * 2\nif True:\n    "price"\n',
    'def price(value):\n    f"price"\n    return value * 2\n',
])
def test_documentation_policy_cannot_hide_executable_string_mentions(tmp_path, text):
    root, client = fixture(tmp_path, text)
    with pytest.raises(FrRuntimeError, match='textual|unresolved'):
        client.review(change(client, 'update'))
    assert (root / 'helper.py').read_text() == text


def test_preserved_documentation_still_requires_resolving_its_mentions(tmp_path):
    text = 'def price(value):\n    """price returns twice its input."""\n    return value * 2\n'
    root, client = fixture(tmp_path, text)
    with pytest.raises(FrRuntimeError, match='textual|unresolved'):
        client.review(change(client, 'preserve'))
    assert (root / 'helper.py').read_text() == text


def test_rename_policy_is_explicit_and_signature_policy_is_not_accepted():
    with pytest.raises(TypeError):
        RefactorRequest.rename('price')
    with pytest.raises(IrError):
        RefactorRequest('rename', 'price', None)
    with pytest.raises(IrError):
        RefactorRequest('remove-parameter', 0, 'update')
