#!/usr/bin/env python3
"""Compare a declared scalar Python contract with CPython and retained fr reports."""
import argparse
import ast
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
from functools import lru_cache

ROOT = Path(__file__).resolve().parents[1]
sys.path[:0] = [str(ROOT / 'tools'), str(ROOT / 'sdk/python/src')]
from python_contract_cases import EXAMPLES, RULES, cases
from fr_ir.flow_summaries import FunctionSummaries
from fr_ir.flow_dependencies import FlowDependencies
from python_contract_oracle import execute
from fr_ir.runtime import FrReport
from evidence_basis import file_digest

spec = importlib.util.spec_from_file_location('profile', ROOT / 'tools/compiler-profile.py')
profile = importlib.util.module_from_spec(spec)
spec.loader.exec_module(profile)
SCHEMA = 'fr-python-contract-3'
LIMITS = ('Finite scalar examples, not full Python correctness or path feasibility. '
          'Implicit exceptions, aliases and dynamic dispatch remain outside this model. '
          'Runtime observations cover only declared inputs. Retained output is not execution attestation. '
          'No live-agent, token-efficiency or repository-generalization claim.')


def require(condition, message):
    if not condition:
        raise ValueError(message)


def bindings():
    result = profile.bindings()
    for path in ('tools/python-contract.py', 'tools/python_contract_cases.py', 'tools/python_contract_oracle.py',
                 'docs/python-contract.md', 'sdk/python/tests/test_flow_imports.py',
                 'sdk/python/tests/test_expression_control.py', 'sdk/python/src/fr_ir/flow_summaries.py',
                 'sdk/python/src/fr_ir/flow_dependencies.py', 'sdk/python/src/fr_ir/flow_initialization.py'):
        result[path] = file_digest(ROOT / path)
    return dict(sorted(result.items()))


def oracle(case):
    # Cache only executions of the immutable authored input. Every retained row is
    # still compared, including its exact runtime types, on every audit.
    return json.loads(oracle_json(json.dumps(case, sort_keys=True, ensure_ascii=False)))


@lru_cache(maxsize=512)
def oracle_json(encoded):
    case = json.loads(encoded)
    if not case['imports']:
        return json.dumps(execute(case, {}), ensure_ascii=False)
    result = subprocess.run([sys.executable, '-I', '-B', str(ROOT / 'tools/python_contract_oracle.py')],
                            input=encoded, capture_output=True, text=True, check=True, timeout=10)
    return result.stdout


def ast_locations(case, path=None):
    source = case['files'][path] if path else case['source']
    lines = source.encode().splitlines(keepends=True)
    offsets = [0]
    for line in lines:
        offsets.append(offsets[-1] + len(line))
    spans, calls = set(), {}
    for node in ast.walk(ast.parse(source)):
        if not hasattr(node, 'end_lineno') or node.end_lineno is None:
            continue
        span = (offsets[node.lineno - 1] + node.col_offset, offsets[node.end_lineno - 1] + node.end_col_offset)
        spans.add(span)
        if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)) and node.decorator_list:
            first = min(item.lineno for item in node.decorator_list)
            spans.add((offsets[first - 1] + node.col_offset, span[1]))
        if isinstance(node, ast.Call):
            calls[span] = node.func.id if isinstance(node.func, ast.Name) else None
    return spans, calls


def configuration(case):
    names = case['names']
    return {'version': 'python-contract-1', 'sources': [names['source']], 'sinks': [names['sink']],
            'sanitizers': {names['clean']: 'html'}, 'propagators': [names['forward']]}


def flow_command(case, handle, root):
    return ['project', 'dataflow', handle, '--summaries', *(['--imports'] if case['imports'] else []),
            '--rules', root + '/rules.json', '--context', 'html', '--steps', str(case['steps']),
            '--depth', str(case['depth']), '--bytes', str(case['bytes'])]


def audit_signature(function, definition, path, source, revision):
    arguments = definition.args
    parameters = [(node, kind) for nodes, kind in (
        (arguments.posonlyargs, 'positional-only'), (arguments.args, 'positional-or-keyword'),
        (arguments.kwonlyargs, 'keyword-only')) for node in nodes]
    require([(item['name'], item['kind']) for item in function['signature']]
            == [(node.arg, kind) for node, kind in parameters], 'callee signature changed')
    positional = arguments.posonlyargs + arguments.args
    defaults = [None] * (len(positional) - len(arguments.defaults)) + arguments.defaults + arguments.kw_defaults
    lines = source.encode().splitlines(keepends=True)
    for item, (node, _), default in zip(function['signature'], parameters, defaults):
        start = sum(map(len, lines[:node.lineno - 1])) + node.col_offset
        point = item['site']
        require(point['path'] == path and point['revision'] == revision
                and point['location'] == profile.location(source, start, start + len(node.arg.encode())),
                'parameter source correspondence changed')
        require((item['default'] is None) == (default is None), 'literal default presence changed')
        if default is not None:
            point = item['default']
            start, end = point['location']['span']['start'], point['location']['span']['end']
            expected_start = sum(map(len, lines[:default.lineno - 1])) + default.col_offset
            expected_end = sum(map(len, lines[:default.end_lineno - 1])) + default.end_col_offset
            require(point['path'] == path and point['revision'] == revision
                    and point['location'] == profile.location(source, start, end)
                    and start <= expected_start < expected_end <= end
                    and ast.dump(ast.parse(source.encode()[start:end].decode(), mode='eval').body) == ast.dump(default),
                    'default source correspondence changed')


def audit_case(row, case):
    require(profile.compiler.digest(row['case']) == profile.compiler.digest(case), 'declared example changed')
    require(profile.compiler.digest(row['runtime']) == profile.compiler.digest(oracle(case))
            == profile.compiler.digest(case['expected_runtime']), 'CPython outcome or evaluation order changed')
    flow, found = row['flow'], row['find']
    require(row['configuration'] == configuration(case), 'caller contracts changed')
    require(found['page']['remaining'] == 0 and len(found['rows']) == 1, 'entry selection incomplete')
    definition = dict(zip(found['columns'], found['rows'][0]))
    require(definition['path'] == case['path'], 'entry file changed')
    span = definition['location']['name']['span']
    require(case['source'].encode()[span['start']:span['end']].decode() == case['names']['entry']
            and definition['location']['name'] == profile.location(case['source'], span['start'], span['end']),
            'entry coordinates changed')
    require(row['commands'] == [['project', 'find', case['names']['entry']],
                                flow_command(case, definition['handle'], row['root'])], 'analysis inputs changed')
    invocations = row['invocations']
    require(len(invocations) == 2 and all(type(item['exit_code']) is int for item in invocations)
            and invocations[0] == {'exit_code': 0, 'stderr': ''},
            'entry lookup did not succeed cleanly')
    if case['expected_model']['outcome'] == 'refused':
        expected = case['expected_model']
        require(flow is None and profile.compiler.digest(row['error']) == profile.compiler.digest(expected['error']),
                'CLI refusal cannot be an analysis report')
        require(type(invocations[1]['exit_code']) is int and invocations[1]['exit_code'] == expected['exit_code']
                and invocations[1]['stderr'].startswith('Error: ' + expected['error']['error']['message']),
                'CLI refusal status or diagnostic changed')
        return
    require(row['error'] is None and invocations[1] == {'exit_code': 0, 'stderr': ''},
            'analysis report did not succeed cleanly')
    require(flow['schema'] == 'fr-dataflow-1' and flow['semantics'] == 'python-scalar-summaries'
            and flow['claim'] == 'possible-value-propagation' and flow['revision'] == found['revision'],
            'analysis identity or claim changed')
    require(flow['target'] == definition['handle'] and flow['inputs']['summary_mode'] is True
            and flow['inputs']['context'] == 'html' and flow['inputs']['selection']['name'] == case['names']['entry']
            and flow['inputs']['source'] == {'path': case['path'], 'digest': profile.compiler.digest(case['source'])}
            and flow['inputs']['budget']['steps'] == case['steps']
            and flow['inputs']['budget']['depth'] == case['depth']
            and flow['inputs']['budget']['bytes'] == case['bytes'], 'analysis input basis changed')
    ordered_rules = {key: configuration(case)[key] for key in ('version', 'sources', 'sinks', 'propagators', 'sanitizers')}
    rules_digest = hashlib.sha256(json.dumps(ordered_rules, ensure_ascii=False, separators=(',', ':')).encode()).hexdigest()
    require(flow['rules_digest'] == flow['inputs']['rules'] == rules_digest
            and flow['context'] == 'html' and flow['inputs']['configuration'] == profile.compiler.digest({})
            and flow['inputs']['selection']['span'] == definition['location']['definition']['span']
            and flow['input_digest'] == profile.compiler.digest(flow['inputs']), 'report input digest changed')
    assumptions = [
        'scalar values; no aliases, monkey patching or implicit flows.',
        'branch feasibility unchecked', 'external rules are caller-supplied contracts.',
        'explicit raises terminate; implicit exceptions, handlers and resource effects are outside this model.',
        'finite origin sets; joins lose branch correlation; traces are derivations, not executable paths.']
    require(flow['assumptions'] == assumptions, 'analysis assumptions changed')
    require(flow['complete'] is case['expected_model']['complete'], 'completeness changed')
    require(bool(flow['witnesses']) is case['expected_model']['witness'], 'possible-flow answer changed')
    cutoff = case['expected_model']['cutoff']
    require((not flow['cutoffs']) if cutoff is None else cutoff in json.dumps(flow['cutoffs']), 'analysis boundary changed')
    summaries = FunctionSummaries.from_report(FrReport(flow, ()))
    completion = case['expected_model']['completion']
    if completion is not None:
        entry = summaries.for_function((case['path'] + '::' if case['imports'] else '') + case['names']['entry'])
        require(profile.compiler.digest(flow['completion']) == profile.compiler.digest(completion)
                and entry.normal_return is completion['normal_return']
                and entry.may_raise is completion['may_raise'], 'normal or exceptional return changed')
    require(summaries.expression_control.path_feasibility is False, 'path feasibility claim changed')
    require(flow['assignment_control']['implicit_exceptions'] is False
            and flow['call_binding']['implicit_exceptions'] is False, 'implicit exception boundary changed')
    locations = {path: ast_locations(case, path) for path in case['files']}
    for site in flow['initialization']['sites']:
        point = site['occurrence']
        path = point['path']
        require(path in locations, 'initialization file changed')
        location = point['location']
        start, end = location['span']['start'], location['span']['end']
        require((start, end) in locations[path][0]
                and location == profile.location(case['files'][path], start, end),
                'initialization source correspondence changed')
    sources = sorted((path, *span) for path, (_, calls) in locations.items()
                     for span, name in calls.items() if name == case['names']['source'])
    sinks = sorted((path, *span) for path, (_, calls) in locations.items()
                   for span, name in calls.items() if name == case['names']['sink'])
    if case['imports']:
        dependencies = FlowDependencies.from_report(FrReport(flow, ()))
        modules = flow['inputs']['modules']
        admitted = case['module_files'] if case['module_files'] is not None else case['files']
        require(modules['files'] == {path: profile.compiler.digest(case['files'][path]) for path in admitted},
                'module source inventory changed')
        require(dependencies.complete is modules['complete'], 'module completeness changed')
        directories = {str(parent) for path in case['files'] for parent in Path(path).parents if str(parent) != '.'}
        for lookup in [modules['entry'], *modules['lookups']]:
            for path, fact in lookup['candidates'].items():
                expected = ({'status': 'source', 'digest': profile.compiler.digest(case['files'][path])}
                            if path in case['files'] else
                            {'status': 'directory' if path in directories else 'missing'})
                require(fact == expected, 'module candidate differs from declared files')
        expected = case['module_lookup']
        if expected is not None:
            matches = [lookup for lookup in modules['lookups']
                       if lookup['importer'] == expected['importer'] and lookup['alias'] == expected['alias']]
            require(len(matches) == 1 and matches[0]['resolution'] == expected['resolution']
                    and len(matches[0]['binding_chain']) == expected['binding_chain_length'],
                    'module binding boundary changed')
    for identity, function in flow['function_summaries']['functions'].items():
        path, name = identity.split('::', 1) if case['imports'] else (case['path'], identity)
        require(path in case['files'], 'callee file changed')
        definitions = [node for node in ast.walk(ast.parse(case['files'][path]))
                       if isinstance(node, ast.FunctionDef) and node.name == name]
        require(len(definitions) == 1, 'callee definition changed')
        audit_signature(function, definitions[0], path, case['files'][path], flow['revision'])
    pairs = []
    for witness in flow['witnesses']:
        trace = witness['trace']['occurrences']
        endpoints = [index for index, point in enumerate(trace) if point == witness['site']]
        require(witness['claim'] == 'possible-value-propagation; path feasibility unchecked.'
                and witness['rules'] == configuration(case)['version'] and witness['sink'] == case['names']['sink']
                and trace[0]['role'] == 'source' and witness['site']['role'] == 'sink'
                and len(endpoints) == 1
                and all(point['role'] == 'summary-call' for point in trace[endpoints[0] + 1:]),
                'witness authority or endpoints changed')
        for occurrence in trace:
            path = occurrence['path']
            require(path in locations, 'witness file changed')
            spans, calls = locations[path]
            location = occurrence['location']
            start, end = location['span']['start'], location['span']['end']
            require((start, end) in spans and location == profile.location(case['files'][path], start, end)
                    and occurrence['revision'] == flow['revision'],
                    'witness source correspondence changed')
            if occurrence['role'] in ('source', 'sink'):
                require(calls.get((start, end)) == case['names'][occurrence['role']], 'witness call identity changed')
            if occurrence['role'] in ('summary-call', 'call-result'):
                require((start, end) in calls, 'summary occurrence is not a call')
        first = trace[0]['location']['span']
        last = witness['site']['location']['span']
        pairs.append([sources.index((trace[0]['path'], first['start'], first['end'])),
                      sinks.index((witness['site']['path'], last['start'], last['end']))])
    require(sorted(pairs) == case['expected_model']['pairs'], 'source-to-sink pairing changed')


def summary():
    return {'rules': len(RULES), 'examples': len(EXAMPLES), 'cases': len(cases()),
            'renamed_relocated_cases': len(EXAMPLES), 'contract_complete': True,
            'documented_difference_cases': sum(case['difference'] is not None for case in cases()),
            'incomplete_cases': sum(case['expected_model']['outcome'] == 'report'
                                    and not case['expected_model']['complete'] for case in cases()),
            'refused_cases': sum(case['expected_model']['outcome'] == 'refused' for case in cases())}


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
    require(profile.compiler.digest(value['summary']) == profile.compiler.digest(summary()), 'contract conclusion changed')
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
    errors = []
    with tempfile.TemporaryDirectory(prefix='fr-python-contract-') as temporary:
        for index, case in enumerate(cases()):
            root = Path(temporary) / str(index)
            for name, source in case['files'].items():
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(source.encode())
            rules = root / 'rules.json'
            rules.write_text(json.dumps(configuration(case)))
            commands, invocations = [], []
            def fr(args):
                commands.append(args)
                result = subprocess.run([str(binary), '--json', '-C', str(root), *args],
                                        capture_output=True, text=True, timeout=30)
                invocations.append({'exit_code': result.returncode, 'stderr': result.stderr})
                return json.loads(result.stdout)
            found = fr(['project', 'find', case['names']['entry']])
            require(invocations[0]['exit_code'] == 0, invocations[0]['stderr'])
            handle = found['rows'][0][found['columns'].index('handle')]
            result = fr(flow_command(case, handle, str(root)))
            refused = invocations[1]['exit_code'] != 0
            row = {'case': case, 'runtime': oracle(case), 'find': found,
                   'configuration': configuration(case), 'root': str(root), 'commands': commands,
                   'invocations': invocations, 'flow': None if refused else result,
                   'error': result if refused else None}
            value['cases'].append(row)
            save(value, output)
            try:
                audit_case(row, case)
            except Exception as error:
                errors.append(case['id'] + ': ' + str(error))
                print(errors[-1], file=sys.stderr, flush=True)
            else:
                print(case['id'] + ': checked', file=sys.stderr, flush=True)
    if errors:
        value['summary'] = {'contract_complete': False, 'errors': errors}
        save(value, output)
        raise ValueError('\n'.join(errors))
    value['summary'] = summary()
    return value


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--fr', type=Path, default=ROOT / 'target/debug/fr')
    parser.add_argument('--output', type=Path)
    parser.add_argument('--audit', type=Path)
    parser.add_argument('--check-runtime', action='store_true', help='Check authored CPython expectations without fr')
    args = parser.parse_args()
    if args.check_runtime:
        require(not args.audit and not args.output, 'runtime-only checks do not collect analysis evidence')
        for case in cases():
            require(profile.compiler.digest(oracle(case)) == profile.compiler.digest(case['expected_runtime']),
                    case['id'] + ': authored CPython outcome changed')
        print(json.dumps({'runtime_cases': len(cases()), 'analysis_checked': False}, sort_keys=True))
        return
    value = json.loads(args.audit.read_text()) if args.audit else measure(args.fr, args.output)
    result = audit(value)
    save(value, args.output)
    print(json.dumps(result, sort_keys=True))


if __name__ == '__main__':
    main()
