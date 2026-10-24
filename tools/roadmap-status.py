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

from evidence_basis import file_digest, cargo_manifest_basis
from python_repository_basis import file_digest as python_repository_digest

ROOT = Path(__file__).resolve().parents[1]
CATALOG = Path('tests/agent-eval/roadmap.json')
REPORT = Path('docs/roadmap-status.json')
PAGE = Path('docs/roadmap-status.md')
KINDS = {
    'fr-checked-translation': 'deterministic',
    'fr-python-effects': 'deterministic',
    'fr-python-text-flow': 'deterministic',
    'fr-python-text-package': 'deterministic',
    'fr-qualified-text-package': 'deterministic',
    'fr-flow-facts-acceptance-1': 'deterministic',
    'fr-package-flow-acceptance-1': 'deterministic',
    'fr-flow-cache-acceptance-1': 'measurement',
    'fr-compiler-evidence-acceptance-1': 'compiler',
    'fr-compiler-profile-1': 'compiler',
    'fr-python-contract-1': 'semantic-contract',
    'fr-python-contract-2': 'semantic-contract',
    'fr-python-contract-3': 'semantic-contract',
    'fr-index-resolution-result-1': 'measurement',
    'fr-host-recovery-acceptance-1': 'fault-injection',
    'fr-structural-change-artifacts-1': 'deterministic',
    'fr-refinement-artifacts-1': 'model-proof',
    'fr-unknown-target-cohort-1': 'live-agent',
    'fr-python-repository-acceptance-1': 'deterministic',
    'fr-python-repository-acceptance-2': 'deterministic',
    'fr-python-repository-flow-cohort-result-1': 'deterministic',
    'fr-original-package-questions': 'deterministic',
    'fr-project-changes': 'deterministic',
    'fr-project-changes-recheck': 'deterministic',
    'fr-saved-work-cost': 'measurement',
    'fr-saved-answer-improvement': 'measurement',
    'fr-project-session-cost': 'measurement',
    'fr-rechecked-package-questions': 'deterministic',
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


def binding_digest(path, schema, root):
    # Every current collector normalizes workspace members, not every file
    # named Cargo.toml. Nonmember manifests retain their original bytes.
    if path.name == 'Cargo.toml':
        workspace = tomllib.loads((root / 'Cargo.toml').read_text())['workspace']['members']
        members = {root / ('Cargo.toml' if member == '.' else member + '/Cargo.toml') for member in workspace}
        if path in members:
            return hashlib.sha256(cargo_manifest_basis(path.read_bytes())).hexdigest()
    if schema in ('fr-python-repository-acceptance-2', 'fr-original-package-questions', 'fr-rechecked-package-questions'):
        return python_repository_digest(path)
    return file_digest(path)


def evidence(root, entry):
    path = safe(root, entry['path'])
    base = {'id': entry['id'], 'path': entry['path'], 'kind': entry['kind'], 'audit': entry['audit'],
            'scope': entry['scope'], 'integrity': 'missing', 'freshness': 'unknown', 'changed_sources': []}
    if not path.is_file():
        return base, None
    require(digest(path) == entry['sha256'], f"tampered evidence: {entry['id']}")
    value = json.loads(path.read_text())
    require(KINDS.get(value.get('schema')) == entry['kind'], f"evidence kind/schema mismatch: {entry['id']}")
    if value['schema'] == 'fr-original-package-questions':
        receipt = path.with_name('receipt.json')
        require(receipt.is_file() and digest(receipt) == entry['receipt_sha256'], 'package collection receipt changed')
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
    binding_field = 'bindings' if value['schema'] == 'fr-index-resolution-result-1' else 'source_bindings'
    bindings = value.get(binding_field, {})
    require(isinstance(bindings, dict), 'invalid source bindings')
    changed = []
    for name, expected in bindings.items():
        source = safe(root, name)
        if not source.is_file() or binding_digest(source, value['schema'], root) != expected:
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
    project_requirements = {'C.repository-mutations', 'D.structural'}
    project_done = project_requirements == {
        gate['id'] for gate in report['obligations']
        if gate['id'] in project_requirements and gate['state'] == 'demonstrated'}
    cost_done = any(gate['id'] == 'C.performance' and gate['state'] == 'demonstrated'
        for gate in report['obligations'])
    resuming_remaining = 'Measure the cost of resuming work at project scale'
    if project_done:
        resumed_work = 'Saved answers match fresh checks through seven changes on three projects'
        delivered_changes = 'Real Rust and Python renames, including behavior checks, undo and patch replay'
        next_work = (
            'The [project-change milestone](project-change-milestone.md) now has passing original records. '
            'Saved answers stay current through the declared changes, and the Rust and Python renames include their required delivery checks. '
            'Next, measure the cost of continuing a saved task against asking the same questions from scratch on the same changed files. '
            'Count setup, input checking, analysis, memory, disk and text returned. '
            'Measure broader agent savings and proofs through their separate requirements.')
        if cost_done:
            resuming_remaining = 'Use the measured costs to choose an improvement; measure complete agent tasks separately'
            next_work = (
                'The project-change checks and [complete session cost comparison](project-session-cost.md) have matching current records. '
                'The comparison repeats the same questions on three projects before and after one source edit. '
                'It compares CLI calls, fresh and running Python clients, and saved answers. Startup, checking, edits, shutdown and cleanup enter session totals; setup remains separate. '
                'Follow the three-PR roadmap: first make fr usable in all three agents and measure complete costs against ordinary tools and CodeGraph. '
                'Then complete broader Python analysis and translation, followed by source-connected proofs and repeated agent comparisons. '
                'Existing acceptance requirements remain in force; the delivery packages do not change completion claims.')
    else:
        resumed_work = 'Saved changes, undo, checks and recovery tests'
        delivered_changes = 'Reviewed edits, behavior checks and small supported proofs'
        next_work = (
            'Finish the [project-change checks](project-change-milestone.md) before measuring their costs. '
            'Compare saved and fresh answers after renames, moves, deletions, competing imports, configuration changes, question settings changes and analyzer changes. '
            'Complete a function change with its required callers, checks, undo and patch replay in both Rust and Python. '
            'Choose three independent projects and keep one implementation unread until the checker and test procedure are fixed. '
            'Both requirements need matching original records. Broader agent savings and proofs remain separate work.')
    lines = ['# What fr can demonstrate, and what remains', '',
        '<!-- Generated by tools/roadmap-status.py --write; edit the catalog and renderer. -->', '',
        '`fr` helps an agent find code, inspect a proposed change, run checks and keep work that '
        'can be resumed or undone. The goal is to finish unfamiliar tasks correctly with less '
        'reading, fewer retries and lower total cost.', '',
        'For example, a checkout repair should find the calculation and callers, explain the bug, '
        'produce a reviewed patch and pass the test that catches the rounding error. Finding the '
        'function is useful, but it does not establish that whole outcome.', '',
        f'**{done} of {total} specific test requirements have matching recorded results; '
        f'{total - done} remain open. {report["milestones_complete"]} of 4 larger goals are complete.** '
        'These counts do not measure product readiness or savings for an agent.', '',
        'A requirement is one specific behavior we promise to check. A recorded result describes '
        'a chosen program, inputs and tool revision. “Demonstrated” means that result meets its '
        'stated requirement; “open” means required work or matching results are missing.', '',
        '## What works and what still gets in the way', '',
        '| User task | What we have now | What remains |', '|---|---|---|',
        '| Find the code behind a bug | Function locations, known callers and selected source reads | Reliable explanations on unfamiliar tasks with less wasted reading |',
        '| Trace input to an output | Selected original-package questions in Boltons, dateutil, JDcal and Stringcase, including number, date and text values | Other imports and unsupported behavior still leave many answers incomplete |',
        f'| Continue after a failed attempt | {resumed_work} | {resuming_remaining} |',
        f'| Deliver a repair or check a property | {delivered_changes} | Independent agent task success and a checked connection between proofs and actual code |', '',
        '**We have not demonstrated general time or cost savings.** The older two-task comparison '
        'used more calls, reading and time with fr than with ordinary file tools. The recent Python repository scripts apply '
        'known repairs; they test editing and recovery. They do not measure an agent discovering '
        'the repair. The [measurement guide](benchmarks.md) explains baselines, included costs and '
        'unmeasured overhead. Read the [product review](product-review.md) and [result meanings](understanding-results.md).', '',
        '## What happens next', '',
        next_work, '',
        'See the [plan](../PLAN.md#next-three-prs) for the work sequence and '
        '[generalization requirements](../PLAN.md#generalization-requirements) for the testing policy.', '',
        '## Milestones', '',
        'The implementation column counts completed code changes. The test column counts specific '
        'requirements with matching saved results. A larger goal needs all of its required work, '
        'not just more code or more examples.', '',
        '| Milestone | Implementation items | Demonstrated tests | State |', '|---|---:|---:|---|']
    for name, item in report['milestones'].items():
        engineering = item['engineering']
        lines.append(f"| {name}. {names[name]} | {engineering['done']}/{engineering['total']} | "
                     f"{item['demonstrated']}/{item['total']} | {item['state']} |")
    lines += ['', '## Finite acceptance obligations', '',
        'These are the specific requirements, checked one by one. Each item gives a user example, '
        'what must be established, its recorded results and the next action. IDs such as '
        '`B.contract` let tests refer to the same requirement; readers can follow the accompanying question.', '',
        'An oracle is an independent reference check, such as Python execution. A source binding '
        'records fingerprints of inputs behind a saved result. Changed inputs require renewed checks. '
        'A held-out project stays unread while developing the tool, then tests whether the rules '
        'work beyond familiar examples.', '']
    lines += [
        'A source supplies a tracked input; a sink consumes it. A sanitizer is a user-declared '
        'cleaning function for a particular output context, such as HTML. An effect is another '
        'action the code performs, such as writing a file.', '',
        'A cache saves earlier results for possible reuse. Reuse must check their recorded inputs '
        'before trusting them. Latency means elapsed request time; aggregate memory measures the '
        'combined memory held by the measured processes.', '',
    ]
    examples = {
        'A.exact': 'An agent explains a bug and links the exact call and source lines it inspected.',
        'A.repositories': 'Replay the two prescribed repository repairs and check their behavior, saved changes and delivery.',
        'A.live-repeat': 'Compare agents fixing the same unfamiliar bug, counting all work including failed attempts.',
        'A.compiler-profile': 'A Rust expression parses, but the selected build reports a type error at that expression.',
        'B.scalar': 'Follow an input through a simple helper and check whether overwriting the value removes the original connection.',
        'B.contract': 'A helper returns its left argument; check selected, ignored and overwritten input cases against Python.',
        'B.repository-flow': 'Trace an input through a real imported library without mistaking an unfinished answer for safety.',
        'B.effects': 'A helper mutates a shared list; the answer must account for later reads of that same object.',
        'C.resume': 'After another agent edits the helper, recheck the finding that depended on it and keep unrelated notes.',
        'C.clean': 'A reused answer must agree with a fresh analysis of the same current code.',
        'C.repository-mutations': 'Move a helper or add a previously missing module and check the conclusions that depend on it.',
        'C.performance': 'Measure whether saved answers reduce total work and storage on a real project after edits.',
        'D.checked': 'Apply a reviewed repair, run its required checks and verify the exported patch in another checkout.',
        'D.structural': 'Rename Boltons\' `clamp` function to `clamp_value` and update its callers and documentation. Limiting 70 to the range 0 through 50 must still return 50.',
        'D.translation': 'Move a small helper to another language and check its promised inputs, outputs and error behavior.',
        'D.existing-proof': 'Check that a proof of an access rule describes the actual function and its language behavior.',
        'D.new-proof': 'Write a new access function together with its required rule and a checked proof of that rule.',
        'D.host': 'Resume or undo an interrupted edit without overwriting someone else’s newer work.',
    }
    by_id = {item['id']: item for item in report['evidence']}
    for gate in report['obligations']:
        if gate['blockers']:
            evidence_text = ' '.join(explain_blocker(blocker) for blocker in gate['blockers'])
        else:
            evidence_text = ', '.join(
                f"[{item['evidence']}](../{by_id[item['evidence']]['path']})"
                for item in gate['requires'])
        lines += [f"### {gate['id']}: {gate.get('title', gate['id'])}", '',
            f"User example: {examples[gate['id']]}", '',
            f"**State: {gate['state']}.** {gate['acceptance']}", '',
            f"Recorded results or remaining work: {evidence_text.rstrip('.')}.", '', f"Next: {gate['next']}", '']
    lines += ['## Evidence ledger', '',
        'The entries below identify the stored test results. “Current” means their recorded source '
        'inputs still match; “stale” means at least one input changed; “historical” means they '
        'cannot close a current item. “Checked” means saved files match their recorded fingerprints. The report checks '
        'hashes and assertions. Independent behavior tests still need to run in CI.', '',
        '| Saved result / kind of test | File fingerprints | Do recorded inputs still match? | What it checked and how to check it again |', '|---|---|---|---|']
    for item in report['evidence']:
        fresh = item['freshness']
        if item['changed_sources']:
            fresh += f' ({len(item["changed_sources"])} changed inputs)'
        kinds = {
            'deterministic': 'scripted behavior checks', 'measurement': 'measured tool operations',
            'compiler': 'compiler and execution comparisons', 'semantic-contract': 'Python rule comparisons',
            'fault-injection': 'deliberately interrupted operations', 'model-proof': 'proofs of stated models',
            'live-agent': 'recorded agent attempts', 'source-proof': 'proofs connected to source',
        }
        kind = kinds.get(item['kind'], item['kind'])
        lines.append(f"| [{item['id']}](../{item['path']}) / {kind} | {item['integrity']} | "
                     f"{fresh} | {item['scope']} Audit: `{item['audit']}` |")
    lines += ['', 'Exact changed input paths are retained in [the JSON report](roadmap-status.json).', '',
        '## Limits and interpretation', '']
    lines += ['- ' + boundary for boundary in report['boundaries']]
    lines += ['', f"Evidence baseline: `{report['baseline']}`. Acceptance requirements: `{report['profile']}`.", '',
        'Run `python3 tools/roadmap-status.py --check` to detect report drift, `--write` to regenerate '
        'both reports after reviewing changes, or `--require-complete` to require all milestone tests '
        'and implementation items. On this workstation, use the local resource guard.', '']
    return '\n'.join(lines)


def explain_blocker(blocker):
    if blocker == 'No qualifying retained evidence yet':
        return 'No saved result yet meets this requirement.'
    labels = {
        'missing evidence': 'the saved result is missing',
        'stale source binding': 'recorded source inputs changed; collect and check a new result',
        'historical source binding': 'this historical result cannot establish current behavior',
    }
    for suffix, explanation in labels.items():
        if blocker.endswith(': ' + suffix):
            return blocker[:-len(suffix)] + explanation + '.'
    if ': unmet ' in blocker:
        name, field = blocker.split(': unmet ', 1)
        return f'{name}: the recorded value at `{field}` differs from this requirement; review the inventory and result.'
    if ': missing ' in blocker:
        name, field = blocker.split(': missing ', 1)
        return f'{name}: the result lacks required field `{field}`.'
    return blocker


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
