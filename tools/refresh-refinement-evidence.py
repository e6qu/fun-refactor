#!/usr/bin/env python3
"""Regenerate the model comparison and six affected source-bound reports on a CI runner."""
from __future__ import annotations

import json
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / 'target/refinement-evidence'
LOGS = ROOT / 'target/refinement-refresh-logs'


def main():
    if os.environ.get('GITHUB_ACTIONS') != 'true':
        raise SystemExit('Run this workload on GitHub Actions; local regeneration is disabled.')
    LOGS.mkdir(parents=True, exist_ok=True)
    subprocess.run(['cargo', 'build', '--locked'], cwd=ROOT, check=True)
    with (LOGS / 'binaries.jsonl').open('w') as log:
        subprocess.run(['cargo', 'test', '--test', 'index_resolution', '--no-run',
                        '--message-format=json'], cwd=ROOT, stdout=log, check=True)
    records = [json.loads(line) for line in (LOGS / 'binaries.jsonl').read_text().splitlines()]
    binary = next(row['executable'] for row in records if row.get('reason') == 'compiler-artifact'
                  and row['target']['name'] == 'index_resolution' and row.get('executable'))
    fr = str(ROOT / 'target/debug/fr')
    commands = [('model-comparisons', ['tools/refinement-acceptance.py', '--fr', fr, '--output',
                                      str(OUTPUT / '2026-09-28-model-comparisons')])]
    for name, script in [('retained-proofs', 'proof-evidence-acceptance'),
                         ('agent-guide-context', 'agent-guide-context'),
                         ('completion-workflows', 'completion-workflows'),
                         ('intent-action-context', 'intent-action-context'),
                         ('index-resolution', 'index-resolution-acceptance'),
                         ('host-recovery', 'host-recovery-acceptance')]:
        path = OUTPUT / f'2026-09-28-refinement-{name}' / 'result.json'
        path.parent.mkdir(parents=True, exist_ok=True)
        command = [f'tools/{script}.py']
        if name == 'index-resolution':
            command += ['--binary', binary]
        elif name != 'host-recovery':
            command += ['--fr', fr]
        if name != 'intent-action-context':
            command += ['--output', str(path)]
        commands.append((name, command))
    for name, command in commands:
        print(f'Refreshing {name}', flush=True)
        with (LOGS / f'{name}.log').open('w') as log:
            if name == 'intent-action-context':
                path = OUTPUT / f'2026-09-28-refinement-{name}' / 'result.json'
                with path.open('w') as output:
                    subprocess.run([sys.executable, *command], cwd=ROOT, stdout=output,
                                   stderr=log, check=True, timeout=1800)
            else:
                subprocess.run([sys.executable, *command], cwd=ROOT, stdout=log,
                               stderr=subprocess.STDOUT, check=True, timeout=1800)
        print(f'Passed {name}', flush=True)


if __name__ == '__main__':
    main()
