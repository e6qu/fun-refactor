#!/usr/bin/env python3
"""Retain authenticated language results and connect their finite roadmap claims."""
import argparse
import importlib.util
import json
import os
from pathlib import Path, PurePosixPath
import re

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('scalar_retention', ROOT / 'tools/retain-scalar-evidence.py')
scalar = importlib.util.module_from_spec(spec)
spec.loader.exec_module(scalar)
require = scalar.require


def selected_files(data, group):
    require(group in scalar.LANGUAGE, 'unknown language result group')
    prefix = 'language-behavior/' + scalar.LANGUAGE[group] + '/'
    report = json.loads(data[prefix + 'result.json'])
    rows = report['translations'] if group == 'checked-translation' else report['cases']
    names = ['result.json', *(name for row in rows for name in row['files'])]
    require(len(names) == len(set(names)), 'language result repeats a retained file')
    for name in names:
        path = PurePosixPath(name)
        require(not path.is_absolute() and str(path) == name and '..' not in path.parts,
                'language result contains an unsafe file path')
    return {name: data[prefix + name] for name in names}


def register(catalog, group, relative, receipt):
    scope = ('Both Rust/Python directions, each repeated after renaming and moving; 9,030 exhaustive input pairs '
             'and eleven signed-64-bit boundary pairs per translation. A finite behavior check, not a general proof.'
             if group == 'checked-translation' else
             'Nine Python effect examples repeated after renaming and moving, with two runtime inputs each. '
             'Exact source locations and visible model/runtime disagreement; handlers and shared objects remain unsupported.')
    entry = {'id': group, 'kind': 'deterministic', 'path': str(relative / 'result.json'),
             'sha256': receipt['files']['result.json'], 'scope': scope,
             'audit': 'python3 ' + ' '.join(scalar.audit_arguments(group, relative / 'result.json'))}
    prior = [row for row in catalog['evidence'] if row['id'] == group]
    require(len(prior) <= 1, 'duplicate language evidence ID')
    if prior:
        prior[0].clear()
        prior[0].update(entry)
    else:
        catalog['evidence'].append(entry)
    obligation = 'D.translation' if group == 'checked-translation' else 'B.effects'
    selected, = [row for row in catalog['obligations'] if row['id'] == obligation]
    checks = [('/passed', True)]
    checks += ([('/coverage/exhaustive_pairs', 9030), ('/coverage/boundary_pairs', 11),
                ('/coverage/directions', 2), ('/coverage/renamed_and_moved', True)]
               if group == 'checked-translation' else
               [('/coverage/examples', 9), ('/coverage/renamed_and_moved_cases', 18),
                ('/coverage/runtime_executions', 36), ('/coverage/preserves_model_runtime_difference', True)])
    selected['requires'] = [{'evidence': group, 'assertions': [
        {'pointer': pointer, 'equals': value} for pointer, value in checks]}]
    selected['next'] = ('Keep exact integer division, operand evaluation order and the independent bounded checks passing. '
                        'Other functions and input domains need their own contracts and checks.'
                        if group == 'checked-translation' else
                        'Keep unchecked runtime effects visible at exact source locations. Supporting handlers and shared '
                        'mutable objects still needs language rules and independent project examples.')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run', required=True, type=int)
    parser.add_argument('--prefix', required=True)
    args = parser.parse_args()
    require(os.environ.get('GITHUB_ACTIONS') == 'true' and args.run > 0, 'retain a hosted language collection')
    require(os.environ['GITHUB_REF'].startswith('refs/heads/') and os.environ['GITHUB_REF'] != 'refs/heads/main',
            'retain into the working branch')
    require(re.fullmatch(r'[a-zA-Z0-9][a-zA-Z0-9_-]{0,79}', args.prefix), 'supply a plain dated prefix')
    data, identity = scalar.collect(args.run, 'language-behavior', 'language-behavior', os.environ['GITHUB_REPOSITORY'])
    scalar.retain.command(['git', 'fetch', '--no-tags', '--depth=1', 'origin', identity['source_commit']], capture_output=True)
    scalar.verify_sources(ROOT, [identity['source_commit']])
    path = ROOT / 'tests/agent-eval/roadmap.json'
    catalog = json.loads(path.read_text())
    for group in scalar.LANGUAGE:
        relative, receipt = scalar.keep(group, selected_files(data, group), identity, args.prefix)
        (ROOT / relative / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
        register(catalog, group, relative, receipt)
    path.write_text(json.dumps(catalog, indent=2, ensure_ascii=False) + '\n')
    scalar.retain.command(['python3', '-B', 'tools/roadmap-status.py', '--write'])
    scalar.retain.command(['python3', '-B', 'tools/roadmap-status.py', '--check'])


if __name__ == '__main__':
    main()
