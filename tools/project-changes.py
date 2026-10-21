#!/usr/bin/env python3
"""Inspect fr through its public client before checking original project changes."""
import argparse
import json
from pathlib import Path
import sys
import traceback
import subprocess

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'sdk/python/src'))
from fr_ir.runtime import FrClient
from project_change_sources import download, save, digest, bindings
from project_change_delivery import measure as deliver
from project_change_findings import measure as findings, reopen


def inspect(args):
    args.output.mkdir(parents=True, exist_ok=False)
    selections = [
        ('project', 'dependency_digest', 'investigation.rs'),
        ('project', 'plan', 'structural_change.rs'),
        ('native', 'fingerprint', 'native_fingerprint.rs'),
        ('project', 'change_scope', 'change_scope.rs'),
        ('project', 'consumer', 'change_scope.rs'),
        ('index', 'resolve_one', 'index.rs'),
        ('index', 'resolve_by_evidence', 'index.rs'),
        ('refactor', 'plan', 'refactor/rename.rs'),
        ('native', 'execute', 'history/files.rs'),
        ('native', 'cmd_usages', 'cli.rs'),
    ]
    for directory, name, path in selections:
        directory_root = ROOT / 'src/project' if directory == 'project' else ROOT / 'src'
        root = directory_root / path
        client = FrClient(root, executable=args.fr.resolve(), timeout=20, max_output_bytes=1048576)
        report = client.project('find', name, '--source', '--bytes', '65536').to_data()
        if report['page']['next'] is not None or not report['rows']:
            raise ValueError('fr inspection is unfinished or missing: ' + path)
        save(args.output / (Path(path).stem + '-' + name + '.json'), report)
    changes = subprocess.check_output(['git', 'status', '--porcelain', '--untracked-files=all'],
        cwd=ROOT, text=True).splitlines()
    save(args.output / 'scope.json', {
        'passed': not changes,
        'source_commit': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
        'source_bindings': bindings(ROOT),
        'source_clean': not changes,
        'changed_inputs': changes,
        'scope': 'fr inspected the explicitly selected original source files. File-scoped reads do not establish repository-wide caller discovery.',
        'selections': selections,
    })
    if changes:
        raise ValueError('Commit the runner corrections before qualification: ' + '; '.join(changes))


def collect(args):
    tasks_path = ROOT / 'tests/agent-eval/project-changes/tasks.json'
    tasks = json.loads(tasks_path.read_text())
    args.output.mkdir(parents=True, exist_ok=False)
    save(args.output / 'tasks.json', tasks)
    selected = [task for task in tasks['projects'] if task['role'] == ('unread' if args.phase == 'held-out' else 'development')]
    frozen = None
    if args.phase == 'held-out':
        from project_change_gate import admit
        frozen = admit(args.fr, args.alternate)
        save(args.output / 'freeze.json', frozen)
    results = []
    for task in selected:
        output = args.output / task['id']
        try:
            original = download(task, output / 'source', tasks['limits'])
            result = {'project': task['id']}
            if args.phase == 'delivery':
                result['delivery'] = deliver(task, original, output / 'delivery', args.fr.resolve())
            else:
                result['findings'] = findings(task, original, output / 'findings', args.fr.resolve(), args.alternate.resolve())
            results.append(result)
        except Exception:
            save(output / 'failure.json', {'passed': False, 'traceback': traceback.format_exc()})
            raise
    save(args.output / 'result.json', {'passed': True, 'phase': args.phase, 'projects': results,
        'source_commit': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
        'source_bindings': bindings(ROOT),
        'unread_project_opened': args.phase == 'held-out', 'implementation': frozen, 'binary_sha256': digest(args.fr),
        'limits': tasks['limits'], 'claim': tasks['scope']})


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('phase', choices=('inspect', 'delivery', 'findings', 'held-out', 'reopen'))
    parser.add_argument('--fr', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--alternate', type=Path)
    parser.add_argument('--project', type=Path)
    parser.add_argument('--objects', type=Path)
    parser.add_argument('--request', type=Path)
    parser.add_argument('--plan')
    parser.add_argument('--change-settings', action='store_true')
    args = parser.parse_args()
    if args.phase == 'inspect':
        inspect(args)
    elif args.phase == 'reopen':
        reopen(args)
    else:
        if args.phase in ('findings', 'held-out') and args.alternate is None:
            parser.error('findings requires the recorded alternate checker')
        collect(args)


if __name__ == '__main__':
    main()
