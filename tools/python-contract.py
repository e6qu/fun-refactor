#!/usr/bin/env python3
"""Compare a declared scalar Python contract with CPython and retained fr reports."""
import argparse
import ast
import importlib.util
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
sys.path[:0] = [str(ROOT / 'tools'), str(ROOT / 'sdk/python/src')]
from python_contract_cases import EXAMPLES, RULES, cases
from fr_ir.flow_summaries import FunctionSummaries
from fr_ir.runtime import FrReport
from evidence_basis import file_digest

spec = importlib.util.spec_from_file_location('profile', ROOT / 'tools/compiler-profile.py')
profile = importlib.util.module_from_spec(spec)
spec.loader.exec_module(profile)
SCHEMA = 'fr-python-contract-1'
LIMITS = ('Finite scalar examples, not full Python correctness or path feasibility. '
          'Implicit exceptions, aliases and dynamic dispatch remain outside this model. '
          'Runtime observations cover only declared inputs. Retained output is not execution attestation. '
          'No live-agent, token-efficiency or repository-generalization claim.')


def require(condition, message):
    if not condition:
        raise ValueError(message)


def bindings():
    result = profile.bindings()
    for path in ('tools/python-contract.py', 'tools/python_contract_cases.py', 'docs/python-contract.md',
                 'sdk/python/tests/test_expression_control.py', 'sdk/python/src/fr_ir/flow_summaries.py'):
        result[path] = file_digest(ROOT / path)
    return dict(sorted(result.items()))


def oracle(case):
    events = []
    def source():
        events.append(['source', 17])
        return 17
    def sink(value):
        events.append(['sink', value])
        return value
    def clean(value):
        events.append(['clean', value])
        return 0
    def forward(value):
        events.append(['forward', value])
        return value
    functions = {'source': source, 'sink': sink, 'clean': clean, 'forward': forward}
    namespace = {case['names'][name]: function for name, function in functions.items()}
    exec(compile(case['source'], case['path'], 'exec'), namespace)
    returned, raised = None, None
    try:
        returned = namespace[case['names']['entry']]()
    except Exception as error:
        raised = type(error).__name__
    return {'events': events, 'returned': returned, 'raised': raised}


def ast_locations(case):
    lines = case['source'].encode().splitlines(keepends=True)
    offsets = [0]
    for line in lines:
        offsets.append(offsets[-1] + len(line))
    spans, calls = set(), {}
    for node in ast.walk(ast.parse(case['source'])):
        if not hasattr(node, 'end_lineno') or node.end_lineno is None:
            continue
        span = (offsets[node.lineno - 1] + node.col_offset, offsets[node.end_lineno - 1] + node.end_col_offset)
        spans.add(span)
        if isinstance(node, ast.Call) and isinstance(node.func, ast.Name):
            calls[span] = node.func.id
    return spans, calls


def configuration(case):
    names = case['names']
    return {'version': 'python-contract-1', 'sources': [names['source']], 'sinks': [names['sink']],
            'sanitizers': {names['clean']: 'html'}, 'propagators': [names['forward']]}


def audit_case(row, case):
    require(row['case'] == case, 'declared example changed')
    require(row['runtime'] == oracle(case) == case['expected_runtime'], 'CPython outcome or evaluation order changed')
    flow, found = row['flow'], row['find']
    require(row['configuration'] == configuration(case), 'caller contracts changed')
    require(found['page']['remaining'] == 0 and len(found['rows']) == 1, 'entry selection incomplete')
    definition = dict(zip(found['columns'], found['rows'][0]))
    require(definition['path'] == case['path'], 'entry file changed')
    span = definition['location']['name']['span']
    require(case['source'].encode()[span['start']:span['end']].decode() == case['names']['entry']
            and definition['location']['name'] == profile.location(case['source'], span['start'], span['end']),
            'entry coordinates changed')
    require(flow['schema'] == 'fr-dataflow-1' and flow['semantics'] == 'python-scalar-summaries-5'
            and flow['claim'] == 'possible-value-propagation' and flow['revision'] == found['revision'],
            'analysis identity or claim changed')
    require(flow['complete'] is case['expected_model']['complete'], 'completeness changed')
    require(bool(flow['witnesses']) is case['expected_model']['witness'], 'possible-flow answer changed')
    cutoff = case['expected_model']['cutoff']
    require((not flow['cutoffs']) if cutoff is None else cutoff in json.dumps(flow['cutoffs']), 'analysis boundary changed')
    summaries = FunctionSummaries.from_report(FrReport(flow, ()))
    require(summaries.expression_control.path_feasibility is False, 'path feasibility claim changed')
    require(flow['assignment_control']['implicit_exceptions'] is False
            and flow['call_binding']['implicit_exceptions'] is False, 'implicit exception boundary changed')
    spans, calls = ast_locations(case)
    for witness in flow['witnesses']:
        trace = witness['trace']['occurrences']
        require(witness['claim'] == 'possible-value-propagation; path feasibility unchecked.'
                and witness['rules'] == configuration(case)['version'] and witness['sink'] == case['names']['sink']
                and trace[0]['role'] == 'source' and trace[-1] == witness['site'] and trace[-1]['role'] == 'sink',
                'witness authority or endpoints changed')
        for occurrence in trace:
            location = occurrence['location']
            start, end = location['span']['start'], location['span']['end']
            require((start, end) in spans and location == profile.location(case['source'], start, end)
                    and occurrence['path'] == case['path'] and occurrence['revision'] == flow['revision'],
                    'witness source correspondence changed')
            if occurrence['role'] in ('source', 'sink'):
                require(calls.get((start, end)) == case['names'][occurrence['role']], 'witness call identity changed')
    expected = ['project', 'dataflow', definition['handle'], '--summaries', '--rules', row['root'] + '/rules.json',
                '--context', 'html', '--steps', str(case['steps']), '--bytes', '262144']
    require(row['commands'] == [['project', 'find', case['names']['entry']], expected], 'analysis inputs changed')


def summary():
    return {'rules': len(RULES), 'examples': len(EXAMPLES), 'cases': len(cases()),
            'renamed_relocated_cases': len(EXAMPLES), 'contract_complete': True,
            'documented_difference_cases': sum(case['difference'] is not None for case in cases()),
            'incomplete_cases': sum(not case['expected_model']['complete'] for case in cases())}


def audit(value):
    require(value['schema'] == SCHEMA and value['source_bindings'] == bindings(), 'contract inputs changed')
    require(value['rules'] == RULES and value['limits'] == LIMITS, 'declared scope changed')
    require(re.fullmatch('[0-9a-f]{40}', value['source_commit'])
            and re.fullmatch('[0-9a-f]{64}', value['binary_sha256']), 'collection identity missing')
    require(value['python']['version'] and value['python']['path']
            and re.fullmatch('[0-9a-f]{64}', value['python']['sha256']), 'Python identity missing')
    expected = {case['id']: case for case in cases()}
    actual = {row['case']['id']: row for row in value['cases']}
    require(len(actual) == len(value['cases']) and set(actual) == set(expected), 'missing or duplicate example')
    for key, case in expected.items():
        try:
            audit_case(actual[key], case)
        except Exception as error:
            raise ValueError(key + ': ' + str(error)) from error
    require(value['summary'] == summary(), 'contract conclusion changed')
    return summary()


def save(value, path):
    if path:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + '\n')


def measure(binary, output):
    require(os.environ.get('GITHUB_ACTIONS') == 'true', 'collect only on GitHub runners')
    binary = binary.resolve()
    value = {'schema': SCHEMA, 'source_bindings': bindings(), 'rules': RULES, 'limits': LIMITS,
             'source_commit': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
             'binary_sha256': file_digest(binary), 'python': {'version': sys.version, 'path': sys.executable,
                                                          'sha256': file_digest(sys.executable)},
             'cases': [], 'summary': {'contract_complete': False}}
    with tempfile.TemporaryDirectory(prefix='fr-python-contract-') as temporary:
        for index, case in enumerate(cases()):
            root = Path(temporary) / str(index)
            path = root / case['path']
            path.parent.mkdir(parents=True)
            path.write_text(case['source'])
            rules = root / 'rules.json'
            rules.write_text(json.dumps(configuration(case)))
            commands = []
            def fr(args):
                commands.append(args)
                result = subprocess.run([str(binary), '--json', '-C', str(root), *args],
                                        capture_output=True, text=True, timeout=30)
                require(result.returncode == 0, result.stderr)
                return json.loads(result.stdout)
            found = fr(['project', 'find', case['names']['entry']])
            handle = found['rows'][0][found['columns'].index('handle')]
            flow = fr(['project', 'dataflow', handle, '--summaries', '--rules', str(rules), '--context', 'html',
                       '--steps', str(case['steps']), '--bytes', '262144'])
            row = {'case': case, 'runtime': oracle(case), 'find': found, 'flow': flow,
                   'configuration': configuration(case), 'root': str(root), 'commands': commands}
            value['cases'].append(row)
            save(value, output)
            audit_case(row, case)
            print(case['id'] + ': checked', file=sys.stderr, flush=True)
    value['summary'] = summary()
    return value


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--fr', type=Path, default=ROOT / 'target/debug/fr')
    parser.add_argument('--output', type=Path)
    parser.add_argument('--audit', type=Path)
    args = parser.parse_args()
    value = json.loads(args.audit.read_text()) if args.audit else measure(args.fr, args.output)
    result = audit(value)
    save(value, args.output)
    print(json.dumps(result, sort_keys=True))


if __name__ == '__main__':
    main()
