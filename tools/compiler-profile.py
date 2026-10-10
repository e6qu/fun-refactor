#!/usr/bin/env python3
"""Declare and check a finite Rust/Python explanation profile on hosted runners."""
import argparse
import ast
import importlib.util
import json
import os
from pathlib import Path
import platform
import re
import subprocess
import sys
import tempfile
from types import SimpleNamespace

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'tools'))
from evidence_basis import file_digest


def load(name):
    spec = importlib.util.spec_from_file_location(name, ROOT / 'tools' / (name + '.py'))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


compiler = load('compiler-evidence-acceptance')
semantic = load('semantic-evidence-acceptance')
FEATURES = {
    'rust-repeated': ('indexed references', 'Repeated calls, Unicode columns and Rust flow refusal', 'tests/investigation.rs'),
    'rust-shadow': ('indexed references', 'A local closure must not resolve to the shadowed function', 'tests/investigation.rs'),
    'python-repeated': ('source correspondence', 'Repeated expression origins and exact call locations', 'tests/semantic_evidence.rs'),
    'python-shadow': ('unsupported correspondence', 'Duplicate declarations retain absent origins', 'tests/semantic_evidence.rs'),
    'python-normalized': ('unsupported correspondence', 'Lowered modulo expressions retain absent origins', 'tests/semantic_evidence.rs'),
    'python-local': ('incomplete value flow', 'A later local assignment blocks an external source claim', 'tests/flow_fixed_point.rs'),
    'python-handler': ('incomplete value flow', 'Exception handlers remain an explicit analysis boundary', 'tests/flow_fixed_point.rs'),
    'python-positive': ('value flow', 'A configured scalar reaches a sink through a helper', 'tests/investigation.rs'),
    'python-overwrite': ('value flow', 'An overwrite removes the configured scalar before its use', 'tests/investigation.rs'),
}
PROFILE = {key: {'basis': basis, 'feature': feature, 'existing_test': test}
           for key, (basis, feature, test) in FEATURES.items()}
PROFILE['rust-compiler'] = {'basis': 'compiler diagnostics', 'feature': 'Syntax acceptance, cfg-dependent rejection, exact E0308 spans and stale build inputs',
                            'existing_test': 'tests/compiler_evidence.rs'}
PROFILE['python-checked-origins'] = {'basis': 'executed checks', 'feature': 'AST correspondence, runtime outcomes and invalidated checked plans',
                                     'existing_test': 'tests/semantic_evidence.rs'}


def bindings():
    paths = set(compiler.BINDINGS + semantic.BINDINGS)
    paths.update(str(path.relative_to(ROOT)) for path in (ROOT / 'src').rglob('*.rs'))
    paths.update(row['existing_test'] for row in PROFILE.values())
    paths.add('tools/compiler-profile.py')
    return {path: file_digest(ROOT / path) for path in sorted(paths)}


def cases():
    py = (ROOT / 'tests/agent-eval/semantic-evidence/subject.py').read_text()
    rust = (ROOT / 'tests/agent-eval/compiler-evidence/subject.rs').read_text()
    templates = {
        'rust-repeated': (rust, 'repeated', 'width', 10),
        'rust-shadow': ('pub fn pick() -> i32 { 3 }\npub fn total() -> i32 { let pick = || 9; pick() }\n', 'total', 'pick', 9),
        'python-repeated': (py, 'total', 'café', [1, 5, -1]),
        'python-shadow': ('def total(x):\n    return 1\ndef total(y):\n    return 2\n', 'total', None, [2, 2, 2]),
        'python-normalized': ('def total(x):\n    return x % 3\n', 'total', None, [0, 1, 1]),
        'python-local': ('def total():\n    sink(source())\n    source = 1\n', 'total', None, {'error': 'UnboundLocalError', 'sinks': []}),
        'python-handler': ('def total():\n    try:\n        return 1 // 0\n    except ZeroDivisionError:\n        return 7\n', 'total', None, {'value': 7, 'sinks': []}),
        'python-positive': ('def helper(value):\n    return value\ndef total():\n    value = source()\n    sink(helper(value))\n', 'total', None, {'value': None, 'sinks': [37]}),
        'python-overwrite': ('def helper(value):\n    return value\ndef total():\n    value = source()\n    value = 0\n    sink(helper(value))\n', 'total', None, {'value': None, 'sinks': [0]}),
    }
    result = []
    renames = {'total': 'résultat', 'repeated': 'again', 'width': 'measure', 'pick': 'choose',
               'café': 'thé', 'helper': 'relay', 'source': 'read_value', 'sink': 'emit'}
    for key, (text, target, callee, expected) in templates.items():
        language = key.split('-')[0]
        for variant in ('original', 'renamed-relocated'):
            source = text
            moved = variant != 'original'
            if moved:
                source = re.sub(r'\b(' + '|'.join(renames) + r')\b', lambda m: renames[m[0]], source)
                source = ('// moved π\n\n' if language == 'rust' else '# moved π\n\n') + source
            result.append({'id': key + '/' + variant, 'feature': key, 'variant': variant, 'language': language,
                'path': ('nested/renamed/' if moved else '') + ('subject.rs' if language == 'rust' else 'subject.py'),
                'source': source, 'target': renames.get(target, target) if moved else target,
                'callee': renames.get(callee, callee) if moved else callee, 'expected_runtime': expected,
                'rules': {'version': 'compiler-profile-1', 'sources': [renames['source'] if moved else 'source'],
                          'sinks': [renames['sink'] if moved else 'sink']}})
    return result


def require(condition, message):
    if not condition:
        raise ValueError(message)


def command(argv, root):
    process = subprocess.run(argv, cwd=root, capture_output=True, text=True, timeout=60)
    return {'argv': [str(a) for a in argv], 'exit_code': process.returncode,
            'stdout': process.stdout, 'stderr': process.stderr}


def report(event):
    require(event['exit_code'] == 0, 'fr command failed: ' + event['stderr'])
    return json.loads(event['stdout'])


def python_oracle(case):
    tree = ast.parse(case['source'], filename=case['path'])
    lines = case['source'].encode().splitlines(keepends=True)
    starts = [sum(map(len, lines[:i])) for i in range(len(lines))]
    spans = []
    for node in ast.walk(tree):
        if isinstance(node, ast.Call) and isinstance(node.func, ast.Name) and node.func.id == case['callee']:
            def span(n):
                return [starts[n.lineno - 1] + n.col_offset, starts[n.end_lineno - 1] + n.end_col_offset]
            spans.append({'name': span(node.func), 'expression': span(node)})
    sinks = []
    scope = {case['rules']['sources'][0]: lambda: 37, case['rules']['sinks'][0]: sinks.append}
    exec(compile(tree, case['path'], 'exec'), scope)
    function = scope[case['target']]
    if isinstance(case['expected_runtime'], list):
        observed = [function(value) for value in (0, 1, -2)]
    else:
        try:
            observed = {'value': function(), 'sinks': sinks}
        except Exception as error:
            observed = {'error': type(error).__name__, 'sinks': sinks}
    return {'accepted': True, 'runtime': observed, 'calls': sorted(spans, key=lambda s: s['name'])}


def location(source, start, end):
    data = source.encode()
    def point(offset):
        prefix = data[:offset].decode()
        return {'line': prefix.count('\n') + 1, 'col': len(prefix.rsplit('\n', 1)[-1]) + 1}
    return {'span': {'start': start, 'end': end}, 'range': {'start': point(start), 'end': point(end)}}


def rust_call_spans(case):
    spans = []
    for match in re.finditer(r'\b' + re.escape(case['callee']) + r'\(', case['source']):
        if re.search(r'\bfn\s+$', case['source'][:match.start()]):
            continue
        start = len(case['source'][:match.start()].encode())
        spans.append([start, start + len(case['callee'].encode())])
    return spans


def collect_case(case, binary, rustc, root):
    path = root / case['path']
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(case['source'])
    (root / '.fr').mkdir()
    (root / '.fr/rules.json').write_text(json.dumps(case['rules']))
    events = {}
    def fr(key, *args):
        event = command([str(binary), '--json', '-C', str(root), *args], root)
        events[key] = event
        return event
    found = report(fr('find', 'project', 'find', case['target']))
    target = found['rows'][-1][found['columns'].index('handle')]
    if case['callee']:
        fr('calls', 'project', 'calls', '.', '--limit', '64')
    if case['feature'] in ('python-repeated', 'python-shadow', 'python-normalized'):
        fr('origins', 'project', 'semantic', target, '--body', '--origins', '--origin-limit', '64')
    else:
        fr('flow', 'project', 'dataflow', target, '--rules', '.fr/rules.json', '--steps', '1024', '--bytes', '65536')
    if case['language'] == 'python':
        oracle = python_oracle(case)
    else:
        harness = case['source'] + '\nfn main() { println!("{}", ' + case['target'] + '()); }\n'
        driver = root.parent / (root.name + '-driver.rs')
        driver.write_text(harness)
        executable = root.parent / (root.name + '-driver')
        built = command([rustc, '--edition=2021', '--crate-name', 'profile', str(driver), '-o', str(executable)], root)
        executed = command([str(executable)], root) if built['exit_code'] == 0 else None
        oracle = {'harness': harness, 'build': built, 'execute': executed,
                  'calls': rust_call_spans(case)}
    return {'case': case, 'events': events, 'oracle': oracle}


def audit_case(row, case):
    require(row['case'] == case, 'profile source, configuration or feature changed')
    events = row['events']
    found = report(events['find'])
    columns = found['columns']
    definitions = found['rows']
    require(len(definitions) == (2 if case['feature'] == 'python-shadow' else 1)
            and found['page']['remaining'] == 0, 'declaration coverage changed')
    for definition in definitions:
        loc = definition[columns.index('location')]['name']
        span = loc['span']
        require(loc == location(case['source'], span['start'], span['end'])
                and case['source'].encode()[span['start']:span['end']].decode() == case['target']
                and definition[columns.index('path')] == case['path'], 'declaration coordinates changed')
    handle = definitions[-1][columns.index('handle')]
    expected_commands = {'find': ['project', 'find', case['target']]}
    if case['callee']:
        expected_commands['calls'] = ['project', 'calls', '.', '--limit', '64']
    if case['feature'] in ('python-repeated', 'python-shadow', 'python-normalized'):
        expected_commands['origins'] = ['project', 'semantic', handle, '--body', '--origins', '--origin-limit', '64']
    else:
        expected_commands['flow'] = ['project', 'dataflow', handle, '--rules', '.fr/rules.json', '--steps', '1024', '--bytes', '65536']
    require(set(events) == set(expected_commands), 'missing or extra profile commands')
    for name, args in expected_commands.items():
        require(events[name]['argv'][1:3] == ['--json', '-C'] and events[name]['argv'][4:] == args,
                'profile command identity changed')
    oracle = row['oracle']
    if case['language'] == 'python':
        require(oracle == python_oracle(case) and oracle['runtime'] == case['expected_runtime'], 'Python AST or runtime disagreement')
        spans = [entry['name'] for entry in oracle['calls']]
    else:
        expected_harness = case['source'] + '\nfn main() { println!("{}", ' + case['target'] + '()); }\n'
        require(oracle['harness'] == expected_harness and oracle['build']['exit_code'] == 0
                and oracle['execute']['exit_code'] == 0 and oracle['execute']['stdout'].strip() == str(case['expected_runtime'])
                and oracle['calls'] == rust_call_spans(case), 'Rust compiler/runtime disagreement')
        spans = oracle['calls']
        require(events['flow']['exit_code'] != 0 and 'Python scalar subset only' in
                (events['flow']['stdout'] + events['flow']['stderr']), 'Rust flow boundary was lost')
    if case['callee']:
        calls = report(events['calls'])
        selected = [item for item in calls['items'] if item['kind'] == 'call' and
                    item.get('name', (item.get('callee') or {}).get('name')) == case['callee']]
        require(calls['page']['remaining'] == 0 and len(selected) == len(spans) > 0
                and len({item['site']['origins']['occurrence']['id'] for item in selected}) == len(spans)
                and calls['provenance']['claim'] == 'candidate-relationships', 'call coverage or claim changed')
        actual = sorted(selected, key=lambda item: item['site']['offset'])
        for item, (start, end) in zip(actual, spans):
            occurrence = item['site']['origins']['occurrence']
            require(occurrence['location'] == location(case['source'], start, end)
                    and occurrence['path'] == case['path'] and occurrence['revision'] == calls['revision'], 'call coordinates changed')
            if case['feature'] == 'rust-shadow':
                require(item['callee'] is None and item['status'] == 'unresolved', 'shadowed Rust call invented a callee')
            else:
                require(item['callee']['name'] == case['callee'], 'named call resolution changed')
    if 'origins' in events:
        origins = report(events['origins'])['origins']
        require(origins['complete'] is True and origins['mutation_authority'] is False and origins['items'], 'origin coverage changed')
        if case['feature'] == 'python-repeated':
            calls = [item for item in origins['items'] if item['kind'] == 'call']
            require(len(calls) == len(oracle['calls']), 'repeated expression origins missing')
            for call, expected in zip(calls, oracle['calls']):
                occurrence = call['origins']['occurrence']
                require(call['origins']['status'] == 'exact' and occurrence['path'] == case['path']
                        and occurrence['location'] == location(case['source'], *expected['expression']), 'expression coordinates changed')
        else:
            require(all(item['origins']['status'] == 'absent' for item in origins['items']), 'unsupported correspondence became exact')
    if case['language'] == 'python' and 'flow' in events:
        flow = report(events['flow'])
        supported = case['feature'] in ('python-positive', 'python-overwrite')
        require(flow['complete'] is supported and len(flow['witnesses']) == (1 if case['feature'] == 'python-positive' else 0),
                'flow answer or unsupported boundary changed')
        require(not flow['cutoffs'] if supported else bool(flow['cutoffs']), 'flow omissions changed')
        if case['feature'] == 'python-local':
            require('ambiguous-call:' + case['rules']['sources'][0] in json.dumps(flow['cutoffs']), 'local shadowing cutoff missing')


def audit_compiler(value):
    require(value['source_bindings'] == {p: file_digest(ROOT / p) for p in compiler.BINDINGS}, 'compiler inputs changed')
    require(value['evidence_digest'] == compiler.digest([value['cases'], value['clipped']['report'], value['cargo']['report'], value['plan']]),
            'compiler evidence digest changed')
    source = (compiler.FIXTURE / 'subject.rs').read_bytes()
    start = source.index(b'{ value }') + 2
    require(value['oracle'] == {'default_accepted': True, 'strict_rejected': True, 'code': 'E0308',
                              'primary_span': [start, start + 5], 'runtime_cases': 6}, 'compiler oracle changed')
    for name in ('default', 'strict'):
        case = value['cases'][name]
        checks = compiler.FrReport(case['checks'], ())
        require(checks.at('/passed') is (name == 'default'), 'compiler check outcome changed')
        items = []
        for page in case['pages']:
            parsed = compiler.CompilerEvidence.from_report(compiler.FrReport(page, ()), checks=checks)
            require(page['capture']['complete'] is True and page['page']['before'] == len(items), 'compiler capture incomplete')
            items.extend(parsed.items)
        require(case['pages'][-1]['page']['remaining'] == 0, 'compiler pages missing')
        if name == 'strict':
            exact = [span for item in items if item.code == 'E0308' for span in item.spans if span.primary]
            require(any(span.status == 'exact' and span.syntax == 'accepted' and
                    [span.occurrence.location.span.start, span.occurrence.location.span.end] == [start, start + 5]
                    for span in exact), 'syntax/compiler disagreement lost')
        else:
            require(not items, 'default compilation acquired diagnostics')
    require(set(value['refusals']) == {'external_identity', 'environment', 'new_build_configuration', 'cargo_environment', 'source'}
            and all(value['refusals'].values()), 'compiler stale-input refusals lost')
    require(value['clipped']['report']['complete'] is False and value['cargo']['report']['complete'] is True
            and value['cargo']['report']['outcome']['passed'] is False, 'compiler truncation or Cargo outcome changed')


def audit(value):
    require(value['schema'] == 'fr-compiler-profile-1' and value['source_bindings'] == bindings(), 'profile inputs changed')
    require(value['profile'] == PROFILE, 'declared feature table changed')
    require(value['python_version'] and value['rustc_version'] and value['cargo_version'], 'compiler/runtime versions missing')
    require(re.fullmatch('[0-9a-f]{40}', value['source_commit']) and re.fullmatch('[0-9a-f]{64}', value['binary_sha256']), 'collection identity missing')
    require(set(value['toolchains']) == {'rustc', 'cargo', 'python'} and all(
        item['path'] and re.fullmatch('[0-9a-f]{64}', item['sha256']) for item in value['toolchains'].values()),
        'toolchain executable identities missing')
    expected = {case['id']: case for case in cases()}
    rows = {row['case']['id']: row for row in value['cases']}
    require(set(rows) == set(expected) and len(rows) == len(value['cases']), 'missing or duplicate profile case')
    for key, case in expected.items():
        audit_case(rows[key], case)
        require(all(event['argv'][0] == value['fr_executable'] for event in rows[key]['events'].values()), 'fr executable changed')
        if case['language'] == 'rust':
            build = rows[key]['oracle']['build']['argv']
            executed = rows[key]['oracle']['execute']['argv']
            root = Path(rows[key]['events']['find']['argv'][3])
            require(build == [value['toolchains']['rustc']['path'], '--edition=2021', '--crate-name', 'profile',
                    str(root.parent / (root.name + '-driver.rs')), '-o', str(root.parent / (root.name + '-driver'))]
                    and executed == [build[-1]], 'Rust build inputs changed')
    for nested in (value['compiler'], value['semantic']):
        require(nested['binary_sha256'] == value['binary_sha256'] and nested['repository_revision'] == value['source_commit'],
                'nested report uses a different binary or commit')
    audit_compiler(value['compiler'])
    semantic.audit(value['semantic'])
    require(value['rustc_version'] == value['compiler']['compiler_version'], 'compiler version disagrees across collectors')
    require(value['python_version'].split()[0] == value['semantic']['python'], 'Python version disagrees across collectors')
    expected_summary = {'features': len(PROFILE), 'cases': len(expected), 'renamed_relocated_cases': len(expected) // 2,
                        'false_claims': 0, 'syntax_compiler_disagreements': 1, 'profile_complete': True}
    require(value['summary'] == expected_summary, 'profile conclusion changed')
    return expected_summary


def save(value, output):
    if output:
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_text(json.dumps(value, ensure_ascii=False, indent=2) + '\n')


def measure(binary, output=None):
    require(os.environ.get('GITHUB_ACTIONS') == 'true', 'collect only on GitHub runners')
    binary = binary.resolve()
    rustc = subprocess.check_output(['rustup', 'which', 'rustc'], text=True).strip()
    cargo = subprocess.check_output(['rustup', 'which', 'cargo'], text=True).strip()
    value = {'schema': 'fr-compiler-profile-1', 'source_bindings': bindings(), 'profile': PROFILE,
             'source_commit': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
             'binary_sha256': file_digest(binary), 'fr_executable': str(binary), 'python_version': sys.version,
             'toolchains': {name: {'path': path, 'sha256': file_digest(path)} for name, path in
                            [('rustc', rustc), ('cargo', cargo), ('python', sys.executable)]},
             'rustc_version': subprocess.check_output([rustc, '-vV'], text=True),
             'cargo_version': subprocess.check_output(['cargo', '-Vv'], text=True), 'platform': platform.platform(),
             'cases': [], 'summary': {'profile_complete': False},
             'limits': 'Finite declared features and prescribed programs, not general language correctness. '
             'Call edges are indexed candidates, not runtime dispatch proofs. Runtime results cover listed inputs only. '
             'Compiler output is caller-retained evidence, not execution attestation. No live agent, token or cost comparison.'}
    with tempfile.TemporaryDirectory(prefix='fr-compiler-profile-') as temporary:
        for index, case in enumerate(cases()):
            root = Path(temporary) / ('case' + str(index))
            root.mkdir()
            row = collect_case(case, binary, rustc, root)
            value['cases'].append(row)
            save(value, output)
            print(case['id'] + ': captured', file=sys.stderr, flush=True)
    value['compiler'] = compiler.measure(SimpleNamespace(fr=binary))
    save(value, output)
    compiler.audit(value['compiler'])
    value['semantic'] = semantic.measure(binary)
    save(value, output)
    semantic.audit(value['semantic'])
    value['summary'] = {'features': len(PROFILE), 'cases': len(value['cases']), 'renamed_relocated_cases': len(value['cases']) // 2,
                        'false_claims': 0, 'syntax_compiler_disagreements': 1, 'profile_complete': True}
    return value


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--fr', type=Path, default=ROOT / 'target/debug/fr')
    parser.add_argument('--output', type=Path)
    parser.add_argument('--audit', type=Path)
    args = parser.parse_args()
    value = json.loads(args.audit.read_text()) if args.audit else measure(args.fr, args.output)
    try:
        checked = audit(value)
    except Exception as error:
        value['summary'] = {'profile_complete': False, 'error': str(error)}
        save(value, args.output)
        raise
    save(value, args.output)
    print(json.dumps(checked, sort_keys=True))


if __name__ == '__main__':
    main()
