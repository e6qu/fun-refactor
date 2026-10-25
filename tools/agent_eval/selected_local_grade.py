"""Independently check retained local CLI attempts in private GitHub containers."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile

from . import selected_grading as grading
from . import selected_local as local
from .selected_plan import encoded, prepare, require


def run(data, binary, output, completion):
    require(os.environ.get('GITHUB_ACTIONS') == 'true', 'independently grade retained local attempts on GitHub')
    require(completion in {'partial', 'complete'}, 'choose whether all 36 checked attempts are required')
    output.mkdir(parents=True, exist_ok=False)
    plan = json.loads((data / 'plan.json').read_text())
    unsigned = {key: value for key, value in plan.items() if key != 'sha256'}
    require(hashlib.sha256(encoded(unsigned)).hexdigest() == plan['sha256'], 'local execution plan changed')
    require(plan['implementation'] == local.implementation_bindings(), 'checking code or model inputs changed after freeze')
    require(plan['schedule'] == prepare(local.ROOT), 'local schedule differs from current task requirements')
    require(plan['source']['native_inputs'] == local.identity()['native_inputs'], 'fr native source differs from the study')
    rust = Path(subprocess.check_output(['rustc', '--print', 'sysroot'], timeout=10).decode().strip())
    image = grading.build(output / 'image')
    summary = {'passed': False, 'plan_sha256': plan['sha256'], 'live_model_calls': 0,
        'scope': 'Independent checks of actual local CLI changes and their submitted patches. Failed attempts remain results.',
        'attempts': {row['id']: {'status': 'unstarted', 'outcome': None} for row in plan['schedule']['attempts']}}
    try:
        with tempfile.TemporaryDirectory(prefix='fr-m1-live-grade-') as temporary:
            root = Path(temporary)
            prepared = {}
            for task in local.tasks.TASKS:
                original = root / task
                record = local.tasks.prepare(task, original, binary, root / (task + '-preparation'))
                require(record['original'] == plan['seeds'][task]['receipt']['original'],
                        'private original differs from the frozen local task: ' + task)
                prepared[task] = (original, record)
            for row in plan['schedule']['attempts']:
                identity, task = row['id'], row['task']
                retained = data / 'attempts' / identity
                if not retained.exists():
                    continue
                result = json.loads((retained / 'result.json').read_text())
                require(all(result[key] == expected for key, expected in
                    [('id', identity), ('profile', row['profile']), ('task', task), ('setup', row['setup']),
                     ('plan_sha256', plan['sha256'])]), 'retained attempt identity differs')
                require(result['status'] in {'completed', 'failed'}, 'retain interrupted work before requesting grading')
                archive = retained / 'candidate.tar.gz'
                require(archive.is_file(), 'started attempt has no retained source archive: ' + identity)
                candidate = root / identity
                local.unpack(archive, {'archive_sha256': result['candidate_archive_sha256'],
                                      'original': result['final_source']}, candidate)
                patch = retained / 'change.patch' if result['patch_delivered'] else None
                if patch is not None:
                    require(local.hash_file(patch) == result['patch_sha256'], 'agent patch changed after collection')
                original, record = prepared[task]
                verdict = grading.grade(image, task, candidate, original, record, binary, rust, patch, output / identity)
                accepted = result['status'] == 'completed' and verdict['passed']
                summary['attempts'][identity] = {'status': 'checked', 'outcome': 'passed' if accepted else 'failed',
                    'native_status': result['status'], 'source_and_patch_accepted': verdict['passed'],
                    'actual_cost': result['actual_cost'], 'usage': result.get('usage'),
                    'fr_contribution': result.get('fr_contribution')}
            summary['checked_attempts'] = sum(row['status'] == 'checked' for row in summary['attempts'].values())
            summary['accepted_attempts'] = sum(row['outcome'] == 'passed' for row in summary['attempts'].values())
            summary['all_36_checked'] = summary['checked_attempts'] == 36
            require(completion != 'complete' or summary['all_36_checked'], 'M1 still has unstarted or unchecked attempts')
            summary['passed'] = True
            return summary
    except Exception as error:
        summary['harness_error'] = type(error).__name__ + ': ' + str(error)
        raise
    finally:
        local.tasks.save(output / 'summary.json', summary)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--data', type=Path, required=True)
    parser.add_argument('--fr', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--completion', choices=('partial', 'complete'), required=True)
    args = parser.parse_args()
    print(json.dumps(run(args.data.resolve(), args.fr.resolve(), args.output.resolve(), args.completion)))
