#!/usr/bin/env python3
"""Save fresh hosted reports after checking their source and behavioral evidence."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import subprocess

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('scalar', ROOT / 'tools/retain-scalar-evidence.py')
scalar = importlib.util.module_from_spec(spec)
spec.loader.exec_module(scalar)
require = scalar.require
GROUPS = {
    'contexts': ['project-batch-context', 'task-bundle-context', 'task-change-context', 'workflow-context'],
    'host-recovery': ['host-recovery'],
    'retained-proofs': ['retained-proofs'],
    'resumable-correspondence': ['resumable-correspondence'],
    'index-consumers': ['index-consumers'],
}
SCRIPTS = {
    'host-recovery': 'host-recovery-acceptance',
    'retained-proofs': 'proof-evidence-acceptance',
    'resumable-correspondence': 'correspondence-acceptance',
    'index-consumers': 'index-consumers-acceptance',
}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--runs', required=True)
    parser.add_argument('--prefix', required=True)
    args = parser.parse_args()
    require(os.environ.get('GITHUB_ACTIONS') == 'true', 'retain reports on GitHub runners')
    branch = os.environ.get('GITHUB_REF', '')
    require(branch.startswith('refs/heads/') and branch != 'refs/heads/main', 'use a working branch')
    require(re.fullmatch(r'[a-zA-Z0-9][a-zA-Z0-9_-]{0,79}', args.prefix), 'need a plain dated prefix')
    runs = json.loads(args.runs)
    require(type(runs) is dict and bool(runs) and set(runs) <= set(GROUPS)
            and all(type(run) is int and run > 0 for run in runs.values()),
            'declare the requested hosted collections explicitly')
    collected = []
    for collection in sorted(runs):
        groups = GROUPS[collection]
        data, identity = scalar.collect(runs[collection], 'refinement-evidence', 'refresh',
                                        os.environ['GITHUB_REPOSITORY'])
        paths = scalar.report_paths(data, groups)
        for group, path in paths.items():
            collected.append((group, data[path], identity))
    revisions = {identity['source_commit'] for _, _, identity in collected}
    require(len(revisions) == 1, 'collect the repair reports from the same source commit')
    for revision in revisions:
        scalar.retain.command(['git', 'fetch', '--no-tags', '--depth=1', 'origin', revision],
                              capture_output=True)
    scalar.verify_sources(ROOT, revisions)
    receipts = {}
    for group, data, identity in collected:
        directory = ROOT / 'tests/agent-eval/results' / (args.prefix + '-' + group)
        require(not directory.exists(), 'preserve previous reports; choose a new directory')
        directory.mkdir()
        report = directory / 'result.json'
        report.write_bytes(data)
        script = SCRIPTS.get(group, group)
        flag = '--verify' if group == 'index-consumers' else '--audit'
        subprocess.run(['python3', '-B', f'tools/{script}.py', flag, str(report)],
                       cwd=ROOT, check=True, timeout=60, stdout=subprocess.DEVNULL)
        receipts[group] = {'collection': identity, 'sha256': hashlib.sha256(data).hexdigest()}
        print(f'Validated {group}', flush=True)
    catalog_path = ROOT / 'tests/agent-eval/roadmap.json'
    catalog = json.loads(catalog_path.read_text())
    if 'host-recovery' in receipts:
        entry, = [row for row in catalog['evidence'] if row['id'] == 'host-recovery']
        entry['path'] = f'tests/agent-eval/results/{args.prefix}-host-recovery/result.json'
        entry['audit'] = 'python3 tools/host-recovery-acceptance.py --audit ' + entry['path']
        entry['sha256'] = receipts['host-recovery']['sha256']
        catalog_path.write_text(json.dumps(catalog, indent=2, ensure_ascii=False) + '\n')
    directory = ROOT / 'tests/agent-eval/results' / (args.prefix + '-ci-collection')
    require(not directory.exists(), 'preserve previous collection receipts')
    directory.mkdir()
    (directory / 'manifest.json').write_text(json.dumps(
        {'schema': 'fr-ci-report-collection', 'groups': receipts}, indent=2) + '\n')
    if 'retained-proofs' in receipts:
        proof_test = ROOT / 'tests/retained_proofs.rs'
        text, count = re.subn(
            r'"tests/agent-eval/results/[^"/]+-retained-proofs/result\.json"',
            f'"tests/agent-eval/results/{args.prefix}-retained-proofs/result.json"',
            proof_test.read_text())
        require(count == 1, 'expected one active retained-proof replay reference')
        proof_test.write_text(text)
    subprocess.run(['python3', '-B', 'tools/roadmap-status.py', '--write'], cwd=ROOT, check=True, timeout=60)


if __name__ == '__main__':
    main()
