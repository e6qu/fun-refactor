#!/usr/bin/env python3
"""List stale current reports before starting expensive behavior gates.

This checks recorded input dependencies, not behavior or archived artifacts.
The existing collectors, retention audits and historical replay remain authoritative.
"""
import argparse
import ast
import hashlib
import importlib.util
import json
from pathlib import Path
import re
import subprocess
import sys

from evidence_basis import native_source_paths

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('roadmap_status', ROOT / 'tools/roadmap-status.py')
roadmap = importlib.util.module_from_spec(spec)
spec.loader.exec_module(roadmap)

# Stable catalog IDs, not dated paths. Historical comparisons deliberately stay out.
CURRENT_CATALOG = (
    'host-recovery', 'structural', 'model-relations', 'python-contract',
    'original-package-questions', 'current-project-changes', 'current-saved-work-cost',
    'checked-translation', 'python-effects', 'python-text-flow',
)
PYTHON_SELECTORS = (
    'compiler-profile', 'python-contract', 'workflow-route-context',
    'guide-body-context', 'recovery-delivery-context', 'source-repair-context',
)
# The existing retention format permits a 64 MiB expanded collection.
LIMIT = 64 * 1024 * 1024
TOTAL_LIMIT = 128 * 1024 * 1024


class Reader:
    """Read source normally; read omitted report blobs from a sparse Git checkout."""
    def __init__(self, root):
        self.root = Path(root)
        self.used = 0
        self.cache = {}

    def read(self, name):
        path = roadmap.safe(self.root, name)
        if name in self.cache:
            return self.cache[name]
        if path.is_file():
            size = path.stat().st_size
            load = path.read_bytes
        elif name.startswith('tests/agent-eval/results/'):
            object_name = 'HEAD:' + name
            size = int(subprocess.check_output(['git', 'cat-file', '-s', object_name],
                       cwd=self.root, stderr=subprocess.DEVNULL, timeout=10))
            load = lambda: subprocess.check_output(['git', 'show', object_name],
                          cwd=self.root, stderr=subprocess.DEVNULL, timeout=10)
        else:
            raise ValueError('missing input: ' + name)
        if size > LIMIT or self.used + size > TOTAL_LIMIT:
            raise ValueError('report read budget exceeded: ' + name)
        data = load()
        self.used += len(data)
        self.cache[name] = data
        return data

    def text(self, name):
        return self.read(name).decode()

    def json(self, name):
        return json.loads(self.read(name))


def one(matches, label):
    if len(matches) != 1:
        raise ValueError('need one current report selector: ' + label)
    return matches[0]


def selections(reader):
    selected = {}

    def add(name, path, sha256=None):
        roadmap.safe(reader.root, path)
        row = selected.setdefault(path, {'path': path, 'selected_by': [], 'sha256': sha256})
        if sha256 and row['sha256'] and row['sha256'] != sha256:
            raise ValueError('report selectors disagree: ' + path)
        row['sha256'] = sha256 or row['sha256']
        if name not in row['selected_by']:
            row['selected_by'].append(name)

    scalar = reader.json('tests/agent-eval/current-scalar-reports.json')
    if scalar['schema'] != 'fr-current-scalar-reports':
        raise ValueError('unknown scalar report selector')
    catalog = reader.json('tests/agent-eval/roadmap.json')
    if catalog['schema'] != 'fr-roadmap-catalog-1':
        raise ValueError('unknown roadmap catalog')
    for name, row in scalar['reports'].items():
        add(name, row['path'], row['sha256'])
        if row['catalog_id']:
            entry = one([e for e in catalog['evidence'] if e['id'] == row['catalog_id']], name)
            if (entry['path'], entry['sha256']) != (row['path'], row['sha256']):
                raise ValueError('scalar and roadmap selectors disagree: ' + name)
    for name in CURRENT_CATALOG:
        row = one([e for e in catalog['evidence'] if e['id'] == name], name)
        add(name, row['path'], row['sha256'])
    for name in PYTHON_SELECTORS:
        text = reader.text('tools/test-' + name + '.py')
        path = one(re.findall(r"^REPORT = ROOT / '([^']+)'$", text, re.M), name)
        add(name, path)

    # Reuse the retainer's selectors without importing collectors or running checks.
    constants = {}
    for node in ast.parse(reader.text('tools/retain-ci-reports.py')).body:
        if isinstance(node, ast.Assign) and len(node.targets) == 1 and isinstance(node.targets[0], ast.Name):
            if node.targets[0].id in {'ACTIVE_REFERENCES', 'ACTIVE_FUNCTIONS', 'DIRECTORY_REPORTS'}:
                constants[node.targets[0].id] = ast.literal_eval(node.value)
    for name, source in constants['ACTIVE_REFERENCES'].items():
        directory = name in constants['DIRECTORY_REPORTS']
        suffix = '' if directory else '/result.json'
        path = one(re.findall(r'"(tests/agent-eval/results/[^"/]+-' + re.escape(name + suffix) + ')"',
                              reader.text(source)), name)
        add(name, path + '/manifest.json' if directory else path)
    text = reader.text('tests/python_sdk.rs')
    for name, function in constants['ACTIVE_FUNCTIONS'].items():
        body = one(re.findall(r'fn ' + re.escape(function) + r'\(\)\s*\{(.*?)\n\}', text, re.S), name)
        path = one(re.findall(r'"(tests/agent-eval/[^"\n]+\.json)"', body), name)
        add(name, path)
    text = reader.text('tests/agent_acceptance.rs')
    path = one(re.findall(r'"(tests/agent-eval/results/[^"/]+-change-scope)"', text), 'change-scope')
    add('change-scope', path + '/manifest.json')
    return sorted(selected.values(), key=lambda row: row['path'])


def inspect(reader, selected, native, digests):
    row = {key: selected[key] for key in ('path', 'selected_by')}
    row.update(state='error', changed_sources=[])
    try:
        data = reader.read(row['path'])
        if selected.get('sha256') and hashlib.sha256(data).hexdigest() != selected['sha256']:
            raise ValueError('retained report digest changed')
        value = json.loads(data)
        archived_sources = {name.removeprefix('binding-sources/'): expected
                            for name, expected in value.get('files', {}).items()
                            if name.startswith('binding-sources/')}
        # Directory audits put their source bindings in either manifest or result.
        if row['path'].endswith('/manifest.json') and 'result.json' in value.get('files', {}):
            child = str(Path(row['path']).with_name('result.json'))
            child_data = reader.read(child)
            if hashlib.sha256(child_data).hexdigest() != value['files']['result.json']:
                raise ValueError('retained result digest changed')
            nested = json.loads(child_data)
            if not value.get('source_bindings') and not value.get('bindings'):
                value = nested
        bindings = value.get('source_bindings', value.get('bindings', {}))
        if not isinstance(bindings, dict):
            raise ValueError('invalid source bindings')
        changed = set()
        for name, expected in archived_sources.items():
            path = roadmap.safe(reader.root, name)
            if not path.is_file() or hashlib.sha256(path.read_bytes()).hexdigest() != expected:
                changed.add(name)
        for name, expected in bindings.items():
            path = roadmap.safe(reader.root, name)
            if not isinstance(expected, str) or not re.fullmatch('[0-9a-f]{64}', expected):
                raise ValueError('invalid input digest: ' + name)
            # Only Python release metadata uses a schema-specific normalization.
            key = (value.get('schema') if name == 'sdk/python/src/fr_ir/_version.py' else None, name)
            if key not in digests:
                if path.is_file():
                    digests[key] = roadmap.binding_digest(path, value.get('schema'), reader.root)
                elif name.startswith('tests/agent-eval/results/'):
                    # Current reports can depend on a historical receipt omitted by sparse checkout.
                    digests[key] = hashlib.sha256(reader.read(name)).hexdigest()
                else:
                    digests[key] = None
            if digests[key] != expected:
                changed.add(name)
        if 'native-source-inputs.txt' in bindings:
            changed.update(native - bindings.keys())
        if 'sdk/python/pyproject.toml' in bindings:
            client = reader.root / 'sdk/python/src'
            changed.update(path.relative_to(reader.root).as_posix() for path in client.rglob('*.py')
                           if path.relative_to(reader.root).as_posix() not in bindings)
        if value.get('schema') == 'fr-project-changes-recheck':
            for directory in ('sdk/python/src', 'sdk/python/tests'):
                changed.update(path.relative_to(reader.root).as_posix()
                               for path in (reader.root / directory).rglob('*.py')
                               if path.relative_to(reader.root).as_posix() not in bindings)
        row.update(state='stale' if changed else 'current' if bindings or archived_sources else 'not-source-bound',
                   changed_sources=sorted(changed))
    except (ValueError, KeyError, OSError, subprocess.SubprocessError) as error:
        row['error'] = str(error)
    return row


def compute(root):
    reader = Reader(root)
    native = set(native_source_paths(root))
    digests = {}
    selected = selections(reader)
    rows = []
    for row in selected:
        # Bound each report and its dependencies; do not retain earlier report
        # payloads merely to inspect the next one. The digest cache stays small.
        reader.cache.clear()
        reader.used = 0
        rows.append(inspect(reader, row, native, digests))
    return {'schema': 'fr-evidence-preflight-1',
            'passed': all(row['state'] not in {'stale', 'error'} for row in rows),
            'scope': 'Current report input freshness; behavior and archive audits still run separately.',
            'reports': rows}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--json', action='store_true')
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    try:
        report = compute(ROOT)
    except (ValueError, KeyError, OSError, subprocess.SubprocessError) as error:
        print('Evidence preflight: ' + str(error), file=sys.stderr)
        return 1
    encoded = json.dumps(report, indent=2) + '\n'
    if args.output:
        args.output.write_text(encoded)
    if args.json:
        print(encoded, end='')
    else:
        for row in report['reports']:
            print(row['state'] + ': ' + ', '.join(row['selected_by']))
            for name in row['changed_sources']:
                print('  ' + name)
            if row.get('error'):
                print('  ' + row['error'])
        print(report['scope'])
    return 0 if report['passed'] else 1


if __name__ == '__main__':
    sys.exit(main())
