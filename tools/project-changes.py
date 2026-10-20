#!/usr/bin/env python3
"""Inspect fr through its public client before checking original project changes."""
import argparse
import json
from pathlib import Path
import sys
import traceback

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'sdk/python/src'))
from fr_ir.runtime import FrClient
from project_change_sources import download, save, digest
from project_change_delivery import measure as deliver
from project_change_findings import measure as findings, reopen


def inspect(args):
    args.output.mkdir(parents=True, exist_ok=False)
    selections = [
        ('project', 'dependency_digest', 'investigation.rs'),
        ('project', 'plan', 'structural_change.rs'),
        ('native', 'fingerprint', 'native_fingerprint.rs'),
    ]
    for directory, name, path in selections:
        root = ROOT / 'src/project' if directory == 'project' else ROOT / 'src'
        client = FrClient(root, executable=args.fr.resolve(), timeout=20, max_output_bytes=1048576)
        report = client.project('find', name, '--in', path, '--source', '--bytes', '65536').to_data()
        if report['page']['next'] is not None or not report['rows']:
            raise ValueError('fr inspection is unfinished or missing: ' + path)
        save(args.output / (name + '.json'), report)
    save(args.output / 'scope.json', {
        'scope': 'fr inspected its original src/project and src directories. These bounded reads do not establish repository-wide caller discovery.',
        'selections': selections,
    })


def collect(args):
    tasks_path = ROOT / 'tests/agent-eval/project-changes/tasks.json'
    tasks = json.loads(tasks_path.read_text())
    args.output.mkdir(parents=True, exist_ok=False)
    save(args.output / 'tasks.json', tasks)
    selected = [task for task in tasks['projects'] if task['role'] == 'development']
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
        'unread_project_opened': False, 'binary_sha256': digest(args.fr),
        'limits': tasks['limits'], 'claim': tasks['scope']})


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('phase', choices=('inspect', 'delivery', 'findings', 'reopen'))
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
        if args.phase == 'findings' and args.alternate is None:
            parser.error('findings requires the recorded alternate checker')
        collect(args)


if __name__ == '__main__':
    main()
