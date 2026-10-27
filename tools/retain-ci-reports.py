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
    'checked-outcomes': ['checked-outcomes'],
    'model-comparisons': ['model-comparisons'],
    'contexts': ['project-batch-context', 'task-bundle-context', 'task-change-context', 'workflow-context'],
    'host-recovery': ['host-recovery'],
    'retained-proofs': ['retained-proofs'],
    'resumable-correspondence': ['resumable-correspondence'],
    'index-consumers': ['index-consumers'],
    'completion-workflows': ['completion-workflows'],
    'structural-change': ['structural-change'],
    'package-flow': ['package-flow'],
    'python-repositories': ['python-repositories'],
    'compiler-profile': ['compiler-profile'],
    'sdk-contexts': ['agent-context-workspace', 'agent-guide-context', 'agent-runtime-context',
                     'native-intent-context', 'intent-action-context'],
    'all': ['model-comparisons', 'checked-outcomes', 'completion-workflows', 'structural-change'],
    'dependency': ['retained-proofs', 'host-recovery', 'resumable-correspondence', 'index-consumers'],
}
ARTIFACT_GROUPS = {**GROUPS,
    'all': ['model-comparisons', 'checked-outcomes', 'retained-proofs', 'agent-guide-context',
            'completion-workflows', 'structural-change', 'intent-action-context', 'index-resolution', 'host-recovery'],
    'dependency': ['compiler-evidence', 'resumable-correspondence', 'semantic-evidence',
                   'index-consumers', 'retained-proofs', 'host-recovery'],
}
SCRIPTS = {
    'checked-outcomes': 'outcome-acceptance',
    'model-comparisons': 'refinement-acceptance',
    'host-recovery': 'host-recovery-acceptance',
    'retained-proofs': 'proof-evidence-acceptance',
    'resumable-correspondence': 'correspondence-acceptance',
    'index-consumers': 'index-consumers-acceptance',
    'structural-change': 'structural-change-acceptance',
    'package-flow': 'package-flow-acceptance',
    'python-repositories': 'python-repository-acceptance',
    'compiler-profile': 'compiler-profile',
}
ACTIVE_REFERENCES = {
    'checked-outcomes': 'tests/agent_acceptance.rs',
    'model-comparisons': 'tests/model_comparisons.rs',
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
DIRECTORY_REPORTS = {'structural-change', 'checked-outcomes', 'model-comparisons'}
REGISTERED_REPORTS = {
    'structural-change': 'rust-scoped-structural-migration',
    'model-comparisons': 'retained-boolean-model-comparisons',
}


def collection_reports(data, collection, identity):
    """Validate the complete named batch before selecting its assigned reports."""
    prefixes, inventory = set(), set()
    payloads = {}
    for group in ARTIFACT_GROUPS[collection]:
        matches = [name for name in data if re.fullmatch(
            r'refinement-evidence/[a-zA-Z0-9][a-zA-Z0-9_-]{0,79}-' + re.escape(group) + '/result.json', name)]
        require(len(matches) == 1, 'need exactly one report in the declared collection: ' + group)
        path = matches[0]
        prefixes.add(path.removesuffix('-' + group + '/result.json'))
        parent = str(Path(path).parent) + '/'
        if group in DIRECTORY_REPORTS:
            payload = {name[len(parent):]: content for name, content in data.items() if name.startswith(parent)}
            require('manifest.json' in payload, 'directory collection needs its manifest')
            manifest = json.loads(payload['manifest.json'])
            source_key = 'revision' if group == 'checked-outcomes' else 'source_commit'
            require(manifest[source_key] == identity['source_commit'], 'directory collection source changed')
            require(set(payload) == {'manifest.json', *manifest['files']}, 'directory archive inventory changed')
            for name, expected in manifest['files'].items():
                require(hashlib.sha256(payload[name]).hexdigest() == expected, 'directory artifact changed: ' + name)
        else:
            payload = {'result.json': data[path]}
        inventory.update(parent + name for name in payload)
        payloads[group] = payload
    require(len(prefixes) == 1 and all(name in inventory or name.startswith('refinement-refresh-logs/') for name in data),
            'collection prefixes or report artifact inventory changed')
    return {group: payloads[group] for group in GROUPS[collection]}


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
    selected = [group for collection in runs for group in GROUPS[collection]]
    require(len(selected) == len(set(selected)), 'assign each retained report to exactly one collection')
    collected = []
    for collection in sorted(runs):
        if collection == 'python-repositories':
            artifact, producer = 'roadmap-evidence', 'repositories'
        elif collection == 'compiler-profile':
            artifact, producer = 'compiler-profile', 'compiler-profile'
        else:
            artifact, producer = 'refinement-evidence', 'refresh'
        data, identity = scalar.collect(runs[collection], artifact, producer,
                                        os.environ['GITHUB_REPOSITORY'])
        if collection == 'python-repositories':
            value = json.loads(data['result.json'])
            require(value['runner']['revision'] == identity['source_commit']
                    and set(data) == {'result.json', *value['artifacts']},
                    'repository source or artifact inventory changed')
            collected.append((collection, data, identity))
            continue
        if collection == 'compiler-profile':
            require(set(data) == {'result.json'}, 'compiler report inventory changed')
            require(json.loads(data['result.json'])['source_commit'] == identity['source_commit'],
                    'compiler report source changed')
            collected.append((collection, data, identity))
            continue
        for group, payload in collection_reports(data, collection, identity).items():
            collected.append((group, payload, identity))
    revisions = {identity['source_commit'] for _, _, identity in collected}
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
        argument = directory if group in DIRECTORY_REPORTS | {'python-repositories'} else report
        subprocess.run(['python3', '-B', f'tools/{script}.py', flag, str(argument)],
                       cwd=ROOT, check=True, timeout=60, stdout=subprocess.DEVNULL)
        receipts[group] = {'collection': identity, 'sha256': hashlib.sha256(payload['result.json']).hexdigest()}
        print(f'Validated {group}', flush=True)
    catalog_path = ROOT / 'tests/agent-eval/roadmap.json'
    catalog = json.loads(catalog_path.read_text())
    if 'model-comparisons' in receipts:
        entry, = [row for row in catalog['evidence'] if row['id'] == 'model-relations']
        directory = ROOT / 'tests/agent-eval/results' / (args.prefix + '-model-comparisons')
        entry.update(path=str((directory / 'manifest.json').relative_to(ROOT)),
                     sha256=hashlib.sha256((directory / 'manifest.json').read_bytes()).hexdigest(),
                     audit='python3 -B tools/refinement-acceptance.py --audit ' + str(directory.relative_to(ROOT)))
        catalog_path.write_text(json.dumps(catalog, indent=2, ensure_ascii=False) + '\n')
    for group, identifier in (('package-flow', 'package-flow'), ('python-repositories', 'repository-tasks'),
                              ('compiler-profile', 'compiler-profile')):
        if group in receipts:
            entry, = [row for row in catalog['evidence'] if row['id'] == identifier]
            directory = f'tests/agent-eval/results/{args.prefix}-{group}'
            entry.update(path=directory + '/result.json', sha256=receipts[group]['sha256'],
                         audit='python3 -B tools/' + SCRIPTS[group] + '.py --audit ' +
                         (directory if group == 'python-repositories' else directory + '/result.json'))
    if receipts.keys() & {'package-flow', 'python-repositories', 'compiler-profile'}:
        catalog_path.write_text(json.dumps(catalog, indent=2, ensure_ascii=False) + '\n')
    if 'compiler-profile' in receipts:
        receipt = receipts['compiler-profile']
        family = ROOT / 'tests/agent-eval/results' / (args.prefix + '-scalar-collection')
        family.mkdir()
        (family / 'manifest.json').write_text(json.dumps({'schema': 'fr-scalar-collection', 'groups': {
            'compiler-profile': {'collection': receipt['collection'], 'files': {'result.json': receipt['sha256']}}
        }}, indent=2) + '\n')
        test = ROOT / 'tools/test-compiler-profile.py'
        text, count = re.subn(r"^REPORT = ROOT / '[^']+'$",
            f"REPORT = ROOT / 'tests/agent-eval/results/{args.prefix}-compiler-profile/result.json'",
            test.read_text(), flags=re.M)
        require(count == 1, 'expected one current compiler report reference')
        test.write_text(text)
        subprocess.run(['python3', '-B', 'tools/test-compiler-profile.py'], cwd=ROOT, check=True, timeout=120)
        subprocess.run(['git', 'add', '--', 'tools/test-compiler-profile.py'], cwd=ROOT, check=True)
    settings_path = ROOT / 'tests/agent-eval/current-scalar-reports.json'
    settings = json.loads(settings_path.read_text())
    require(settings['schema'] == 'fr-current-scalar-reports', 'unknown current report settings')
    selected = receipts.keys() & settings['reports'].keys()
    if selected:
        for group in selected:
            settings['reports'][group].update(
                path=f'tests/agent-eval/results/{args.prefix}-{group}/result.json',
                sha256=receipts[group]['sha256'])
        settings_path.write_text(json.dumps(settings, indent=2) + '\n')
        subprocess.run(['git', 'add', '--', 'tests/agent-eval/current-scalar-reports.json'], cwd=ROOT, check=True)
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
        suffix = '' if group in DIRECTORY_REPORTS else '/result.json'
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
    rust_tests = {ACTIVE_REFERENCES[group] for group in receipts.keys() & ACTIVE_REFERENCES.keys()
                  if ACTIVE_REFERENCES[group].endswith('.rs')}
    if receipts.keys() & ACTIVE_FUNCTIONS.keys():
        rust_tests.add('tests/python_sdk.rs')
    if rust_tests:
        subprocess.run(['rustup', 'run', '1.99.1', 'rustfmt', '--edition', '2021', *sorted(rust_tests)],
                       cwd=ROOT, check=True, timeout=30)
    registered = receipts.keys() & REGISTERED_REPORTS.keys()
    if registered:
        registry_path = ROOT / 'tests/agent-eval/representative-acceptance.json'
        registry = json.loads(registry_path.read_text())
        for group in sorted(registered):
            case, = [row for row in registry['cases'] if row['id'] == REGISTERED_REPORTS[group]
                     and row['runner'] == ACTIVE_REFERENCES[group]]
            case['runner_sha256'] = hashlib.sha256((ROOT / case['runner']).read_bytes()).hexdigest()
        registry_path.write_text(json.dumps(registry, indent=2) + '\n')
        subprocess.run(['python3', '-B', 'tools/representative-acceptance.py', 'audit'],
                       cwd=ROOT, check=True, timeout=60, stdout=subprocess.DEVNULL)
        subprocess.run(['git', 'add', '--', 'tests/agent-eval/representative-acceptance.json'], cwd=ROOT, check=True)
    subprocess.run(['python3', '-B', 'tools/roadmap-status.py', '--write'], cwd=ROOT, check=True, timeout=60)


if __name__ == '__main__':
    main()
