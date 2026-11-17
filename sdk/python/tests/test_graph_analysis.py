"""Compare conversion and handler paths with independent Python execution."""
import json
import subprocess
import sys
from pathlib import Path

import pytest

from fr_ir.runtime import FrClient

ROOT = Path(__file__).resolve().parents[3]


def analyze(tmp_path, source):
    (tmp_path / 'subject.py').write_text(source)
    rules = tmp_path / 'rules.json'
    rules.write_text(json.dumps({'version': 'graph-analysis', 'sources': ['read'],
                                'sinks': ['send'], 'sanitizers': {}}))
    client = FrClient(tmp_path, executable=str(ROOT / 'target/debug/fr'), max_output_bytes=1048576)
    handle = client.project('find', 'entry').definition_target().handle
    return client.project('dataflow', handle, '--summaries', '--imports',
                          '--standard-library', 'builtins', '--rules', str(rules),
                          '--context', 'html', '--steps', '4096', '--depth', '8',
                          '--bytes', '1048576').to_data()


def execute(source, value):
    driver = ('import json\nsent = []\n'
              f'def read(): return {value!r}\n'
              'def send(value): sent.append(str(value))\n' + source +
              '\nentry()\nprint(json.dumps(sent))\n')
    run = subprocess.run([sys.executable, '-I', '-c', driver], capture_output=True,
                         text=True, check=True, timeout=10)
    return json.loads(run.stdout)


@pytest.mark.parametrize('base', ['', ', 10'])
@pytest.mark.parametrize('moved', [False, True])
def test_invalid_number_input_reaches_its_bound_error_handler(tmp_path, base, moved):
    source = (f'def convert(value):\n    return int(str(value){base})\n'
              'def entry():\n    try:\n        convert(read())\n'
              '    except ValueError as problem:\n        send(str(problem))\n')
    if moved:
        source = source.replace('convert', 'parse_customer').replace('problem', 'failure')
    assert execute(source, '123') == []
    assert execute(source, 'bad-input')
    report = analyze(tmp_path, source)
    assert report['complete'], report['cutoffs']
    assert report['witnesses']
    assert not report['completion']['may_raise']
    assert any(point['role'] == 'exception-binding'
               for witness in report['witnesses']
               for point in witness['trace']['occurrences'])


def test_nonmatching_handler_does_not_receive_the_conversion_error(tmp_path):
    report = analyze(tmp_path, 'def entry():\n    try:\n        int(str(read()))\n'
                              '    except TypeError as error:\n        send(str(error))\n')
    assert report['complete'], report['cutoffs']
    assert not report['witnesses']
    assert report['completion']['may_raise']


def test_error_binding_is_deleted_after_its_handler(tmp_path):
    report = analyze(tmp_path, 'def entry():\n    error = "old"\n    try:\n'
                              '        raise ValueError(read())\n'
                              '    except ValueError as error:\n        pass\n'
                              '    send(error)\n')
    assert not report['complete']
    assert any('binding:error' in reason or 'unbound:error' in reason for reason in report['cutoffs'])
    assert not report['witnesses']


def test_unknown_conversion_protocol_stays_incomplete(tmp_path):
    report = analyze(tmp_path, 'def entry():\n    return int(read())\n')
    assert not report['complete']
    assert 'integer-conversion-type-unchecked' in report['cutoffs']


def test_valid_integer_literal_needs_no_error_path(tmp_path):
    report = analyze(tmp_path, 'def entry():\n    try:\n        int(42)\n'
                              '    except ValueError:\n        send(read())\n')
    assert report['complete'], report['cutoffs']
    assert not report['witnesses']
    assert not report['completion']['may_raise']


@pytest.mark.parametrize('body, expected, witness', [
    ('values = [0]\nalias = values\nalias[0] = read()\nsend(values[0])', ['Customer'], True),
    ('values = [read()]\nalias = values\nalias[0] = 0\nsend(values[0])', ['0'], False),
    ('values = []\nalias = values\nalias.append(read())\nsend(values[0])', ['Customer'], True),
    ('values = [read()]\nalias = values\nalias.clear()\nvalues.append(0)\nsend(values[0])', ['0'], False),
    ('values = [read()]\nother = []\nother.extend(values)\nvalues.clear()\nsend(other[0])', ['Customer'], True),
    ('values = [read()]\nalias = values\nalias = [0]\nsend(values[0])', ['Customer'], True),
])
@pytest.mark.parametrize('renamed', [False, True])
def test_list_aliases_match_python(tmp_path, body, expected, witness, renamed):
    source = 'def entry():\n' + ''.join('    ' + line + '\n' for line in body.splitlines())
    if renamed:
        source = source.replace('values', 'customers').replace('alias', 'shared')
    assert execute(source, 'Customer') == expected
    report = analyze(tmp_path, source)
    assert report['complete'], report['cutoffs']
    assert bool(report['witnesses']) is witness


@pytest.mark.parametrize('clear, witness', [(False, True), (True, False)])
def test_helper_mutation_reaches_every_alias(tmp_path, clear, witness):
    source = ('def update(items, value):\n    items[0] = value\n'
              'def entry():\n    values = [read()]\n    alias = values\n'
              f'    update(alias, {"0" if clear else "read()"})\n    send(values[0])\n')
    assert execute(source, 'Customer') == (['Customer'] if witness else ['0'])
    report = analyze(tmp_path, source)
    assert report['complete'], report['cutoffs']
    assert bool(report['witnesses']) is witness


@pytest.mark.parametrize('body, cutoff', [
    ('values = []\nsend(values[0])', 'list-index-may-raise'),
    ('values = [read()]\nsend(values[read()])', 'list-index-unchecked'),
    ('values = []\nvalues.append(values)\nsend(values)', 'nested-list-identity-unchecked'),
    ('values = [read()]\nreturn values', 'list-return-identity-unchecked'),
    ('values = [read()]\nif read():\n    values = 0\nvalues.clear()', 'list-binding-type-unchecked'),
])
def test_unsupported_list_behavior_stays_incomplete(tmp_path, body, cutoff):
    source = 'def entry():\n' + ''.join('    ' + line + '\n' for line in body.splitlines())
    report = analyze(tmp_path, source)
    assert not report['complete']
    assert cutoff in report['cutoffs'], report['cutoffs']
