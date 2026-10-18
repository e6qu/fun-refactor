#!/usr/bin/env python3
"""Replay pinned scalar helpers through full imports and isolated declarations."""
import argparse
import ast
import copy
import importlib.util
import json
from pathlib import Path
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'sdk/python/src'))
from fr_ir.flow_summaries import FunctionSummaries
from fr_ir.runtime import FrReport
spec = importlib.util.spec_from_file_location('flow', ROOT / 'tools/python-repository-flow.py')
flow = importlib.util.module_from_spec(spec)
spec.loader.exec_module(flow)
flow.SCHEMA = 'fr-python-repository-declarations'
flow.COHORT = copy.deepcopy(flow.COHORT)
flow.COHORT['schema'] = 'fr-python-repository-declarations-cohort'
# Freeze native and SDK code before replaying the familiar libraries.
flow.COHORT['analyzer_revision'] = '9c7c4d3f03ca676a11039515acdffe4ec71260cb'
flow.COHORT['analyzer_trees'] = {
    'src': 'aa577d7b3a0b573831cd60304b1432a6f4057ef7',
    'grammars': '34eae6d2a08b6a412f466895e78a0c36001a0aa7',
    'crates': '2ed9ad9fe2de47060b969c7afc139c275b911ed2',
    'queries': '465c5e1e81c76abac02baec561fff07168b3cea2',
    'build.rs': 'f58ae81bfa48f7a5a48950329b941ff0f13ccb51',
    'Cargo.toml': '5082ca88f07ea95e34ef1c056a58fc9da50d3172',
    'Cargo.lock': '4985716cd6ed1788a45fafecd8b22aac14685ba1',
    'native-source-inputs.txt': 'e7ba79122be183f8d827d3bc2c262f7b63e58bc4',
    'sdk/python/src': '9bdfd85927fb74023daaf00c82d8b1a0741b2c3c',
}
flow.COHORT['holdout'] = 'All three libraries are already-seen coverage. Jaraco.functools remains unopened and does not participate in this regression collection.'
for project in flow.COHORT['projects']:
    project['role'] = 'coverage'
original_bindings = flow.bindings
HELPERS = [
    {'id': 'toolz-return-none', 'project': 'toolz', 'module': 'toolz.functoolz', 'path': 'toolz/functoolz.py',
     'function': 'return_none', 'parameters': ['exc'], 'expression': 'None',
     'companion_path': 'toolz/functoolz.py', 'companion': 'apply'},
    {'id': 'funcy-inc', 'project': 'funcy', 'module': 'funcy.primitives', 'path': 'funcy/primitives.py',
     'function': 'inc', 'parameters': ['x'], 'expression': 'x + 1',
     'companion_path': 'funcy/funcs.py', 'companion': 'compose'},
    {'id': 'funcy-dec', 'project': 'funcy', 'module': 'funcy.primitives', 'path': 'funcy/primitives.py',
     'function': 'dec', 'parameters': ['x'], 'expression': 'x - 1',
     'companion_path': 'funcy/funcs.py', 'companion': 'compose'},
    {'id': 'pydash-add', 'project': 'pydash', 'module': 'pydash.numerical', 'path': 'src/pydash/numerical.py',
     'function': 'add', 'parameters': ['a', 'b'], 'expression': 'a + b',
     'companion_path': 'src/pydash/utilities.py', 'companion': 'range_'},
]
CONTEXTS = ('full-package', 'isolated-declarations')
STANDALONE_HELPERS = frozenset({'funcy-inc', 'funcy-dec'})
VARIANTS = ('original', 'renamed-relocated')
flow.COHORT['additional_helpers'] = HELPERS
flow.COHORT['isolated_initialization'] = 'probe_metadata = 0'
flow.COHORT['standalone_modules'] = sorted(STANDALONE_HELPERS)
flow.COHORT['standalone_policy'] = 'Copy the complete original module unchanged without package parents. This context cannot establish full-package acceptance.'


def contexts(helper):
    return (*CONTEXTS, 'standalone-module') if helper['id'] in STANDALONE_HELPERS else CONTEXTS


def implementation(source, name):
    declarations = [node for node in ast.parse(source).body
                    if isinstance(node, ast.FunctionDef) and node.name == name]
    runtime = []
    for node in declarations:
        overload = any(isinstance(item, ast.Attribute) and item.attr == 'overload'
                       and isinstance(item.value, ast.Name) and item.value.id == 't'
                       for item in node.decorator_list)
        stub = (len(node.body) == 1 and isinstance(node.body[0], ast.Expr)
                and isinstance(node.body[0].value, ast.Constant) and node.body[0].value.value is Ellipsis)
        if not overload or not stub:
            runtime.append(node)
    flow.require(len(runtime) == 1 and declarations[-1] is runtime[0],
                 'runtime helper selection is ambiguous')
    node = runtime[0]
    arguments = node.args.posonlyargs + node.args.args + node.args.kwonlyargs
    flow.require(not node.decorator_list and node.returns is None
                 and all(item.annotation is None for item in arguments)
                 and (node.args.vararg is None or node.args.vararg.annotation is None)
                 and (node.args.kwarg is None or node.args.kwarg.annotation is None)
                 and not node.args.defaults and not node.args.kw_defaults,
                 'isolated declaration requires a plain inert signature')
    return node, ast.get_source_segment(source, node)


def inspection(root, helper):
    source = (root / helper['path']).read_bytes().decode()
    node, function = implementation(source, helper['function'])
    arguments = node.args.posonlyargs + node.args.args
    statements = list(node.body)
    if (statements and isinstance(statements[0], ast.Expr)
            and isinstance(statements[0].value, ast.Constant) and isinstance(statements[0].value.value, str)):
        statements.pop(0)
    flow.require([item.arg for item in arguments] == helper['parameters']
                 and node.args.vararg is None and node.args.kwarg is None and not node.args.kwonlyargs
                 and len(statements) == 1 and isinstance(statements[0], ast.Return)
                 and ast.dump(statements[0].value) == ast.dump(ast.parse(helper['expression'], mode='eval').body),
                 'scalar body differs from its independent specification')
    companion_source = (root / helper['companion_path']).read_bytes().decode()
    companion, companion_function = implementation(companion_source, helper['companion'])
    flow.require(companion.args.vararg is not None
                 and any(isinstance(item, ast.Name) and item.id == companion.args.vararg.arg
                         for statement in companion.body for item in ast.walk(statement)),
                 'companion must reference its deferred variadic parameter')
    return {'source_sha256': flow.sha(source.encode()), 'function': function,
            'companion_source_sha256': flow.sha(companion_source.encode()), 'companion': companion_function}


def prepare(root, project, helper, context, variant, case, inspected, temporary):
    if context == 'full-package':
        scope = root / project['python_root']
        module = helper['module']
    else:
        scope = temporary / (context + '-' + helper['id'])
        scope.mkdir(exist_ok=True)
        module = 'pinned_functions'
        if context == 'isolated-declarations':
            (scope / 'pinned_functions.py').write_text(flow.COHORT['isolated_initialization'] + '\n\n'
                                                     + inspected['function'] + '\n\n' + inspected['companion'] + '\n')
        else:
            flow.require(context == 'standalone-module' and helper['id'] in STANDALONE_HELPERS,
                         'standalone module selection changed')
            source = (root / helper['path']).read_bytes()
            flow.require(flow.sha(source) == inspected['source_sha256']
                         and not any(isinstance(node, (ast.Import, ast.ImportFrom)) for node in ast.parse(source).body),
                         'standalone source must be unchanged and import-free')
            (scope / 'pinned_functions.py').write_bytes(source)
    entry = 'probe_entry' if variant == 'original' else 'renamed_entry'
    path = 'repository_probe.py' if variant == 'original' else 'relocated_probe/entry.py'
    alias = 'relay' if variant == 'original' else 'renamed_relay'
    if variant == 'renamed-relocated':
        (scope / 'relocated_probe').mkdir(exist_ok=True)
        (scope / 'relocated_probe/__init__.py').write_text('')
    extra = ', 1' if len(helper['parameters']) == 2 else ''
    selected, blocked = f'{alias}(emit_value(){extra})', f'{alias}(value{extra})'
    body = {
        'positive': f'return record_value({selected})',
        'overwritten': f'value = {selected}\nvalue = 0\nreturn record_value({blocked})',
        'unknown': f'return record_value(opaque({selected}))',
        'steps': f'return record_value({selected})',
        'bytes': f'return record_value({selected})',
    }[case]
    source = f'from {module} import {helper["function"]} as {alias}\n\ndef {entry}():\n'
    source += ''.join('    ' + line + '\n' for line in body.splitlines())
    (scope / path).write_text(source)
    (scope / 'rules.json').write_text(json.dumps(flow.COHORT['rules']))
    return scope, path, entry, source


def expected_runtime(helper, case):
    def result(value):
        if helper['expression'] == 'None':
            return None
        return value - 1 if helper['expression'] == 'x - 1' else value + 1
    return [{'events': [['source', value], ['sink', result(0 if case == 'overwritten' else value)]],
             'returned': result(0 if case == 'overwritten' else value)} for value in (17, 31)]


def assessment(row, helper):
    result = row['invocations'][1]
    if result['exit_code']:
        flow.require(row['case'] == 'bytes' and 'invalid response byte budget' in json.dumps(result['report']),
                     'unexpected CLI refusal')
        return {'status': 'refused', 'no_path': False}
    report = result['report']
    flow.require(report['schema'] == 'fr-dataflow-1' and report['semantics'] == 'python-scalar-summaries'
                 and report['claim'] == 'possible-value-propagation' and type(report['complete']) is bool,
                 'declaration report authority changed')
    typed = FunctionSummaries.from_report(FrReport(report, ()))
    if row['context'] in ('isolated-declarations', 'standalone-module') and row['case'] in ('positive', 'overwritten'):
        flow.require(report['complete'] and not report['cutoffs'], 'inert declaration still blocks scalar flow')
        flow.require(bool(report['witnesses']) == (row['case'] == 'positive' and helper['expression'] != 'None'),
                     'isolated scalar flow disagrees with its specification')
    else:
        flow.require(not report['complete'] and report['cutoffs'], 'unsupported effects claimed completeness')
    if row['case'] not in ('steps', 'bytes'):
        name = ('pinned_functions.py' if row['context'] != 'full-package'
                else str(Path(helper['path']).relative_to(project_for(helper)['python_root'])))
        name += '::' + helper['function']
        summary = typed.for_function(name)
        flow.require(summary.normal_return is True,
                     'non-identity helper did not enter the summary solver')
    return {'status': 'complete' if report['complete'] else 'incomplete',
            'no_path': report['complete'] and not report['witnesses']}


def project_for(helper):
    return next(project for project in flow.COHORT['projects'] if project['id'] == helper['project'])


def declaration_summary(rows):
    counts = {name: sum(row['assessment']['status'] == name for row in rows)
              for name in ('complete', 'incomplete', 'refused')}
    indexed = {(row['helper'], row['context'], row['variant'], row['case']): row for row in rows}
    agreement = all(row['assessment'] == indexed[(row['helper'], row['context'], 'renamed-relocated', row['case'])]['assessment']
                    and bool(row['invocations'][1]['report'].get('witnesses')) ==
                    bool(indexed[(row['helper'], row['context'], 'renamed-relocated', row['case'])]['invocations'][1]['report'].get('witnesses'))
                    for row in rows if row['variant'] == 'original')
    return {'repositories': len({row['project'] for row in rows}), 'helpers': len(HELPERS), 'cases': len(rows),
            'runtime_executions': len(rows) * 2, 'complete_cases': counts['complete'],
            'incomplete_cases': counts['incomplete'], 'refused_cases': counts['refused'],
            'contexts': {context: {name: sum(row['context'] == context and row['assessment']['status'] == name
                                           for row in rows) for name in ('complete', 'incomplete', 'refused')}
                         for context in (*CONTEXTS, 'standalone-module')},
            'metamorphic_agreement': agreement, 'repository_acceptance_demonstrated': False}


def declaration_inventory(value):
    expected = {(helper['id'], context, variant, case) for helper in HELPERS
                for context in contexts(helper) for variant in VARIANTS for case in flow.BODIES}
    keys = [(row['helper'], row['context'], row['variant'], row['case']) for row in value['declaration_cases']]
    flow.require(len(keys) == len(set(keys)) and set(keys) == expected, 'missing or duplicate declaration cells')
    flow.require(set(value['declaration_inspection']) == {helper['id'] for helper in HELPERS},
                 'declaration inspection inventory changed')
    projects = {helper['id']: helper['project'] for helper in HELPERS}
    flow.require(all(row['project'] == projects[row['helper']] for row in value['declaration_cases']),
                 'declaration repository identity changed')


def declarations(value, directory, binary=None):
    if binary is not None:
        value['declaration_cases'], value['declaration_inspection'] = [], {}
    else:
        declaration_inventory(value)
    def save():
        (directory / 'result.json').write_text(json.dumps(value, indent=2, sort_keys=True) + '\n')
    with tempfile.TemporaryDirectory(prefix='fr-declarations-') as temporary_path:
        temporary = Path(temporary_path)
        for project in flow.COHORT['projects']:
            data = (directory / (project['id'] + '.tar.gz')).read_bytes()
            root = temporary / project['id']
            flow.require(flow.snapshot(data, project, root) == value['snapshots'][project['id']]['files'],
                         'declaration snapshot inventory changed')
            for helper in [item for item in HELPERS if item['project'] == project['id']]:
                inspected = inspection(root, helper)
                if binary is not None:
                    value['declaration_inspection'][helper['id']] = inspected
                    save()
                else:
                    flow.require(value['declaration_inspection'][helper['id']] == inspected, 'declaration source changed')
                for context in contexts(helper):
                    for variant in VARIANTS:
                        for case in flow.BODIES:
                            scope, path, entry, source = prepare(root, project, helper, context, variant, case, inspected, temporary)
                            if binary is not None:
                                row = {'project': project['id'], 'helper': helper['id'], 'context': context,
                                       'variant': variant, 'case': case, 'source': source, 'path': path, 'entry': entry,
                                       'runtime': flow.runtime(scope, path, entry),
                                       'invocations': flow.analyze(binary, scope, path, entry, case)}
                                value['declaration_cases'].append(row)
                                save()
                            else:
                                row = next(row for row in value['declaration_cases'] if
                                           (row['helper'], row['context'], row['variant'], row['case']) ==
                                           (helper['id'], context, variant, case))
                                flow.require((row['path'], row['entry'], row['source']) == (path, entry, source),
                                             'declaration adapter changed')
                                flow.require(row['runtime'] == flow.runtime(scope, path, entry), 'runtime observation changed')
                            flow.require(row['runtime'] == expected_runtime(helper, case), 'scalar runtime outcome changed')
                            flow.check_flow_sources(row, scope)
                            judged = assessment(row, helper)
                            if binary is not None:
                                row['assessment'] = judged
                                print(helper['id'], context, variant, case, judged, flush=True)
                                save()
                            else:
                                flow.require(row['assessment'] == judged, 'incomplete declaration evidence was promoted')
    if binary is not None:
        declaration_inventory(value)
        value['declaration_summary'] = declaration_summary(value['declaration_cases'])
        save()
    else:
        flow.require(value['declaration_summary'] == declaration_summary(value['declaration_cases']),
                     'declaration summary accounting changed')


def bindings():
    value = original_bindings()
    value['tools/python-fixed-arity-probes.py'] = flow.sha(Path(__file__).read_bytes())
    value['sdk/python/src/fr_ir/flow_summaries.py'] = flow.sha((ROOT / 'sdk/python/src/fr_ir/flow_summaries.py').read_bytes())
    value['sdk/python/src/fr_ir/flow_initialization.py'] = flow.sha((ROOT / 'sdk/python/src/fr_ir/flow_initialization.py').read_bytes())
    value['tools/test-frozen-analyzer.py'] = flow.sha((ROOT / 'tools/test-frozen-analyzer.py').read_bytes())
    return value


flow.bindings = bindings


def check(value):
    for project in flow.COHORT['projects']:
        name = str(Path(project['path']).relative_to(project['python_root'])) + '::' + project['function']
        for row in value['cases']:
            if row['project'] != project['id'] or row['case'] in ('steps', 'bytes'):
                continue
            report = row['invocations'][1]['report']
            flow.require(report['semantics'] == 'python-scalar-summaries'
                         and 'unsupported-parameter-contract:' + name not in report['cutoffs'],
                         'fixed-slot helper still hits the parameter boundary')
            summary = report['function_summaries']['functions'].get(name)
            flow.require(summary is not None and summary['normal_return'] is True,
                         'upstream fixed-slot helper did not enter the summary solver')
    flow.require(value['summary']['acceptance_demonstrated'] is False,
                 'identity-adapter regressions cannot close repository acceptance')
    return value['summary']


def startup_counts(value):
    # Print one declared full-package example per familiar project. The reports
    # themselves retain every case; this is an investigation aid, not acceptance.
    result = {}
    for row in value['cases']:
        if row['variant'] == 'original' and row['case'] == 'positive':
            report = row['invocations'][1]['report']
            result[row['project']] = report['initialization']['by_file']
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--fr', type=Path, default=ROOT / 'target/debug/fr')
    parser.add_argument('--output', type=Path)
    parser.add_argument('--audit', type=Path)
    args = parser.parse_args()
    if args.audit:
        value = json.loads(args.audit.read_bytes())
        flow.audit(value, args.audit.parent)
        declarations(value, args.audit.parent)
    else:
        flow.require(args.output, 'choose collection or an offline audit')
        args.phase, args.coverage_run = 'coverage', None
        flow.measure(args)
        value = json.loads((args.output / 'result.json').read_bytes())
        declarations(value, args.output, args.fr.resolve())
    check(value)
    print(json.dumps({'identity': value['summary'], 'declarations': value['declaration_summary']}, sort_keys=True))
    print('Unchecked startup statements by loaded file: ' + json.dumps(startup_counts(value), sort_keys=True))


if __name__ == '__main__':
    main()
