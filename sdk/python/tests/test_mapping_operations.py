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
