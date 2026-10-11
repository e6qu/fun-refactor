#!/usr/bin/env python3
"""Collect bounded upstream scalar probes without promoting incomplete answers."""
import argparse
import ast
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path, PurePosixPath
import subprocess
import sys
import tarfile
import tempfile
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'tools'))
FIXTURE = ROOT / 'tests/agent-eval/python-repository-flow'
COHORT = json.loads((FIXTURE / 'cohort.json').read_text())
spec = importlib.util.spec_from_file_location('profile', ROOT / 'tools/compiler-profile.py')
profile = importlib.util.module_from_spec(spec)
spec.loader.exec_module(profile)
SCHEMA = 'fr-python-repository-flow-1'
BODIES = {
    'positive': 'return record_value(relay(relay(emit_value())))',
    'overwritten': 'value = relay(emit_value())\nvalue = 0\nreturn record_value(relay(value))',
    'unknown': 'return record_value(opaque(relay(emit_value())))',
    'steps': 'return record_value(relay(emit_value()))',
    'bytes': 'return record_value(relay(emit_value()))',
}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def bindings():
    result = profile.bindings()
    for name in ('tools/python-repository-flow.py', 'tools/python_repository_flow_oracle.py',
                 'tests/agent-eval/python-repository-flow/cohort.json',
                 'tests/agent-eval/python-repository-flow/runtime-requirements.txt'):
        result[name] = sha((ROOT / name).read_bytes())
    return result


def command(arguments, **kwargs):
    result = subprocess.run(arguments, capture_output=True, text=True, timeout=30, **kwargs)
    require(result.returncode == 0, f'{arguments[0]} failed: {result.stderr[-4096:]}')
    return result


def validate_cohort(cohort):
    limits = cohort['budgets']
    for name, low, high in [('steps', 1, 4096), ('depth', 1, 32), ('report_bytes', 2048, 1048576)]:
        require(type(limits[name]) is int and low <= limits[name] <= high, 'declared CLI budget is invalid: ' + name)


def snapshot(data, project, destination):
    limits = COHORT['budgets']
    require(len(data) <= limits['archive_bytes'], 'archive exceeds admission')
    inventory = {}
    with tarfile.open(fileobj=io.BytesIO(data), mode='r:gz') as archive:
        prefix = project['repository'].split('/')[-1] + '-' + project['revision']
        seen = set()
        expanded = 0
        for index, member in enumerate(archive):
            expanded += member.size
            require(index < limits['members'] and expanded <= limits['expanded_bytes'],
                    'expanded archive exceeds admission')
            path = PurePosixPath(member.name)
            require(not path.is_absolute() and '..' not in path.parts and path.parts[0] == prefix
                    and (member.isdir() or member.isfile()), 'unsafe snapshot member')
            relative = Path(*path.parts[1:])
            require(relative not in seen, 'duplicate snapshot member')
            seen.add(relative)
            target = destination / relative
            if member.isdir():
                target.mkdir(parents=True, exist_ok=True)
            else:
                data = archive.extractfile(member).read()
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(data)
                inventory[relative.as_posix()] = sha(data)
    require(any('license' in name.lower() or 'copying' in name.lower() for name in inventory),
            'snapshot license is absent')
    return inventory


def inspection(root, project):
    source = (root / project['path']).read_bytes().decode()
    tree = ast.parse(source)
    typing_aliases, overload_aliases = set(), set()
    for node in tree.body:
        if isinstance(node, ast.Import):
            for alias in node.names:
                name = alias.asname or alias.name.split('.')[0]
                typing_aliases.discard(name)
                overload_aliases.discard(name)
                if alias.name in ('typing', 'typing_extensions'):
                    typing_aliases.add(name)
        if isinstance(node, ast.ImportFrom):
            for alias in node.names:
                name = alias.asname or alias.name
                typing_aliases.discard(name)
                overload_aliases.discard(name)
                if node.module in ('typing', 'typing_extensions') and alias.name == 'overload':
                    overload_aliases.add(name)
    for node in tree.body:
        if isinstance(node, (ast.FunctionDef, ast.ClassDef)):
            typing_aliases.discard(node.name)
            overload_aliases.discard(node.name)
        if isinstance(node, (ast.Assign, ast.AnnAssign, ast.AugAssign, ast.Delete)):
            rebound = {child.id for child in ast.walk(node) if isinstance(child, ast.Name)
                       and isinstance(child.ctx, (ast.Store, ast.Del))}
            typing_aliases -= rebound
            overload_aliases -= rebound
    nodes = [node for node in tree.body
             if isinstance(node, ast.FunctionDef) and node.name == project['function']]
    declarations = []
    implementations = []
    for node in nodes:
        decorators = node.decorator_list
        decorator = decorators[0] if len(decorators) == 1 else None
        typed = ((isinstance(decorator, ast.Attribute) and decorator.attr == 'overload'
                  and isinstance(decorator.value, ast.Name) and decorator.value.id in typing_aliases)
                 or (isinstance(decorator, ast.Name) and decorator.id in overload_aliases))
        stub = (len(node.body) == 1 and isinstance(node.body[0], ast.Expr)
                and isinstance(node.body[0].value, ast.Constant) and node.body[0].value.value is Ellipsis)
        (declarations if typed and stub else implementations).append(node)
    require(len(implementations) == 1 and nodes[-1] is implementations[0], 'upstream helper selection is ambiguous')
    node = implementations[0]
    arguments = node.args.posonlyargs + node.args.args
    statements = list(node.body)
    if isinstance(statements[0], ast.Expr) and isinstance(statements[0].value, ast.Constant):
        statements.pop(0)
    require(len(arguments) == 1 and not node.args.kwarg and not node.args.kwonlyargs
            and not node.decorator_list and len(statements) == 1 and isinstance(statements[0], ast.Return)
            and isinstance(statements[0].value, ast.Name) and statements[0].value.id == arguments[0].arg,
            'upstream helper differs from the independently declared identity contract')
    for default in node.args.defaults:
        ast.literal_eval(default)
    return {'path': project['path'], 'name': node.name, 'parameter': arguments[0].arg,
            'unused_varargs': node.args.vararg.arg if node.args.vararg else None,
            'type_only_declarations': len(declarations),
            'source_sha256': sha(source.encode()), 'function': ast.get_source_segment(source, node)}


def prepare(root, project, variant, case):
    scope = root / project['python_root']
    entry = 'probe_entry' if variant == 'original' else 'renamed_entry'
    path = 'repository_probe.py' if variant == 'original' else 'relocated_probe/entry.py'
    module, function = project['module'], project['function']
    alias = 'relay'
    if variant == 'renamed-relocated':
        alias = 'renamed_relay'
        (scope / 'relocated_probe').mkdir(exist_ok=True)
        (scope / 'relocated_probe/__init__.py').write_text('')
    source = f'from {module} import {function} as {alias}\n\ndef {entry}():\n'
    source += ''.join('    ' + line.replace('relay(', alias + '(') + '\n' for line in BODIES[case].splitlines())
    (scope / path).write_text(source)
    (scope / 'rules.json').write_text(json.dumps(COHORT['rules']))
    return scope, path, entry, source


def runtime(scope, path, entry):
    result = []
    for value in (17, 31):
        request = {'root': str(scope), 'path': path, 'entry': entry, 'value': value}
        observed = command([sys.executable, '-I', '-B', str(ROOT / 'tools/python_repository_flow_oracle.py')],
                           input=json.dumps(request), cwd=scope)
        result.append(json.loads(observed.stdout))
    return result


def analyze(binary, scope, path, entry, case):
    def fr(arguments):
        process = subprocess.run([str(binary), '--json', '-C', str(scope), *arguments],
                                 capture_output=True, text=True, timeout=30)
        return {'arguments': arguments, 'exit_code': process.returncode,
                'stderr': process.stderr, 'report': json.loads(process.stdout)}
    found = fr(['project', 'find', entry, '--in', path])
    require(found['exit_code'] == 0 and len(found['report']['rows']) == 1
            and found['report']['page']['next'] is None, 'entry discovery is not exact')
    handle = found['report']['rows'][0][found['report']['columns'].index('handle')]
    limits = COHORT['budgets']
    arguments = ['project', 'dataflow', handle, '--summaries', '--imports', '--rules', str(scope / 'rules.json'),
                 '--context', 'html', '--steps', str(1 if case == 'steps' else limits['steps']),
                 '--depth', str(limits['depth']), '--bytes', str(1 if case == 'bytes' else limits['report_bytes'])]
    return [found, fr(arguments)]


def assessment(row):
    invocations = row['invocations']
    result = invocations[1]
    report = result['report']
    if result['exit_code']:
        require(row['case'] == 'bytes' and 'invalid response byte budget' in json.dumps(report),
                'unexpected CLI refusal: ' + json.dumps(report)[:4096])
        return {'status': 'refused', 'no_path': False}
    require(report['schema'] == 'fr-dataflow-1' and report['claim'] == 'possible-value-propagation'
            and type(report['complete']) is bool, 'flow report authority changed')
    complete, witnesses = report['complete'], report['witnesses']
    require(not complete or not report['cutoffs'], 'complete report retains analysis cutoffs')
    if row['case'] in ('unknown', 'steps'):
        require(not complete and report['cutoffs'], 'unknown effects or exhausted budget claimed completeness')
    if complete:
        require(bool(witnesses) == (row['case'] == 'positive'), 'complete flow disagrees with independent inspection')
    else:
        require(report['cutoffs'], 'incomplete report lacks boundary evidence')
    return {'status': 'complete' if complete else 'incomplete', 'no_path': complete and not witnesses}


def summary(rows):
    demonstrated = all(row['assessment']['status'] == 'complete'
                       for row in rows if row['case'] in ('positive', 'overwritten'))
    indexed = {(row['project'], row['variant'], row['case']): row for row in rows}
    metamorphic = True
    for row in rows:
        if row['variant'] == 'original':
            other = indexed[(row['project'], 'renamed-relocated', row['case'])]
            metamorphic &= (row['assessment'] == other['assessment']
                            and bool(row['invocations'][1]['report'].get('witnesses'))
                            == bool(other['invocations'][1]['report'].get('witnesses')))
    return {'repositories': len({row['project'] for row in rows}), 'cases': len(rows),
            'complete_cases': sum(row['assessment']['status'] == 'complete' for row in rows),
            'incomplete_cases': sum(row['assessment']['status'] == 'incomplete' for row in rows),
            'refused_cases': sum(row['assessment']['status'] == 'refused' for row in rows),
            'metamorphic_agreement': metamorphic,
            'acceptance_demonstrated': demonstrated and metamorphic and len({row['project'] for row in rows}) == 3}


def check_flow_sources(row, scope):
    found, result = row['invocations']
    require(found['exit_code'] == 0 and len(found['report']['rows']) == 1
            and found['report']['page']['next'] is None, 'entry discovery changed')
    selected = dict(zip(found['report']['columns'], found['report']['rows'][0]))
    require(selected['path'] == row['path'] and selected['name'] == row['entry'], 'entry selection changed')
    if result['exit_code']:
        return
    flow = result['report']
    require(flow['target'] == selected['handle'] and flow['revision'] == found['report']['revision']
            and flow['inputs']['source'] == {'path': row['path'], 'digest': profile.compiler.digest(row['source'])}
            and flow['inputs']['summary_mode'] is True and flow['context'] == 'html'
            and flow['inputs']['selection']['name'] == row['entry']
            and flow['inputs']['budget'] == {'steps': 1 if row['case'] == 'steps' else COHORT['budgets']['steps'],
                                            'depth': COHORT['budgets']['depth'],
                                            'bytes': COHORT['budgets']['report_bytes']}, 'flow input identity changed')
    rules = {name: COHORT['rules'][name] for name in ('version', 'sources', 'sinks', 'propagators', 'sanitizers')}
    require(flow['rules_digest'] == sha(json.dumps(rules, separators=(',', ':')).encode())
            and flow['input_digest'] == profile.compiler.digest(flow['inputs']), 'caller contracts or input digest changed')
    for path, digest in flow['inputs']['modules']['files'].items():
        file = (scope / path).resolve()
        require(file.is_relative_to(scope.resolve()) and profile.compiler.digest(file.read_bytes().decode()) == digest,
                'analyzed module differs from the retained source')
    for witness in flow['witnesses']:
        points = witness['trace']['occurrences']
        require(witness['claim'] == 'possible-value-propagation; path feasibility unchecked.'
                and witness['rules'] == COHORT['rules']['version'] and witness['sink'] == 'record_value'
                and points and points[0]['role'] == 'source' and witness['site']['role'] == 'sink'
                and witness['site'] in points, 'witness authority changed')
        for point in points:
            file = (scope / point['path']).resolve()
            require(file.is_relative_to(scope.resolve()) and point['revision'] == flow['revision'],
                    'witness source identity changed')
            source = file.read_bytes().decode()
            location = point['location']
            start, end = location['span']['start'], location['span']['end']
            require(location == profile.location(source, start, end), 'witness coordinates changed')
            if point['role'] in ('source', 'sink'):
                call = ast.parse(source.encode()[start:end].decode(), mode='eval').body
                name = 'emit_value' if point['role'] == 'source' else 'record_value'
                require(isinstance(call, ast.Call) and isinstance(call.func, ast.Name) and call.func.id == name,
                        'witness endpoint changed')


def checked_coverage(run, jobs, source_commit, repository):
    require(run['status'] == 'completed' and run['conclusion'] == 'success'
            and run['head_sha'] == source_commit
            and run['path'] == '.github/workflows/refinement-evidence.yml'
            and run['repository']['full_name'] == repository
            and run['head_repository']['full_name'] == repository,
            'coverage must pass on the exact same-repository frozen collector revision')
    require(any(job['name'] == 'coverage' and job['conclusion'] == 'success'
                and job['run_id'] == run['id'] for job in jobs), 'coverage producer did not succeed')
    return {'run_id': run['id'], 'source_commit': run['head_sha'], 'url': run['html_url']}


def audit(value, directory):
    require(value['schema'] == SCHEMA and value['source_bindings'] == bindings()
            and value['cohort'] == COHORT, 'report source or cohort changed')
    require(value['phase'] in ('coverage', 'held-out'), 'collection phase changed')
    require((value['coverage'] is None) == (value['phase'] == 'coverage'), 'held-out provenance changed')
    if value['phase'] == 'held-out':
        require(value['coverage']['source_commit'] == value['source_commit'], 'held-out analyzer changed')
    projects = [p for p in COHORT['projects'] if p['role'] == value['phase']]
    require(set(value['snapshots']) == {p['id'] for p in projects}, 'snapshot cohort changed')
    expected = {(p['id'], variant, case) for p in projects
                for variant in ('original', 'renamed-relocated') for case in BODIES}
    keys = [(row['project'], row['variant'], row['case']) for row in value['cases']]
    require(len(keys) == len(set(keys)) and set(keys) == expected, 'missing or duplicate planned cells')
    with tempfile.TemporaryDirectory(prefix='fr-repository-audit-') as temporary:
        for project in projects:
            data = (directory / (project['id'] + '.tar.gz')).read_bytes()
            metadata = value['snapshots'][project['id']]
            require(metadata['sha256'] == sha(data) and metadata['bytes'] == len(data), 'snapshot digest changed')
            root = Path(temporary) / project['id']
            require(snapshot(data, project, root) == metadata['files'], 'snapshot source inventory changed')
            inspected = inspection(root, project)
            require(inspected == metadata['inspection'], 'independent source inspection changed')
            for row in [r for r in value['cases'] if r['project'] == project['id']]:
                scope, path, entry, source = prepare(root, project, row['variant'], row['case'])
                require(row['source'] == source and row['path'] == path and row['entry'] == entry,
                        'declared entry changed')
                observed = runtime(scope, path, entry)
                expected_runtime = [{'events': [['source', n], ['sink', 0 if row['case'] == 'overwritten' else n]],
                                     'returned': 0 if row['case'] == 'overwritten' else n} for n in (17, 31)]
                require(row['runtime'] == observed == expected_runtime, 'upstream runtime outcome changed')
                check_flow_sources(row, scope)
                require(row['assessment'] == assessment(row), 'incomplete evidence was promoted')
    require(value['summary'] == summary(value['cases']), 'summary accounting changed')
    return value['summary']


def measure(args):
    require(os.environ.get('GITHUB_ACTIONS') == 'true', 'collect only on GitHub runners')
    validate_cohort(COHORT)
    command(['git', 'diff', '--exit-code', COHORT['analyzer_revision'], '--',
             'src', 'grammars', 'sdk/python/src', 'Cargo.toml', 'Cargo.lock'], cwd=ROOT)
    source_commit = command(['git', 'rev-parse', 'HEAD'], cwd=ROOT).stdout.strip()
    coverage = None
    if args.phase == 'held-out':
        require(args.coverage_run, 'held-out collection requires prior successful coverage')
        coverage = json.loads(command(['gh', 'api', f'repos/{os.environ["GITHUB_REPOSITORY"]}/actions/runs/{args.coverage_run}']).stdout)
        jobs = json.loads(command(['gh', 'api', f'repos/{os.environ["GITHUB_REPOSITORY"]}/actions/runs/{args.coverage_run}/jobs']).stdout)['jobs']
        coverage = checked_coverage(coverage, jobs, source_commit, os.environ['GITHUB_REPOSITORY'])
    args.output.mkdir(parents=True, exist_ok=False)
    value = {'schema': SCHEMA, 'cohort': COHORT, 'source_bindings': bindings(), 'source_commit': source_commit,
             'binary_sha256': sha(args.fr.read_bytes()), 'phase': args.phase, 'coverage': coverage,
             'snapshots': {}, 'cases': [], 'summary': {}}
    def save():
        (args.output / 'result.json').write_text(json.dumps(value, indent=2, sort_keys=True) + '\n')
    with tempfile.TemporaryDirectory(prefix='fr-repository-flow-') as temporary:
        for project in [p for p in COHORT['projects'] if p['role'] == args.phase]:
            url = f'https://codeload.github.com/{project["repository"]}/tar.gz/{project["revision"]}'
            with urllib.request.urlopen(url, timeout=30) as response:
                data = response.read(COHORT['budgets']['archive_bytes'] + 1)
            root = Path(temporary) / project['id']
            inventory = snapshot(data, project, root)
            (args.output / (project['id'] + '.tar.gz')).write_bytes(data)
            inspected = inspection(root, project)
            value['snapshots'][project['id']] = {'url': url, 'sha256': sha(data), 'bytes': len(data),
                                                'files': inventory, 'inspection': inspected}
            save()
            for variant in ('original', 'renamed-relocated'):
                for case in BODIES:
                    scope, path, entry, source = prepare(root, project, variant, case)
                    row = {'project': project['id'], 'variant': variant, 'case': case,
                           'source': source, 'path': path, 'entry': entry,
                           'runtime': runtime(scope, path, entry),
                           'invocations': analyze(args.fr.resolve(), scope, path, entry, case)}
                    value['cases'].append(row)
                    save()
                    row['assessment'] = assessment(row)
                    print(project['id'], variant, case, row['assessment'], flush=True)
                    save()
    value['summary'] = summary(value['cases'])
    save()
    return audit(value, args.output)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--phase', choices=['coverage', 'held-out'])
    parser.add_argument('--coverage-run', type=int)
    parser.add_argument('--fr', type=Path, default=ROOT / 'target/debug/fr')
    parser.add_argument('--output', type=Path)
    parser.add_argument('--audit', type=Path)
    args = parser.parse_args()
    if args.audit:
        result = audit(json.loads(args.audit.read_text()), args.audit.parent)
    else:
        require(args.phase and args.output, 'collection needs a phase and new output directory')
        result = measure(args)
    print(json.dumps(result, sort_keys=True))


if __name__ == '__main__':
    main()
