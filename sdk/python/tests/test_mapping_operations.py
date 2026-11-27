"""Check dictionary unions and pair updates against independent Python execution."""
import pytest

from test_graph_analysis import analyze, execute
from test_list_sequences import program


@pytest.mark.parametrize('body,expected', [
    ('values = {"token": read()}\nmerged = values | {"token": 0}\nsend(merged["token"])\nsend(values["token"])', ['0', 'Customer']),
    ('values = {"token": read()}\nmerged = {} | values\nmerged["token"] = 0\nsend(values["token"])', ['Customer']),
    ('inner = [read()]\nvalues = {"token": inner}\nmerged = values | {}\nmerged["token"][0] = 0\nsend(inner[0])', ['0']),
    ('values = {1: read()} | {True: 0}\nsend(values[1])', ['0']),
    ('values = {"a": 0, "b": read()} | {"a": 1, "c": 2}\nsend(values)', ["{'a': 1, 'b': 'Customer', 'c': 2}"]),
    ('values = {"token": read()}\nalias = values\nvalues |= {"token": 0}\nsend(alias["token"])', ['0']),
    ('values = {}\nalias = values\nvalues |= [("token", read())]\nsend(alias["token"])', ['Customer']),
    ('values = {"token": read()}\nvalues |= values\nsend(values["token"])', ['Customer']),
    ('inner = [read()]\nvalues = {}\nvalues |= (("token", inner),)\nvalues["token"][0] = 0\nsend(inner[0])', ['0']),
    ('values = dict([("token", read()), ("token", 0)])\nsend(values["token"])', ['0']),
    ('values = dict((("token", read()),), token=0)\nsend(values["token"])', ['0']),
    ('values = dict([[None, read()], [1, 0], [True, read()]])\nsend(values[None])\nsend(values[1])', ['Customer', 'Customer']),
    ('key = "token"\nvalues = dict([(key, read())])\nsend(values["token"])', ['Customer']),
    ('values = {}\nvalues.update([("token", read())])\nsend(values["token"])', ['Customer']),
    ('values = {"token": read()}\nvalues.update((("token", 0),))\nsend(values["token"])', ['0']),
    ('inner = [read()]\nvalues = dict([("token", inner)])\nvalues["token"][0] = 0\nsend(inner[0])', ['0']),
    ('values = {"token": read()}\ntry:\n    values.update([("token", 0), ("bad",)])\nexcept ValueError:\n    send(values["token"])', ['0']),
    ('values = {"token": read()}\ntry:\n    values |= [("token", 0), 1]\nexcept TypeError:\n    send(values["token"])', ['0']),
    ('values = {"token": read()}\ntry:\n    values.update([("token", 0), ([], 1)])\nexcept TypeError:\n    send(values["token"])', ['0']),
    ('values = {"token": read()}\ntry:\n    values.update([("other", 0), ("bad",)], token=0)\nexcept ValueError:\n    send(values["token"])', ['Customer']),
    ('values = {"token": read()}\ntry:\n    values.update({"token": 0}, {})\nexcept TypeError:\n    send(values["token"])', ['Customer']),
    ('values = {"token": read()}\ntry:\n    values | [("token", 0)]\nexcept TypeError:\n    send(values["token"])', ['Customer']),
    ('values = {"token": read()}\ntry:\n    0 | values\nexcept TypeError:\n    send(values["token"])', ['Customer']),
    ('values = {"token": read()}\ntry:\n    values |= None\nexcept TypeError:\n    send(values["token"])', ['Customer']),
])
@pytest.mark.parametrize('renamed', [False, True])
def test_mapping_operations_match_python(tmp_path, body, expected, renamed):
    source = program(body, renamed=renamed)
    assert execute(source, 'Customer') == expected
    report = analyze(tmp_path, source)
    assert report['complete'], report['cutoffs']
    assert bool(report['witnesses']) is ('Customer' in str(expected))


@pytest.mark.parametrize('expression', ['values | relay(values)', 'dict(relay(values))'])
def test_operands_are_evaluated_before_mapping_contents_are_read(tmp_path, expression):
    source = program(f'values = {{"token": read()}}\nmerged = {expression}\nsend(merged["token"])',
                     'def relay(table):\n    table["token"] = 0\n    return table\n')
    assert execute(source, 'Customer') == ['0']
    report = analyze(tmp_path, source)
    assert report['complete'] and not report['witnesses'], report['cutoffs']


def test_helper_failure_and_cleanup_preserve_partial_mapping_updates(tmp_path):
    source = program('values = {"token": read()}\ntry:\n    relay(values)\nexcept ValueError:\n    send(values["token"])',
        'def relay(table):\n    try:\n        table |= [("token", 0), ("bad",)]\n    finally:\n        table["other"] = 1\n')
    assert execute(source, 'Customer') == ['0']
    report = analyze(tmp_path, source)
    assert report['complete'] and not report['witnesses'], report['cutoffs']


@pytest.mark.parametrize('call', ['dict(pairs, spare=change(pairs))', 'relay(pairs, change(pairs))'])
def test_pair_contents_are_read_after_all_call_arguments(tmp_path, call):
    source = program(f'pairs = [["token", read()]]\nvalues = {call}\nsend(values["token"])',
        'def change(pairs):\n    pairs[0][1] = 0\n    return 1\n'
        'def relay(pairs, unused):\n    return dict(pairs)\n')
    assert execute(source, 'Customer') == ['0']
    report = analyze(tmp_path, source)
    assert report['complete'] and not report['witnesses'], report['cutoffs']


def test_known_pair_keys_survive_helper_binding_and_reads(tmp_path):
    source = program('keys = ["token"]\nvalues = relay(keys[0], read())\nsend(values["token"])',
                     'def relay(key, value):\n    return dict([(key, value)])\n')
    assert execute(source, 'Customer') == ['Customer']
    report = analyze(tmp_path, source)
    assert report['complete'] and report['witnesses'], report['cutoffs']


@pytest.mark.parametrize('body', [
    'values = dict([(read(), 0)])\nsend(values)',
    'values = dict([((1, 2), read())])\nsend(values)',
    'values = {"token": read()}\nvalues |= arbitrary()\nsend(values)',
    'values = {"token": read()}\nmerged = values | arbitrary()\nsend(merged)',
    'values = {"token": read()}\nouter = [values]\nouter[0] |= {"token": 0}\nsend(values)',
    'key = "first"\nif read():\n    key = "second"\nvalues = dict([(key, 0)])\nsend(values)',
])
def test_unknown_protocols_keys_and_targets_remain_incomplete(tmp_path, body):
    report = analyze(tmp_path, program(body))
    assert not report['complete'] and report['cutoffs']


@pytest.mark.parametrize('count,complete', [(64, True), (65, False)])
@pytest.mark.parametrize('operation', ['values | extra', 'values.update(extra)', 'values |= extra'])
def test_combined_mapping_budget(tmp_path, count, complete, operation):
    pairs = ', '.join(f'"k{i}": 0' for i in range(count - 1))
    body = f'values = {{{pairs}}}\nextra = {{"token": read()}}\n'
    body += f'merged = {operation}\nsend(merged)' if operation == 'values | extra' else f'{operation}\nsend(values)'
    report = analyze(tmp_path, program(body))
    assert report['complete'] is complete, report['cutoffs']
    if complete:
        assert report['witnesses']


@pytest.mark.parametrize('module', ['helpers', 'moved_helpers'])
def test_imported_mapping_update_invalidates_cached_answers(tmp_path, module):
    import json
    import subprocess
    import sys
    from fr_ir.runtime import FrClient
    from fr_ir.context import MemoryObjectStore
    from fr_ir.flow import FlowCache
    from test_graph_analysis import ROOT

    helper = tmp_path / (module + '.py')
    helper.write_text('def relay(table):\n    table |= {}\n')
    source = program('values = {"token": read()}\nrelay(values)\nsend(values["token"])', f'from {module} import relay\n')
    analyze(tmp_path, source)
    client = FrClient(tmp_path, executable=str(ROOT / 'target/debug/fr'), max_output_bytes=1048576)
    cache = FlowCache(MemoryObjectStore())

    def query(selected):
        handle = client.project('find', 'entry').definition_target().handle
        return selected.analyze(client, handle, standard_library=('builtins',),
                                rules=tmp_path / 'rules.json', context='html', steps=4096,
                                depth=8, max_bytes=1048576, summaries=True, imports=True)

    def runtime():
        driver = ('import sys,json\n' + f'sys.path.insert(0, {str(tmp_path)!r})\n'
                  'sent=[]\ndef read(): return "Customer"\ndef send(value): sent.append(value)\n'
                  + source + '\nentry()\nprint(json.dumps(sent))\n')
        return json.loads(subprocess.check_output([sys.executable, '-I', '-B', '-c', driver], text=True, timeout=10))

    assert runtime() == ['Customer']
    cold = query(cache)
    assert cold.report.at('/complete') and cold.witnesses
    assert query(cache).reused
    helper.write_text('def relay(table):\n    table |= [("token", 0)]\n')
    assert runtime() == [0]
    changed = query(cache)
    assert not changed.reused and changed.report.at('/complete') and not changed.witnesses
    fresh = query(FlowCache(MemoryObjectStore()))
    assert changed.witnesses == fresh.witnesses
    for pointer in ('/input_digest', '/completion', '/cutoffs', '/events'):
        assert changed.report.at(pointer) == fresh.report.at(pointer)
