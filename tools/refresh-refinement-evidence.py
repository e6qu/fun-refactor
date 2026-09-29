#!/usr/bin/env python3
"""Regenerate selected source-bound evidence groups on a CI runner."""
from __future__ import annotations

import json
import os
import re
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
WORK = Path(os.environ.get('RUNNER_TEMP', ROOT / 'target')) / (
    f"fr-refinement-{os.environ.get('GITHUB_RUN_ID', 'local')}-{os.environ.get('GITHUB_RUN_ATTEMPT', '1')}"
)
OUTPUT = WORK / 'refinement-evidence'
LOGS = WORK / 'refinement-refresh-logs'
PREFIX = os.environ.get('FR_EVIDENCE_PREFIX', '')


def main():
    if os.environ.get('GITHUB_ACTIONS') != 'true':
        raise SystemExit('Run this workload on GitHub Actions; local regeneration is disabled.')
    if not re.fullmatch(r'[a-zA-Z0-9][a-zA-Z0-9_-]{0,79}', PREFIX):
        raise SystemExit('Supply a plain dated FR_EVIDENCE_PREFIX for new retained reports.')
    group = os.environ.get('FR_EVIDENCE_GROUP', 'all')
    scripts = {
        'model-comparisons': 'refinement-acceptance',
        'retained-proofs': 'proof-evidence-acceptance',
        'agent-guide-context': 'agent-guide-context',
        'completion-workflows': 'completion-workflows',
        'intent-action-context': 'intent-action-context',
        'index-resolution': 'index-resolution-acceptance',
        'host-recovery': 'host-recovery-acceptance',
    }
    flow_scripts = {
        'flow-fixed-point': 'flow-acceptance',
        'recursive-flow': 'recursive-flow-acceptance',
        'flow-facts': 'flow-facts-acceptance',
        'imported-flow': 'imported-flow-acceptance',
        'flow-cache': 'flow-cache-acceptance',
        'package-flow': 'package-flow-acceptance',
        'package-reexports': 'package-reexports-acceptance',
        'module-aliases': 'module-aliases-acceptance',
        'expression-control': 'expression-control-acceptance',
        'call-binding': 'call-binding-acceptance',
        'index-resolution': 'index-resolution-acceptance',
    }
    dependency_scripts = {
        'compiler-evidence': 'compiler-evidence-acceptance',
        'resumable-correspondence': 'correspondence-acceptance',
        'semantic-evidence': 'semantic-evidence-acceptance',
        'index-consumers': 'index-consumers-acceptance',
        'retained-proofs': 'proof-evidence-acceptance',
        'host-recovery': 'host-recovery-acceptance',
    }
    refinement_names = list(scripts)
    scripts.update(flow_scripts)
    scripts.update(dependency_scripts)
    if group not in ('all', 'flow', 'dependency') and group not in scripts:
        raise SystemExit(f'Unknown evidence group: {group}')
    groups = {'all': refinement_names, 'flow': list(flow_scripts), 'dependency': list(dependency_scripts)}
    names = groups.get(group, [group])
    LOGS.mkdir(parents=True, exist_ok=True)
    fr = str(ROOT / 'target/debug/fr')
    if any(name not in ('host-recovery', 'index-resolution', 'index-consumers') for name in names):
        subprocess.run(['cargo', 'build', '--locked'], cwd=ROOT, check=True)
    binaries = {}
    for name, target in [('index-resolution', 'index_resolution'), ('index-consumers', 'index_consumers')]:
        if name not in names:
            continue
        manifest = LOGS / f'{name}-binaries.jsonl'
        with manifest.open('w') as log:
            subprocess.run(['cargo', 'test', '--locked', '--test', target, '--no-run',
                            '--message-format=json'], cwd=ROOT, stdout=log, check=True)
        records = [json.loads(line) for line in manifest.read_text().splitlines()]
        binaries[name] = next(row['executable'] for row in records if row.get('reason') == 'compiler-artifact'
                              and row['target']['name'] == target and row.get('executable'))
    for name in names:
        directory = OUTPUT / f'{PREFIX}-{name}'
        path = directory if name == 'model-comparisons' else directory / 'result.json'
        path.parent.mkdir(parents=True, exist_ok=True)
        command = [sys.executable, f'tools/{scripts[name]}.py']
        if name in binaries:
            command += ['--binary', binaries[name]]
        elif name != 'host-recovery':
            command += ['--fr', fr]
        if name != 'intent-action-context':
            command += ['--output', str(path)]
        print(f'Refreshing {name}', flush=True)
        with (LOGS / f'{name}.log').open('w') as log:
            if name == 'intent-action-context':
                with path.open('w') as output:
                    subprocess.run(command, cwd=ROOT, stdout=output, stderr=log,
                                   check=True, timeout=1800)
            else:
                subprocess.run(command, cwd=ROOT, stdout=log, stderr=subprocess.STDOUT,
                               check=True, timeout=1800)
        print(f'Passed {name}', flush=True)


if __name__ == '__main__':
    main()
