"""Require the reviewed Python acceptance catalog to match its audited corpus."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def require(condition, message):
    if not condition:
        raise ValueError(message)


def check_catalog(catalog, summary):
    gates = [row for row in catalog['obligations'] if row['id'] == 'B.contract']
    require(len(gates) == 1 and gates[0]['kind'] == 'semantic-contract', 'need one Python contract gate')
    requirements = gates[0]['requires']
    require(len(requirements) == 1 and requirements[0]['evidence'] == 'python-contract', 'Python contract evidence selection changed')
    assertions = requirements[0]['assertions']
    expected = {'/summary/' + key: value for key, value in summary.items()}
    require(len(assertions) == len(expected) and
            {item['pointer'] for item in assertions} == set(expected), 'acceptance catalog omits or duplicates summary checks')
    for item in assertions:
        require(set(item) == {'pointer', 'equals'}, 'unknown acceptance assertion')
        actual = expected[item['pointer']]
        require(type(item['equals']) is type(actual) and item['equals'] == actual,
                f"review {item['pointer']}: catalog={item['equals']!r}, audited={actual!r}")
    require(summary['contract_complete'] is True, 'Python contract audit is incomplete')


def rule_coverage(cases, rules):
    pairs = {}
    totals = {name: {'examples': 0, 'cases': 0, 'complete': 0, 'incomplete': 0, 'refused': 0} for name in rules}
    for case in cases:
        base, variant = case['id'].rsplit('/', 1)
        require(variant in {'original', 'moved'} and case['variant'] == variant, 'invalid case variant')
        pair = pairs.setdefault(base, {})
        require(variant not in pair, 'duplicate case variant')
        checked = case['rules_checked']
        require(checked and len(set(checked)) == len(checked) and set(checked) <= set(rules), 'invalid rule selection')
        model = case['expected_model']
        require(model['outcome'] in {'report', 'refused'}, 'invalid expected outcome')
        if model['outcome'] == 'refused':
            state = 'refused'
        else:
            require(type(model['complete']) is bool, 'completeness must be a Boolean')
            state = 'complete' if model['complete'] else 'incomplete'
        pair[variant] = (set(checked), state)
        for name in checked:
            totals[name]['cases'] += 1
            totals[name][state] += 1
    for pair in pairs.values():
        require(set(pair) == {'original', 'moved'} and pair['original'] == pair['moved'],
                'every example needs equivalent original and renamed/moved expectations')
        for name in pair['original'][0]:
            totals[name]['examples'] += 1
    require(all(row['examples'] > 0 for row in totals.values()), 'a declared rule has no paired examples')
    return totals


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--report', type=Path, help='Fresh hosted report before retention')
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    spec = importlib.util.spec_from_file_location('contract', ROOT / 'tools/python-contract.py')
    contract = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(contract)
    catalog = json.loads((ROOT / 'tests/agent-eval/roadmap.json').read_text())
    if args.report:
        require(os.environ.get('GITHUB_ACTIONS') == 'true', 'fresh reports are qualified on GitHub')
        path = args.report
        value = json.loads(path.read_text())
    else:
        entries = [row for row in catalog['evidence'] if row['id'] == 'python-contract']
        require(len(entries) == 1, 'need one retained Python contract')
        path = (ROOT / entries[0]['path']).resolve()
        require(path.is_relative_to(ROOT), 'retained contract path escapes the checkout')
        require(hashlib.sha256(path.read_bytes()).hexdigest() == entries[0]['sha256'], 'retained contract digest changed')
        value = contract.profile.load('retain-python-contract').retained_report(path)
    summary = contract.audit(value)
    check_catalog(catalog, summary)
    result = {'schema': 'fr-python-contract-qualification-1', 'summary': summary,
              'rules': rule_coverage(contract.cases(), contract.RULES),
              'report_sha256': hashlib.sha256(path.read_bytes()).hexdigest(),
              'source_commit': value['source_commit'],
              'scope': 'The declared Python corpus; broader language rules, source proofs and agent outcomes remain separate.'}
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(result, indent=2, sort_keys=True) + '\n')
    print(json.dumps(result, sort_keys=True))


if __name__ == '__main__':
    main()
