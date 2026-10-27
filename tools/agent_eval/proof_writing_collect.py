"""Combine hosted proof, source-execution and native-client checks; make no model calls."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re

from . import proof_writing_tasks as tasks
from . import selected_local as local
from .selected_plan import require


def combine(artifacts, commit, run, bindings):
    cases = {}
    for case, entry in tasks.CASES.items():
        proof = json.loads((artifacts / ('m3-proof-controls-' + case) / 'result.json').read_text())
        runtime = json.loads((artifacts / ('m3-source-runtime-' + case) / 'result.json').read_text())
        source = tasks.ROOT / tasks.DIRECTORY / 'fixtures' / entry['path']
        require(runtime['case'] == proof['case'] == case and proof['live_model_calls'] == runtime['live_model_calls'] == 0,
                'controls must identify their exact case without live model calls: ' + case)
        require(runtime['passed'] is True and runtime['source_implementation_proved'] is False and
                runtime['source_sha256'] == hashlib.sha256(source.read_bytes()).hexdigest() and
                runtime['input_cases'] > 0 and runtime['checks'] and
                all(row['returncode'] == 0 for row in runtime['checks']),
                'missing or failed independent source behavior check: ' + case)
        cases[case] = {flag: proof[flag] for flag in tasks.CONTROL_FLAGS - {'runtime_checked'}}
        cases[case].update(runtime_checked=True, source_implementation_proved=proof['source_implementation_proved'])
    native = json.loads((artifacts / 'm3-native-controls/summary.json').read_text())
    profiles = json.loads((tasks.ROOT / 'tests/agent-eval/selected-agents/profiles.json').read_text())
    expected = {name + '-' + setup for name in profiles for setup in ('files', 'fr')}
    require(native['passed'] is True and native['live_model_calls'] == 0 and
            set(native['exchanges']) == expected and all(row['status'] == 'passed' for row in native['exchanges'].values()),
            'all six native clients must finish their scripted exchanges')
    from .proof_writing_results import TASKS, VARIANTS
    private = json.loads((artifacts / 'm3-private-controls/summary.json').read_text())
    require(private['source_commit'] == commit and private['run'] == run and private['bindings'] == bindings and
            re.fullmatch(r'[0-9a-f]{64}', private['fr_sha256']) is not None,
            'isolated controls must check this exact source, run and proof-writing inputs before model calls')
    expected_private = {task + '-' + variant for task in TASKS for variant in VARIANTS}
    require(set(private['controls']) == expected_private and
            all(row == {'status': 'passed', 'accepted': identity.endswith('-reference')}
                for identity, row in private['controls'].items()), 'all eighteen independent patch controls must pass')
    return tasks.validate_controls({'passed': True, 'live_model_calls': 0, 'source_commit': commit, 'run': run,
        'bindings': bindings, 'cases': cases, 'native': {'passed': True, 'live_model_calls': 0, 'exchanges': len(expected)},
        'private': {key: private[key] for key in ('passed', 'live_model_calls', 'checked_controls', 'image', 'image_archive_sha256')}})


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--artifacts', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    arguments = parser.parse_args()
    require(os.environ.get('GITHUB_ACTIONS') == 'true', 'collect control receipts only on their hosted run')
    source = local.identity()
    require(source['commit'] == os.environ['GITHUB_SHA'], 'control receipt source differs from its hosted checkout')
    result = combine(arguments.artifacts, source['commit'], os.environ['GITHUB_RUN_ID'], local.implementation_bindings('M3'))
    tasks.common.save(arguments.output, result)
    print(json.dumps({'passed': result['passed'], 'cases': len(result['cases']), 'live_model_calls': 0}))
