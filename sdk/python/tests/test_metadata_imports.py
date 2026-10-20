"""Check a proved missing sibling import without skipping unknown startup."""
import copy
import hashlib
import json

import pytest

from fr_ir.context import MemoryObjectStore
from fr_ir.flow import FlowCache
from fr_ir.flow_dependencies import FlowDependencies
from fr_ir.runtime import FrReport, FrRuntimeError
from test_flow_summaries import analyze, evidence, workspace
from test_postponed_annotations import observe


METADATA = "try:\n    from ._version import version as __version__\nexcept ImportError:\n    __version__ = 'unknown'\n"
HOOKS = '''def __getattr__(name):
    raise AttributeError(name)

def __dir__():
    return ['relay']
'''


def install(tmp_path, prefix, forwards, route):
    client, rules = workspace(tmp_path)
    package = tmp_path / 'package'
    package.mkdir(exist_ok=True)
    helper = 'def relay(value):\n    return ' + ('value' if forwards else '0') + '\n'
    if route == 'direct':
        (package / '__init__.py').write_text(prefix + helper + HOOKS)
        imports = 'from package import relay\n'
    else:
        (package / '__init__.py').write_text(prefix + HOOKS)
        (package / 'helpers.py').write_text(helper)
        imports = 'from package.helpers import relay\n'
    (tmp_path / 'subject.py').write_text(imports + 'def entry():\n    return sink(relay(source()))\n')
    return client, rules


@pytest.mark.parametrize('route', ['direct', 'submodule'])
@pytest.mark.parametrize('forwards', [True, False])
def test_missing_import_selects_the_literal_metadata_handler(tmp_path, route, forwards):
    client, rules = install(tmp_path, METADATA, forwards, route)
    result = analyze(client, rules, 'entry', imports=True, standard_library=())
    assert result.report.at('/complete'), result.report.at('/cutoffs')
    assert bool(result.witnesses) is forwards
    assert observe(tmp_path) == {'result': 17 if forwards else 0, 'raised': None,
                                'events': [['source', 17], ['sink', 17 if forwards else 0]]}
    probe, = result.dependencies.metadata_imports
    assert probe.importer == 'package/__init__.py' and probe.selected
    assert probe.lookup.module == 'package._version' and probe.lookup.target is None
    assert not probe.lookup.opaque_candidates
    assert probe.span.contains(probe.handler)
    assert result.dependencies.lookup_count == len(result.dependencies.lookups) + 1


@pytest.mark.parametrize('change', ['source', 'stub', 'extension', 'namespace'])
def test_new_import_candidates_invalidate_the_checked_absence(tmp_path, change):
    client, rules = install(tmp_path, METADATA, True, 'submodule')
    cache = FlowCache(MemoryObjectStore())
    first = analyze(client, rules, 'entry', cache, imports=True, standard_library=())
    assert first.report.at('/complete') and first.witnesses
    (tmp_path / 'unrelated.py').write_text('def note():\n    return 0\n')
    assert analyze(client, rules, 'entry', cache, imports=True, standard_library=()).reused
    target = tmp_path / 'package' / {'source': '_version.py', 'stub': '_version.pyi',
                                     'extension': '_version.abi3.so', 'namespace': '_version'}[change]
    if change == 'namespace':
        target.mkdir()
    else:
        target.write_text("version = 'new'\n")
    changed = analyze(client, rules, 'entry', cache, imports=True, standard_library=())
    assert not changed.reused and not changed.report.at('/complete')
    assert changed.report.at('/input_digest') != first.report.at('/input_digest')
    probe, = changed.dependencies.metadata_imports
    assert not probe.selected
    assert evidence(changed) == evidence(analyze(client, rules, 'entry', imports=True, standard_library=()))


@pytest.mark.parametrize('prefix', [
    METADATA.replace("__version__ = 'unknown'", '__version__ = source()'),
    METADATA.replace('except ImportError:', 'except ValueError:'),
    METADATA.replace('except ImportError:', 'except ImportError as error:'),
    METADATA.replace('except ImportError:', 'except (ImportError, ValueError):'),
    'ImportError = ValueError\n' + METADATA,
    METADATA + 'finally:\n    source()\n',
    METADATA.replace("__version__ = 'unknown'", 'relay = lambda value: 0'),
    METADATA.replace('from ._version', 'from .._version'),
])
def test_unknown_effects_and_unproved_handlers_remain_unfinished(tmp_path, prefix):
    client, rules = install(tmp_path, prefix, True, 'submodule')
    result = analyze(client, rules, 'entry', imports=True, standard_library=())
    assert not result.report.at('/complete')
    assert not result.dependencies.metadata_imports


def test_a_parent_attribute_hook_cannot_be_skipped_when_importing_a_child(tmp_path):
    client, rules = workspace(tmp_path)
    package = tmp_path / 'package'
    package.mkdir()
    (package / '__init__.py').write_text('''class Proxy:
    def relay(self, value):
        return 0
def __getattr__(name):
    return Proxy()
''')
    (package / 'helpers.py').write_text('def relay(value):\n    return value\n')
    (tmp_path / 'subject.py').write_text('from package import helpers\ndef entry():\n    return sink(helpers.relay(source()))\n')
    result = analyze(client, rules, 'entry', imports=True, standard_library=())
    assert not result.report.at('/complete')
    assert 'import-attribute-hook-unchecked' in result.report.at('/cutoffs')
    assert observe(tmp_path)['events'] == [['source', 17], ['sink', 0]]


@pytest.mark.parametrize('change', ['selected', 'location', 'lookup', 'duplicate'])
def test_corrupt_metadata_branch_records_are_rejected(tmp_path, change):
    client, rules = install(tmp_path, METADATA, True, 'submodule')
    result = analyze(client, rules, 'entry', imports=True, standard_library=())
    value = copy.deepcopy(result.report.to_data())
    probes = value['inputs']['modules']['metadata_imports']
    probe = probes[0]
    if change == 'selected':
        probe['selected'] = False
    elif change == 'location':
        probe['handler']['end'] = probe['span']['end'] + 1
    elif change == 'lookup':
        probe['lookup']['candidates']['package/_version.py']['status'] = 'outside-snapshot'
    else:
        probes.append(copy.deepcopy(probe))
    value['input_digest'] = hashlib.sha256(json.dumps(value['inputs'], sort_keys=True, ensure_ascii=False,
                                                      separators=(',', ':')).encode()).hexdigest()
    with pytest.raises(FrRuntimeError):
        FlowDependencies.from_report(FrReport(value, ()))
