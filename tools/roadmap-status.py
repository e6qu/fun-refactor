#!/usr/bin/env python3
"""Compute finite roadmap gates without equating implementation with outcomes.

This audits evidence identity/freshness, not a new execution of every oracle.
The named evaluator commands remain the behavioral audit authority in CI.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path, PurePosixPath
import re
import sys
import tomllib

from evidence_basis import file_digest

ROOT = Path(__file__).resolve().parents[1]
CATALOG = Path('tests/agent-eval/roadmap.json')
REPORT = Path('docs/roadmap-status.json')
PAGE = Path('docs/roadmap-status.md')
KINDS = {
    'fr-flow-facts-acceptance-1': 'deterministic',
    'fr-package-flow-acceptance-1': 'deterministic',
    'fr-flow-cache-acceptance-1': 'measurement',
    'fr-compiler-evidence-acceptance-1': 'compiler',
    'fr-host-recovery-acceptance-1': 'fault-injection',
    'fr-structural-change-artifacts-1': 'deterministic',
    'fr-refinement-artifacts-1': 'model-proof',
    'fr-unknown-target-cohort-1': 'live-agent',
    'fr-python-repository-acceptance-1': 'deterministic',
}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def safe(root, name):
    require(isinstance(name, str) and name != '', 'empty evidence path')
    path = PurePosixPath(name)
    require(not path.is_absolute() and '..' not in path.parts and str(path) == name, f'unsafe path: {name}')
    result = root / name
    require(result.resolve().is_relative_to(root.resolve()), f'escaping path: {name}')
    return result


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def pointer(value, path):
    require(path.startswith('/'), f'invalid evidence pointer: {path}')
    for part in path[1:].split('/'):
        key = part.replace('~1', '/').replace('~0', '~')
        value = value[int(key)] if isinstance(value, list) else value[key]
    return value


def binding_digest(path, schema):
    # The cache evaluator normalizes workspace release versions in Cargo.toml.
    # Its source bindings otherwise use the shared evidence_basis convention.
    if schema != 'fr-flow-cache-acceptance-1' or path.name != 'Cargo.toml':
        return file_digest(path)
    manifest = tomllib.loads(path.read_text())
    version = manifest['package']['version']
    manifest['package']['version'] = '<workspace>'
    for container in [manifest, manifest.get('workspace', {}), *manifest.get('target', {}).values()]:
        for kind in ('dependencies', 'dev-dependencies', 'build-dependencies'):
            for dependency in container.get(kind, {}).values():
                if isinstance(dependency, dict) and 'path' in dependency and dependency.get('version') == version:
                    dependency['version'] = '<workspace>'
    data = json.dumps(manifest, sort_keys=True, ensure_ascii=False, separators=(',', ':'), allow_nan=False).encode()
    return hashlib.sha256(data).hexdigest()


def evidence(root, entry):
    path = safe(root, entry['path'])
    base = {'id': entry['id'], 'path': entry['path'], 'kind': entry['kind'], 'audit': entry['audit'],
            'scope': entry['scope'], 'integrity': 'missing', 'freshness': 'unknown', 'changed_sources': []}
    if not path.is_file():
        return base, None
    require(digest(path) == entry['sha256'], f"tampered evidence: {entry['id']}")
    value = json.loads(path.read_text())
    require(KINDS.get(value.get('schema')) == entry['kind'], f"evidence kind/schema mismatch: {entry['id']}")
    # Artifact manifests and nested live trial manifests must retain their exact identities.
    files = value.get('artifacts', value.get('files', {}))
    require(isinstance(files, dict), f"invalid artifact table: {entry['id']}")
    for name, expected in files.items():
        child = safe(path.parent, name)
        require(child.is_file() and digest(child) == expected, f"tampered artifact: {entry['id']}/{name}")
    if value['schema'] == 'fr-unknown-target-cohort-1':
        for trial in value['trials']:
            child = safe(root, trial['path']) / 'manifest.json'
            require(child.is_file() and digest(child) == trial['manifest_sha256'], 'tampered live trial manifest')
    bindings = value.get('source_bindings', {})
    require(isinstance(bindings, dict), 'invalid source bindings')
    changed = []
    for name, expected in bindings.items():
        source = safe(root, name)
        if not source.is_file() or binding_digest(source, value['schema']) != expected:
            changed.append(name)
    base.update(integrity='checked', freshness=('stale' if changed else 'current') if bindings else 'historical',
                changed_sources=sorted(changed))
    return base, value


def compute(root, catalog):
    require(catalog['schema'] == 'fr-roadmap-catalog-1', 'unknown roadmap schema')
    require(catalog['profile'] == 'agent-analysis-v2', 'unknown acceptance profile')
    entries = catalog['evidence']
    require(len({e['id'] for e in entries}) == len(entries), 'duplicate evidence IDs')
    evaluated = {e['id']: evidence(root, e) for e in entries}
    obligations = catalog['obligations']
    ids = [o['id'] for o in obligations]
    require(len(set(ids)) == len(ids), 'duplicate obligation IDs')
    require(all(re.fullmatch(r'[ABCD]\.[a-z][a-z0-9-]+', i) for i in ids), 'invalid obligation ID')
    gates = []
    for obligation in obligations:
        require(obligation['acceptance'] and obligation['next'], 'gate requires acceptance and next action')
        require(obligation['kind'] in set(KINDS.values()) | {'source-proof', 'semantic-contract'}, 'unknown gate evidence kind')
        requirements = obligation['requires']
        blockers = []
        if not requirements:
            blockers.append('No qualifying retained evidence yet')
        for requirement in requirements:
            require(requirement['evidence'] in evaluated, 'unknown evidence reference')
            item, value = evaluated[requirement['evidence']]
            require(item['kind'] == obligation['kind'], f"wrong evidence kind for {obligation['id']}")
            if item['integrity'] != 'checked':
                blockers.append(f"{item['id']}: missing evidence")
                continue
            if item['freshness'] != 'current':
                blockers.append(f"{item['id']}: {item['freshness']} source binding")
            require(requirement['assertions'], 'gate requires explicit evidence assertions')
            for assertion in requirement['assertions']:
                require(set(assertion) == {'pointer', 'equals'}, 'unknown evidence assertion')
                try:
                    actual = pointer(value, assertion['pointer'])
                except (KeyError, IndexError, ValueError, TypeError):
                    blockers.append(f"{item['id']}: missing {assertion['pointer']}")
                    continue
                wanted = assertion['equals']
                if type(actual) is not type(wanted) or actual != wanted:
                    blockers.append(f"{item['id']}: unmet {assertion['pointer']}")
        gates.append({**obligation, 'state': 'open' if blockers else 'demonstrated', 'blockers': blockers})
    plan = (root / 'PLAN.md').read_text()
    engineering = {}
    for milestone in 'ABCD':
        match = re.search(rf'^### {milestone}\. .*?\n(.*?)(?=^### |^## |\Z)', plan, re.M | re.S)
        require(match is not None, f'missing PLAN milestone {milestone}')
        checkboxes = re.findall(r'^- \[([ x])\]', match.group(1), re.M)
        expected = catalog['engineering'][milestone]
        require({'done': checkboxes.count('x'), 'total': len(checkboxes)} == expected,
                f'PLAN engineering counts drifted for {milestone}; review the catalog')
        engineering[milestone] = expected
    milestones = {}
    for milestone in 'ABCD':
        selected = [g for g in gates if g['id'].startswith(milestone + '.')]
        require(selected, f'empty milestone: {milestone}')
        remaining = [g['id'] for g in selected if g['state'] != 'demonstrated']
        engineering_open = engineering[milestone]['total'] - engineering[milestone]['done']
        milestones[milestone] = {'engineering': engineering[milestone], 'engineering_remaining': engineering_open,
                                 'state': 'open' if remaining or engineering_open else 'complete',
                                 'demonstrated': len(selected) - len(remaining), 'total': len(selected), 'remaining': remaining}
    return {'schema': 'fr-roadmap-status-1', 'profile': catalog['profile'], 'baseline': catalog['baseline'],
            'milestones_complete': sum(m['state'] == 'complete' for m in milestones.values()),
            'milestones': milestones, 'obligations': gates, 'evidence': [e[0] for e in evaluated.values()],
            'boundaries': catalog['boundaries']}


def markdown(report):
    names = {
        'A': 'Find relevant code and explain the evidence',
        'B': 'Explain how values move through code',
        'C': 'Resume work and reuse correct answers',
        'D': 'Make checked changes and useful proofs',
    }
    done = sum(g['state'] == 'demonstrated' for g in report['obligations'])
    total = len(report['obligations'])
    lines = ['# What fr can demonstrate, and what remains', '',
        '<!-- Generated by tools/roadmap-status.py --write; edit the catalog and renderer. -->', '',
        '`fr` is a CLI for AI agents to understand code, make changes and check or prove results. '
        'The goal is correct work with less total agent effort and context. Progressive disclosure '
        'means starting with a small overview and asking for details only when needed.', '',
        f'**{done}/{total} technical acceptance items are demonstrated in their recorded test cases; '
        f'{total - done} remain open. {report["milestones_complete"]}/4 milestones are complete.** '
        'These counts are not a percentage of product readiness.', '',
        'The CLI already provides code navigation, bounded reads, reviewed edits, checks, undo/redo '
        'and patch delivery. Current tests cover useful cases of Python value tracing and restarting '
        'work after code changes. Support is limited to the language features stated in each test.', '',
        '**Agent efficiency remains unproven.** The older two-task live comparison used more calls, '
        'context and time with fr. The recent Python repository runs use prescribed edits; they test '
        'delivery and recovery, not independent diagnosis. See the [product review](product-review.md) '
        'for results, removal candidates and the planned Sonnet/Luna comparison.', '',
        '## What happens next', '',
        'Measure complete agent tasks, including subagents and cached tokens, and compare overlapping '
        'guide and editing routes before expanding them. Then improve reusable language behavior where '
        'observed failures justify it. Future project-scale checks include an unfamiliar project held '
        'out from implementation; benchmark names and expected patches must not select analyzer behavior.', '',
        'See the [plan](../PLAN.md#next-large-chunk) for the work sequence and '
        '[generalization requirements](../PLAN.md#generalization-requirements) for the testing policy.', '',
        '## Milestones', '',
        'Implementation checkboxes say code was added. Demonstrated items say the specific tests below '
        'have retained results with matching inputs. Neither alone establishes a complete milestone.', '',
        '| Milestone | Implementation items | Demonstrated tests | State |', '|---|---:|---:|---|']
    for name, item in report['milestones'].items():
        engineering = item['engineering']
        lines.append(f"| {name}. {names[name]} | {engineering['done']}/{engineering['total']} | "
                     f"{item['demonstrated']}/{item['total']} | {item['state']} |")
    lines += ['', '## Finite acceptance obligations', '',
        'Each item states what must be checked and what happens next. IDs are stable references for '
        'tests and reports. “Demonstrated” applies only to the described cases. It does not mean the '
        'tool handles every project or saves agent tokens.', '']
    for gate in report['obligations']:
        evidence_text = '; '.join(gate['blockers']) or ', '.join(r['evidence'] for r in gate['requires'])
        lines += [f"### {gate['id']}: {gate.get('title', gate['id'])}", '',
            f"**State: {gate['state']}.** {gate['acceptance']}", '',
            f"Evidence or missing work: {evidence_text}.", '', f"Next: {gate['next']}", '']
    lines += ['## Evidence ledger', '',
        'The entries below identify the stored test results. “Current” means their recorded source '
        'inputs still match; “historical” means they cannot close a current item. The report checks '
        'hashes and assertions. Independent behavior tests still need to run in CI.', '',
        '| Result / test kind | Files verified | Input freshness | Scope and audit command |', '|---|---|---|---|']
    for item in report['evidence']:
        fresh = item['freshness']
        if item['changed_sources']:
            fresh += f' ({len(item["changed_sources"])} changed inputs)'
        lines.append(f"| [{item['id']}](../{item['path']}) / {item['kind']} | {item['integrity']} | "
                     f"{fresh} | {item['scope']} Audit: `{item['audit']}` |")
    lines += ['', 'Exact changed input paths are retained in [the JSON report](roadmap-status.json).', '',
        '## Limits and interpretation', '']
    lines += ['- ' + boundary for boundary in report['boundaries']]
    lines += ['', f"Evidence baseline: `{report['baseline']}`. Acceptance requirements: `{report['profile']}`.", '',
        'Run `python3 tools/roadmap-status.py --check` to detect report drift, `--write` to regenerate '
        'both reports after reviewing changes, or `--require-complete` to require all milestone tests '
        'and implementation items. On this workstation, use the local resource guard.', '']
    return '\n'.join(lines)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--check', action='store_true')
    parser.add_argument('--write', action='store_true')
    parser.add_argument('--require-complete', action='store_true')
    args = parser.parse_args()
    require(not (args.check and args.write), 'choose --check or --write')
    report = compute(ROOT, json.loads((ROOT / CATALOG).read_text()))
    rendered = {REPORT: json.dumps(report, indent=2, sort_keys=True) + '\n', PAGE: markdown(report)}
    for path, text in rendered.items():
        if args.write:
            (ROOT / path).write_text(text)
        if args.check:
            require((ROOT / path).is_file() and (ROOT / path).read_text() == text, f'stale generated report: {path}')
    if args.require_complete:
        require(report['milestones_complete'] == 4, 'roadmap has open obligations')
    print(json.dumps({'milestones_complete': report['milestones_complete'], 'milestones': report['milestones']}))


if __name__ == '__main__':
    try:
        main()
    except (ValueError, KeyError, TypeError) as error:
        sys.exit(str(error))
