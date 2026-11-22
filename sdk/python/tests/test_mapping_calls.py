"""Compare dictionary identities, variadic binding and failure order with Python."""
import pytest

from test_graph_analysis import analyze, execute
from test_list_sequences import program


@pytest.mark.parametrize('body,expected,witness', [
    ('values = {"value": read()}\nsend(values["value"])', ['Customer'], True),
    ('values = {"value": read()}\nalias = values\nalias["value"] = 0\nsend(values["value"])', ['0'], False),
    ('values = {"value": read()}\nsaved = values["value"]\nvalues["value"] = 0\nsend(saved)', ['Customer'], True),
    ('inner = [read()]\nvalues = {"value": inner}\nvalues["value"][0] = 0\nsend(inner[0])', ['0'], False),
    ('values = {"value": read()}\nouter = [values]\nouter[0]["value"] = 0\nsend(values["value"])', ['0'], False),
    ('values = {"value": read()}\ncopy = values.copy()\ncopy["value"] = 0\nsend(values["value"])', ['Customer'], True),
    ('values = {"value": [read()]}\ncopy = {**values}\ncopy["value"][0] = 0\nsend(values["value"][0])', ['0'], False),
    ('values = {"value": read(), "value": 0}\nsend(values["value"])', ['0'], False),
    ('values = {1: read(), True: 0}\nsend(values[1])', ['0'], False),
    ('values = {"value": read()}\ncopy = {**values, "value": 0}\nsend(copy["value"])', ['0'], False),
    ('values = {}\nvalues["value"] = read()\nsend(values.pop("value"))', ['Customer'], True),
    ('values = {"value": read()}\nvalues.clear()\nsend(values.get("value", 0))', ['0'], False),
    ('values = {"value": read()}\nvalues.update({"value": 0})\nsend(values["value"])', ['0'], False),
    ('values = {"value": read()}\nvalues.update(value=0)\nsend(values["value"])', ['0'], False),
    ('values = {}\nvalues.setdefault("value", read())\nsend(values["value"])', ['Customer'], True),
    ('values = {"value": 0}\nsend(values.setdefault("value", read()))', ['0'], False),
    ('values = {"value": read()}\ndel values["value"]\nsend(values.get("value", 0))', ['0'], False),
    ('values = dict(value=read())\nsend(values["value"])', ['Customer'], True),
    ('values = {"value": read()}\ntry:\n    values["value"] = 0\n    values.pop("missing")\nexcept LookupError:\n    send(values["value"])', ['0'], False),
    ('values = {"value": read()}\ntry:\n    del values["value"], values["missing"]\nexcept KeyError:\n    send(values.get("value", 0))', ['0'], False),
])
@pytest.mark.parametrize('renamed', [False, True])
def test_dictionary_state_matches_python(tmp_path, body, expected, witness, renamed):
    source = program(body, renamed=renamed)
    assert execute(source, 'Customer') == expected
    report = analyze(tmp_path, source)
    assert report['complete'], report['cutoffs']
    assert bool(report['witnesses']) is witness


@pytest.mark.parametrize('helper,call,expected,witness', [
    ('def relay(*values):\n    return values[0]\n', 'relay(read(), 0)', ['Customer'], True),
    ('def relay(*values):\n    return values[1]\n', 'relay(read(), 0)', ['0'], False),
    ('def relay(**values):\n    return values["value"]\n', 'relay(value=read(), other=0)', ['Customer'], True),
    ('def relay(**values):\n    values["value"] = 0\n    return values["value"]\n', 'relay(**{"value": read()})', ['0'], False),
    ('def relay(value, /, **options):\n    return options["value"]\n', 'relay(0, value=read())', ['Customer'], True),
    ('def relay(first, *rest, last=0, **options):\n    return rest[0]\n', 'relay(0, read(), last=0, extra=0)', ['Customer'], True),
    ('def relay(value=0, *, other=0):\n    return value\n', 'relay(**{"value": read(), "other": 0})', ['Customer'], True),
    ('def relay(**options):\n    return forward(**options)\ndef forward(*, value):\n    return value\n', 'relay(value=read())', ['Customer'], True),
    ('def relay(*values):\n    return forward(*values)\ndef forward(value):\n    return value\n', 'relay(read())', ['Customer'], True),
])
@pytest.mark.parametrize('renamed', [False, True])
def test_variadic_binding_and_forwarding(tmp_path, helper, call, expected, witness, renamed):
    source = program(f'send({call})', helper, renamed=renamed)
    assert execute(source, 'Customer') == expected
    report = analyze(tmp_path, source)
    assert report['complete'], report['cutoffs']
    assert bool(report['witnesses']) is witness


@pytest.mark.parametrize('helper,call,expected', [
    ('def relay(**options):\n    options["value"][0] = 0\n', 'relay(value=values)', ['0']),
    ('def relay(*args):\n    args[0][0] = 0\n', 'relay(values)', ['0']),
    ('def relay(**options):\n    options["value"] = [0]\n', 'relay(value=values)', ['Customer']),
    ('def relay(options):\n    try:\n        return options["missing"]\n    finally:\n        options["value"][0] = 0\n', 'relay({"value": values})', ['0']),
])
def test_helper_aliases_and_cleanup(tmp_path, helper, call, expected):
    source = program(f'values = [read()]\ntry:\n    {call}\nexcept KeyError:\n    pass\nsend(values[0])', helper)
    assert execute(source, 'Customer') == expected
    report = analyze(tmp_path, source)
    assert report['complete'], report['cutoffs']
    assert bool(report['witnesses']) is ('Customer' in expected)


@pytest.mark.parametrize('call', [
    'relay(**{"value": 0}, **{"value": 1}, later=change(values))',
    'relay(**{"value": 0}, value=1, later=change(values))',
    'relay(**{1: 0}, later=change(values))',
    'relay(**0, later=change(values))',
])
def test_keyword_merge_errors_preserve_evaluation_order(tmp_path, call):
    source = program(f'values = [read()]\ntry:\n    {call}\nexcept TypeError:\n    send(values[0])',
        'def change(values):\n    values[0] = 0\n    return 0\ndef relay(**options):\n    send("unreached")\n')
    expected = execute(source, 'Customer')
    assert expected in (['Customer'], ['0'])
    report = analyze(tmp_path, source)
    assert report['complete'], report['cutoffs']
    assert bool(report['witnesses']) is ('Customer' in expected)


@pytest.mark.parametrize('body', [
    'values = {read(): 0}\nsend(values)',
    'values = {"value": read()}\nsend(values[unknown()])',
    'values = {"value": read()}\nvalues.append(0)\nsend(values)',
    'values = {"value": read()}\nsend(values[:])',
])
def test_unknown_mapping_protocols_remain_incomplete(tmp_path, body):
    report = analyze(tmp_path, program(body))
    assert not report['complete'] and report['cutoffs']


@pytest.mark.parametrize('helper', [
    'def relay(value):\n    return dict(token=value)\n',
    'def relay(value):\n    table = {"token": value}\n    try:\n        return table\n    finally:\n        table["token"] = 0\n',
])
def test_returned_mapping_factory_and_cleanup(tmp_path, helper):
    source = program('table = relay(read())\nsend(table["token"])', helper)
    expected = execute(source, 'Customer')
    report = analyze(tmp_path, source)
    assert report['complete'], report['cutoffs']
    assert bool(report['witnesses']) is ('Customer' in expected)


@pytest.mark.parametrize('method', ['get', 'pop', 'setdefault'])
def test_mapping_results_preserve_noniterable_facts(tmp_path, method):
    source = program(f'table = {{"token": 0}}\ntry:\n    first, = table.{method}("token")\nexcept TypeError:\n    send(read())')
    assert execute(source, 'Customer') == ['Customer']
    report = analyze(tmp_path, source)
    assert report['complete'] and report['witnesses'], report['cutoffs']


@pytest.mark.parametrize('count,complete', [(64, True), (65, False)])
@pytest.mark.parametrize('form', ['literal', 'constructor', 'variadic'])
def test_mapping_and_variadic_item_limits(tmp_path, count, complete, form):
    pairs = [f'"k{i}": 0' for i in range(count - 1)] + ['"token": read()']
    expression = '{' + ', '.join(pairs) + '}'
    helper = ''
    if form == 'constructor':
        expression = 'dict(' + ', '.join(f'k{i}=0' for i in range(count - 1)) + ', token=read())'
    elif form == 'variadic':
        expression = 'relay(**' + expression + ')'
        helper = 'def relay(**options):\n    return options\n'
    source = program(f'table = {expression}\nsend(table["token"])', helper)
    assert execute(source, 'Customer') == ['Customer']
    report = analyze(tmp_path, source)
    assert report['complete'] is complete, report['cutoffs']
    if complete:
        assert report['witnesses']
    else:
        assert report['cutoffs']


@pytest.mark.parametrize("module", ["helpers", "moved_helpers"])
def test_imported_variadic_mapping_edits_invalidate_cached_answers(tmp_path, module):
    import json
    import subprocess
    import sys
    from fr_ir.runtime import FrClient
    from fr_ir.context import MemoryObjectStore
    from fr_ir.flow import FlowCache
    from test_graph_analysis import ROOT

    helper = tmp_path / (module + '.py')
    helper.write_text('def relay(**options):\n    return options["token"]\n')
    source = program('send(relay(token=read()))', f'from {module} import relay\n')
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
    helper.write_text('def relay(**options):\n    options["token"] = 0\n    return options["token"]\n')
    assert runtime() == [0]
    changed = query(cache)
    assert not changed.reused and changed.report.at('/complete') and not changed.witnesses
    fresh = query(FlowCache(MemoryObjectStore()))
    assert changed.witnesses == fresh.witnesses
    for pointer in ('/input_digest', '/completion', '/cutoffs', '/events'):
        assert changed.report.at(pointer) == fresh.report.at(pointer)

