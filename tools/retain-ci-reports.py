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
    'completion-workflows': ['completion-workflows'],
    'structural-change': ['structural-change'],
    'sdk-contexts': ['agent-context-workspace', 'agent-guide-context', 'agent-runtime-context',
                     'native-intent-context', 'intent-action-context'],
}
SCRIPTS = {
    'host-recovery': 'host-recovery-acceptance',
    'retained-proofs': 'proof-evidence-acceptance',
    'resumable-correspondence': 'correspondence-acceptance',
    'index-consumers': 'index-consumers-acceptance',
    'structural-change': 'structural-change-acceptance',
}
ACTIVE_REFERENCES = {
    'project-batch-context': 'tests/agent_acceptance.rs',
    'task-bundle-context': 'tests/agent_acceptance.rs',
    'task-change-context': 'tools/agent_eval/test_harness.py',
    'workflow-context': 'tests/agent_acceptance.rs',
    'retained-proofs': 'tests/retained_proofs.rs',
    'resumable-correspondence': 'tests/correspondence.rs',
    'index-consumers': 'tests/consumer_measurements.rs',
    'completion-workflows': 'tests/completion_workflows.rs',
    'structural-change': 'tests/structural_change.rs',
}
ACTIVE_FUNCTIONS = {
    'agent-context-workspace': 'checked_agent_context_workspace_comparison_is_reproducible',
    'agent-guide-context': 'checked_agent_guide_context_comparison_is_reproducible',
    'agent-runtime-context': 'checked_agent_runtime_context_comparison_is_reproducible',
    'native-intent-context': 'native_intent_context_evidence_is_source_bound_and_arithmetically_valid',
    'intent-action-context': 'intent_action_context_evidence_is_source_bound_and_arithmetically_valid',
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
        if collection == 'structural-change':
            matches = [name for name in data if re.fullmatch(
                r'refinement-evidence/[a-zA-Z0-9][a-zA-Z0-9_-]{0,79}-structural-change/result.json', name)]
            require(len(matches) == 1, 'need one structural collection')
            prefix = str(Path(matches[0]).parent) + '/'
            require(all(name.startswith(prefix) or name.startswith('refinement-refresh-logs/') for name in data),
                    'structural archive contains unrelated files')
            paths = {'structural-change': matches[0]}
        else:
            paths = scalar.report_paths(data, groups)
        for group, path in paths.items():
            payload = {'result.json': data[path]}
            if group == 'structural-change':
                prefix = str(Path(path).parent) + '/'
                payload = {name[len(prefix):]: content for name, content in data.items() if name.startswith(prefix)}
                manifest = json.loads(payload['manifest.json'])
                require(manifest['source_commit'] == identity['source_commit'], 'structural collection source changed')
                require(set(payload) == {'manifest.json', *manifest['files']}, 'structural archive inventory changed')
                for name, expected in manifest['files'].items():
                    require(hashlib.sha256(payload[name]).hexdigest() == expected, 'structural artifact changed: ' + name)
            collected.append((group, payload, identity))
    revisions = {identity['source_commit'] for _, _, identity in collected}
    require(len(revisions) == 1, 'collect the repair reports from the same source commit')
    for revision in revisions:
        scalar.retain.command(['git', 'fetch', '--no-tags', '--depth=1', 'origin', revision],
                              capture_output=True)
    scalar.verify_sources(ROOT, revisions)
    receipts = {}
    for group, payload, identity in collected:
        directory = ROOT / 'tests/agent-eval/results' / (args.prefix + '-' + group)
        require(not directory.exists(), 'preserve previous reports; choose a new directory')
        directory.mkdir()
        report = directory / 'result.json'
        for name, content in payload.items():
            path = directory / name
            require(path.resolve().is_relative_to(directory.resolve()), 'unsafe retained report path')
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(content)
        script = SCRIPTS.get(group, group)
        flag = '--verify' if group == 'index-consumers' else '--audit'
        argument = directory if group == 'structural-change' else report
        subprocess.run(['python3', '-B', f'tools/{script}.py', flag, str(argument)],
                       cwd=ROOT, check=True, timeout=60, stdout=subprocess.DEVNULL)
        receipts[group] = {'collection': identity, 'sha256': hashlib.sha256(payload['result.json']).hexdigest()}
        print(f'Validated {group}', flush=True)
    catalog_path = ROOT / 'tests/agent-eval/roadmap.json'
    catalog = json.loads(catalog_path.read_text())
    if 'host-recovery' in receipts:
        entry, = [row for row in catalog['evidence'] if row['id'] == 'host-recovery']
        entry['path'] = f'tests/agent-eval/results/{args.prefix}-host-recovery/result.json'
        entry['audit'] = 'python3 tools/host-recovery-acceptance.py --audit ' + entry['path']
        entry['sha256'] = receipts['host-recovery']['sha256']
        catalog_path.write_text(json.dumps(catalog, indent=2, ensure_ascii=False) + '\n')
    if 'structural-change' in receipts:
        entry, = [row for row in catalog['evidence'] if row['id'] == 'structural']
        directory = ROOT / 'tests/agent-eval/results' / (args.prefix + '-structural-change')
        entry['path'] = str((directory / 'manifest.json').relative_to(ROOT))
        entry['sha256'] = hashlib.sha256((directory / 'manifest.json').read_bytes()).hexdigest()
        entry['audit'] = 'python3 tools/structural-change-acceptance.py --audit ' + str(directory.relative_to(ROOT))
        entry['scope'] = 'Current Rust rename and parameter removal on a fixed fixture, with 49 compiled public results and exact patch reversal.'
        catalog_path.write_text(json.dumps(catalog, indent=2, ensure_ascii=False) + '\n')
    directory = ROOT / 'tests/agent-eval/results' / (args.prefix + '-ci-collection')
    require(not directory.exists(), 'preserve previous collection receipts')
    directory.mkdir()
    (directory / 'manifest.json').write_text(json.dumps(
        {'schema': 'fr-ci-report-collection', 'groups': receipts}, indent=2) + '\n')
    for group in receipts.keys() & ACTIVE_REFERENCES.keys():
        test = ROOT / ACTIVE_REFERENCES[group]
        suffix = '' if group == 'structural-change' else '/result.json'
        text, count = re.subn(
            r'"tests/agent-eval/results/[^"/]+-' + re.escape(group) + re.escape(suffix) + '"',
            f'"tests/agent-eval/results/{args.prefix}-{group}{suffix}"', test.read_text())
        require(count == 1, 'expected one active replay reference for ' + group)
        test.write_text(text)
    for group in receipts.keys() & ACTIVE_FUNCTIONS.keys():
        test = ROOT / 'tests/python_sdk.rs'
        function = ACTIVE_FUNCTIONS[group]
        pattern = (r'(\bfn ' + re.escape(function) + r'\(\) \{(?:(?!\n#\[test\]).)*?)'
                   r'"tests/agent-eval/[^"\n]+\.json"')
        reference = f'"tests/agent-eval/results/{args.prefix}-{group}/result.json"'
        text, count = re.subn(pattern, lambda match: match[1] + reference, test.read_text(), flags=re.S)
        require(count == 1, 'expected one active SDK comparison reference for ' + group)
        test.write_text(text)
    if receipts.keys() & ACTIVE_FUNCTIONS.keys():
        subprocess.run(['rustup', 'run', '1.99.1', 'rustfmt', '--edition', '2021', 'tests/python_sdk.rs'],
                       cwd=ROOT, check=True, timeout=30)
    subprocess.run(['python3', '-B', 'tools/roadmap-status.py', '--write'], cwd=ROOT, check=True, timeout=60)


if __name__ == '__main__':
    main()
