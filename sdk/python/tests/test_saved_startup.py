"""Reuse checked startup across questions; compare fresh answers and Python."""
import copy
import hashlib
import json
from pathlib import Path

import pytest

from fr_ir.context import DirectoryObjectStore, MemoryObjectStore, restore_stored_value, store_merkle_value
from fr_ir.flow_startup import PreparedFlow, StartupCache, _restore, _store
from fr_ir.ir import IrError
from fr_ir.runtime import FrReport, FrRuntimeError
from test_flow_summaries import analyze, evidence
from test_flow_together import ENTRIES, HELPERS, SETUP, install, observe


def ask(cache, client, rules, name, *, imports, steps=4096, max_bytes=1_048_576):
    handle = client.project('find', name).definition_target().handle
    return cache.analyze(client, handle, rules=rules, context='html', steps=steps,
                         depth=8, max_bytes=max_bytes, imports=imports)


@pytest.mark.parametrize('route', ['local', 'helper', 'reexport', 'star'])
@pytest.mark.parametrize('disk', [False, True])
def test_later_question_reopens_saved_startup_and_matches_fresh_and_python(tmp_path, route, disk):
    client, rules = install(tmp_path, route)
    path = tmp_path / 'objects'
    store = DirectoryObjectStore(path) if disk else MemoryObjectStore()
    cache = StartupCache(store)
    first = ask(cache, client, rules, 'forwards', imports=route != 'local')
    assert first.checks == 1 and not first.reused and first.answer.witnesses
    digest = cache.persist()
    if disk:
        store = DirectoryObjectStore(path)
    cache = StartupCache.restore(store, digest)
    later = ask(cache, client, rules, 'stopped', imports=route != 'local')
    fresh = ask(StartupCache(MemoryObjectStore()), client, rules, 'stopped', imports=route != 'local')
    assert later.reused and later.checks == 0 and not later.answer.reused
    assert later.import_checks == int(route != 'local')
    assert later.answer.report.at('/complete') and not later.answer.witnesses
    assert evidence(later.answer) == evidence(fresh.answer)
    assert evidence(later.answer) == evidence(analyze(client, rules, 'stopped', imports=route != 'local'))
    assert len(later.report.at('/startup/dictionary_statements')) == 1
    assert observe(tmp_path) == [17, 0]


def test_unrelated_edit_rebinds_every_answer_to_current_source(tmp_path):
    client, rules = install(tmp_path, 'helper')
    cache = StartupCache(MemoryObjectStore())
    first = ask(cache, client, rules, 'forwards', imports=True)
    (tmp_path / 'other.py').write_text('def unrelated():\n    return 42\n')
    later = ask(cache, client, rules, 'stopped', imports=True)
    fresh = ask(StartupCache(MemoryObjectStore()), client, rules, 'stopped', imports=True)
    assert later.reused and later.answer.report.at('/revision') != first.answer.report.at('/revision')
    assert evidence(later.answer) == evidence(fresh.answer)


@pytest.mark.parametrize('change', ['helper', 'write', 'caller', 'stub', 'package', 'missing',
                                   'symlink', 'config', 'future'])
def test_source_and_import_choices_invalidate_saved_startup(tmp_path, change):
    client, rules = install(tmp_path, 'helper')
    cache = StartupCache(MemoryObjectStore())
    ask(cache, client, rules, 'forwards', imports=True)
    helper = tmp_path / 'helpers.py'
    if change == 'helper':
        helper.write_text(SETUP + HELPERS.replace('return 0', 'return value'))
    elif change == 'write':
        helper.write_text(SETUP.replace("'ready'", 'source()') + HELPERS)
    elif change == 'caller':
        (tmp_path / 'subject.py').write_text('from helpers import relay, discard\n' + ENTRIES + 'opaque()\n')
    elif change == 'stub':
        (tmp_path / 'helpers.pyi').write_text('def discard(value): ...\n')
    elif change == 'package':
        package = tmp_path / 'helpers'
        package.mkdir()
        (package / '__init__.py').write_text(HELPERS)
    elif change == 'missing':
        helper.unlink()
    elif change == 'symlink':
        helper.rename(tmp_path / 'implementation.py')
        helper.symlink_to('implementation.py')
    elif change == 'config':
        (tmp_path / 'pyproject.toml').write_text('[project]\nname="example"\nversion="0.1"\n')
    else:
        helper.write_text('from __future__ import annotations\n' + SETUP + HELPERS)
    later = ask(cache, client, rules, 'stopped', imports=True)
    fresh = ask(StartupCache(MemoryObjectStore()), client, rules, 'stopped', imports=True)
    assert not later.reused and later.checks == 1
    assert later.report.at('/preparation/reason') == 'changed-inputs'
    assert evidence(later.answer) == evidence(fresh.answer)
    if change not in ('helper', 'config', 'future'):
        assert not later.answer.report.at('/complete') and later.report.at('/startup') is None


def test_added_future_import_candidate_invalidates_saved_postponement(tmp_path):
    client, rules = install(tmp_path, 'helper')
    helper = tmp_path / 'helpers.py'
    helper.write_text('from __future__ import annotations\n' + SETUP + HELPERS)
    cache = StartupCache(MemoryObjectStore())
    assert ask(cache, client, rules, 'forwards', imports=True).answer.report.at('/complete')
    (tmp_path / '__future__.py').write_text('annotations = 0\n')
    later = ask(cache, client, rules, 'stopped', imports=True)
    assert not later.reused and not later.answer.report.at('/complete')


@pytest.mark.parametrize('setup', [SETUP + 'opaque()\n',
                                 SETUP.replace("'ready'", 'source()'),
                                 'def change():\n    global relay\n    relay = discard\nchange()\n'])
def test_unchecked_startup_never_enters_saved_work(tmp_path, setup):
    client, rules = install(tmp_path, 'helper', setup)
    store = MemoryObjectStore()
    cache = StartupCache(store)
    for name in ('forwards', 'stopped'):
        result = ask(cache, client, rules, name, imports=True)
        assert not result.reused and not result.answer.report.at('/complete')
        assert result.report.at('/startup') is None
    assert restore_stored_value(store, cache.persist())['record'] is None


def test_saved_work_preserves_captured_scalar_defaults_and_aliases(tmp_path):
    client, rules = install(tmp_path)
    (tmp_path / 'subject.py').write_text(
        'marker = 0\ndef choose(value=marker):\n    return value\n'
        'alias = choose\ndef forwards():\n    return sink(alias(source()))\n'
        'def stopped():\n    return sink(alias())\n')
    cache = StartupCache(MemoryObjectStore())
    assert ask(cache, client, rules, 'forwards', imports=False).answer.witnesses
    later = ask(cache, client, rules, 'stopped', imports=False)
    assert later.reused and later.answer.report.at('/complete') and not later.answer.witnesses


def test_new_rules_apply_to_new_question_without_rechecking_independent_startup(tmp_path):
    client, rules = install(tmp_path, 'helper')
    cache = StartupCache(MemoryObjectStore())
    ask(cache, client, rules, 'forwards', imports=True)
    rules.write_text(json.dumps({'version': 'changed', 'sources': [], 'sinks': ['sink']}))
    later = ask(cache, client, rules, 'forwards', imports=True)
    assert later.reused and later.answer.report.at('/complete') and not later.answer.witnesses
    fresh = ask(StartupCache(MemoryObjectStore()), client, rules, 'forwards', imports=True)
    assert evidence(later.answer) == evidence(fresh.answer)


def test_saved_startup_gives_no_extra_steps_or_output_bytes(tmp_path):
    client, rules = install(tmp_path, 'helper')
    cache = StartupCache(MemoryObjectStore())
    ask(cache, client, rules, 'forwards', imports=True)
    later = ask(cache, client, rules, 'stopped', imports=True, steps=1)
    assert later.reused and not later.answer.report.at('/complete')
    assert later.answer.report.at('/execution/analysis_steps') <= 1
    assert later.report.at('/startup') is None
    ask(cache, client, rules, 'forwards', imports=True)
    with pytest.raises(FrRuntimeError, match='byte budget'):
        ask(cache, client, rules, 'stopped', imports=True, max_bytes=2048)


@pytest.mark.parametrize('limit', ['modules', 'source_bytes'])
def test_saved_preparation_never_expands_import_limits(tmp_path, limit):
    client, rules = install(tmp_path, 'helper')
    cache = StartupCache(MemoryObjectStore())
    ask(cache, client, rules, 'forwards', imports=True)
    if limit == 'source_bytes':
        (tmp_path / 'helpers.py').write_text('#' * 262_144 + '\n' + SETUP + HELPERS)
        cutoff = 'import-source-byte-budget'
    else:
        (tmp_path / 'helpers.py').write_text('from chain0 import relay, discard\n')
        for number in range(16):
            source = f'from chain{number + 1} import relay, discard\n' if number < 15 else HELPERS
            (tmp_path / f'chain{number}.py').write_text(source)
        cutoff = 'import-module-budget'
    later = ask(cache, client, rules, 'stopped', imports=True)
    assert not later.reused and not later.answer.report.at('/complete')
    assert cutoff in later.answer.report.at('/cutoffs')
    assert len(later.answer.report.at('/inputs/modules/files')) <= 16
    assert later.report.at('/startup') is None


def test_another_file_replaces_the_single_saved_entry(tmp_path):
    client, rules = install(tmp_path)
    (tmp_path / 'other.py').write_text('def other():\n    return sink(source())\n')
    cache = StartupCache(MemoryObjectStore())
    ask(cache, client, rules, 'forwards', imports=False)
    other = ask(cache, client, rules, 'other', imports=False)
    assert not other.reused and other.answer.witnesses
    original = ask(cache, client, rules, 'stopped', imports=False)
    assert not original.reused and original.answer.report.at('/complete')


def test_saved_work_reopens_all_declarations_in_a_larger_file(tmp_path):
    client, rules = install(tmp_path)
    unused = ''.join(f'def unused_{number}():\n    return {number}\n' for number in range(200))
    (tmp_path / 'subject.py').write_text(SETUP + HELPERS + ENTRIES + unused)
    cache = StartupCache(MemoryObjectStore())
    first = ask(cache, client, rules, 'forwards', imports=False)
    assert len(first.report.at('/startup/functions')) == 205
    later = ask(cache, client, rules, 'stopped', imports=False)
    assert later.reused and later.answer.report.at('/complete') and not later.answer.witnesses


@pytest.mark.parametrize('field', ['context', 'steps', 'depth', 'max_bytes', 'imports', 'rules'])
def test_every_new_api_setting_is_required(tmp_path, field):
    client, rules = install(tmp_path)
    handle = client.project('find', 'forwards').definition_target().handle
    settings = dict(rules=rules, context='html', steps=4096, depth=8, max_bytes=1_048_576, imports=False)
    del settings[field]
    with pytest.raises(TypeError):
        StartupCache(MemoryObjectStore()).analyze(client, handle, **settings)


def test_corrupt_storage_refuses_without_running_a_fresh_check(tmp_path):
    client, rules = install(tmp_path, 'helper')
    store = DirectoryObjectStore(tmp_path / 'objects')
    cache = StartupCache(store)
    ask(cache, client, rules, 'forwards', imports=True)
    for path in (tmp_path / 'objects').rglob('*.json'):
        record = json.loads(path.read_text())
        if record.get('kind') == 'scalar' and 'fr-python-startup' in str(record.get('value')):
            record['value'] = record['value'].replace('fr-python-startup', 'corrupt-startup')
            path.write_text(json.dumps(record))
            break
    else:
        pytest.fail('no startup chunk found')
    with pytest.raises(IrError, match='verification'):
        ask(cache, client, rules, 'stopped', imports=True)


@pytest.mark.parametrize('change', ['schema', 'digest', 'extra', 'missing', 'span', 'path', 'name',
                                   'omitted', 'alias', 'function', 'statement', 'nested-extra'])
def test_native_rejects_rehashed_malformed_saved_work(tmp_path, change):
    client, rules = install(tmp_path, 'helper')
    result = ask(StartupCache(MemoryObjectStore()), client, rules, 'forwards', imports=True)
    saved = result.report.at('/startup')
    if change == 'schema':
        saved['schema'] = 'old-format'
    elif change == 'digest':
        saved['input_digest'] = '0' * 64
    elif change == 'extra':
        saved['old_field'] = True
    elif change == 'missing':
        del saved['dictionary_omitted']
    elif change == 'span':
        saved['functions'][0]['span']['start'] += 1
    elif change == 'path':
        saved['functions'][0]['path'] = '../helpers.py'
    elif change == 'name':
        saved['functions'][0]['name'] = 'invented'
    elif change == 'omitted':
        saved['dictionary_omitted'] = 1
    elif change == 'alias':
        saved['function_aliases']['invented'] = ['missing', 1]
    elif change == 'function':
        saved['functions'].pop()
    elif change == 'statement':
        saved['dictionary_statements'][0]['span']['start'] += 1
    else:
        saved['functions'][0]['span']['old_field'] = 0
    path = tmp_path / 'saved.json'
    encoded = json.dumps(saved, sort_keys=True, ensure_ascii=False, separators=(',', ':'))
    path.write_text(encoded)
    target = client.project('find', 'stopped').definition_target().handle
    with pytest.raises(FrRuntimeError):
        client.project('dataflow', target, '--summaries', '--imports', '--with-startup',
                       '--steps', '4096', '--depth', '8', '--bytes', '1048576', '--context', 'html',
                       '--rules', str(rules), '--reuse-startup', str(path),
                       '--startup-digest', hashlib.sha256(encoded.encode()).hexdigest())


@pytest.mark.parametrize('change', ['schema', 'checks', 'imports', 'reason', 'inputs', 'complete', 'extra',
                                   'work', 'work-type', 'budget', 'budget-type', 'revision', 'coverage'])
def test_sdk_rejects_inconsistent_prepared_reports(tmp_path, change):
    client, rules = install(tmp_path, 'helper')
    result = ask(StartupCache(MemoryObjectStore()), client, rules, 'forwards', imports=True)
    data = copy.deepcopy(result.report.to_data())
    if change == 'schema':
        data['schema'] = 'old-format'
    elif change == 'checks':
        data['preparation']['checks'] = True
    elif change == 'imports':
        data['preparation']['import_checks'] = 0
    elif change == 'reason':
        data['preparation']['reason'] = 'matching-inputs'
    elif change == 'inputs':
        data['startup']['inputs']['source']['digest'] = '0' * 64
    elif change == 'complete':
        data['answer']['complete'] = False
    elif change == 'extra':
        data['old_field'] = True
    elif change == 'work':
        data['answer']['execution']['analysis_steps'] = 4097
    elif change == 'work-type':
        data['answer']['execution']['analysis_steps'] = True
    elif change == 'budget':
        data['answer']['budget']['steps'] = 4095
    elif change == 'budget-type':
        data['answer']['budget']['call_depth'] = True
    elif change == 'revision':
        data['revision'] = '0' * 64
    else:
        data['coverage'] = {}
    with pytest.raises(FrRuntimeError):
        PreparedFlow.from_report(FrReport(data, ()))


@pytest.mark.parametrize('manifest', [None, {}, {'schema': 'old-format', 'record': None},
                                     {'schema': 'fr-startup-cache', 'record': False},
                                     {'schema': 'fr-startup-cache', 'record': 'invented'},
                                     {'schema': 'fr-startup-cache', 'record': None, 'old_field': 1}])
def test_one_strict_manifest_format(manifest):
    store = MemoryObjectStore()
    digest = store_merkle_value(store, manifest).digest
    with pytest.raises(FrRuntimeError):
        StartupCache.restore(store, digest)


@pytest.mark.parametrize('chunks', [[], [None], ['x' * 32_769], ['{}', '{}'],
                                   ['{"schema":"fr-python-startup","value":NaN}']])
def test_bad_chunk_records_refuse(chunks):
    store = MemoryObjectStore()
    digest = store_merkle_value(store, {'schema': 'fr-startup-record', 'chunks': chunks}).digest
    with pytest.raises(FrRuntimeError):
        _restore(store, digest)


def test_large_location_disclosure_remains_bounded_and_roundtrips(tmp_path):
    client, rules = install(tmp_path)
    source = 'table = {}\n' + ''.join(f"table['key{i}'] = 0\n" for i in range(20))
    (tmp_path / 'subject.py').write_text(source + HELPERS + ENTRIES)
    store = MemoryObjectStore()
    cache = StartupCache(store)
    first = ask(cache, client, rules, 'forwards', imports=False)
    assert len(first.report.at('/startup/dictionary_statements')) == 16
    assert first.report.at('/startup/dictionary_omitted') == 4
    saved = first.report.at('/startup')
    assert _restore(store, _store(store, saved)) == saved
    later = ask(cache, client, rules, 'stopped', imports=False)
    assert later.reused and later.answer.report.at('/complete')
    (tmp_path / 'subject.py').write_text(
        (source + HELPERS + ENTRIES).replace("table['key19'] = 0", "table['key19'] = source()"))
    changed = ask(cache, client, rules, 'stopped', imports=False)
    assert not changed.reused and not changed.answer.report.at('/complete')
    assert changed.report.at('/startup') is None


def test_complete_lesson_uses_separate_processes_and_rechecks_edits(tmp_path):
    import os
    import re
    import subprocess
    import sys
    root = Path(__file__).resolve().parents[3]
    guide = (root / 'docs/save-python-startup.md').read_text()
    programs = re.findall(r'```python\n(.*?)```', guide, re.S)
    assert len(programs) == 5
    for name, source in zip(('helpers.py', 'app.py', 'check_first.py', 'check_later.py', 'run_example.py'), programs):
        (tmp_path / name).write_text(source)
    (tmp_path / 'rules.json').write_text(re.findall(r'```json\n(.*?)```', guide, re.S)[0])
    environment = dict(os.environ, PATH=str(root / 'target/debug') + os.pathsep + os.environ['PATH'],
                       PYTHONPATH=str(root / 'sdk/python/src'))
    def run(name):
        result = subprocess.run([sys.executable, '-B', name], cwd=tmp_path, env=environment,
                                capture_output=True, text=True, timeout=20)
        assert result.returncode == 0, result.stderr
        return result.stdout
    assert run('check_first.py') == ('Preparation checks: 1\nImport checks: 1\n'
                                     'Finished: True\nPossible input connection: True\n')
    assert run('check_later.py') == ('Preparation checks: 0\nImport checks: 1\n'
                                     'Finished: True\nPossible input connection: False\n')
    assert run('run_example.py') == 'ready\n17\n0\n'
    helper = tmp_path / 'helpers.py'
    helper.write_text(helper.read_text().replace('return 0', 'return value'))
    assert run('check_later.py') == ('Preparation checks: 1\nImport checks: 1\n'
                                     'Finished: True\nPossible input connection: True\n')
    assert run('run_example.py') == 'ready\n17\n17\n'
    helper.write_text(helper.read_text().replace('register()\n\n', 'register()\nunknown()\n\n'))
    assert run('check_later.py') == ('Preparation checks: 1\nImport checks: 1\n'
                                     'Finished: False\nPossible input connection: True\n')
    helper.write_text(helper.read_text().replace('unknown()\n', ''))
    assert run('check_later.py') == ('Preparation checks: 1\nImport checks: 1\n'
                                     'Finished: True\nPossible input connection: True\n')
