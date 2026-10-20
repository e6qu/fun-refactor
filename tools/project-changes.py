#!/usr/bin/env python3
"""Inspect fr through its public client before checking original project changes."""
import argparse
import json
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'sdk/python/src'))
from fr_ir.runtime import FrClient


def save(path, value):
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + '\n')


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


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('phase', choices=('inspect',))
    parser.add_argument('--fr', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    inspect(args)


if __name__ == '__main__':
    main()
