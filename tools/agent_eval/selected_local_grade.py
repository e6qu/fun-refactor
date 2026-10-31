"""Check the approved public patches and summary in private GitHub containers."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile

from . import selected_grading as grading
from . import selected_local as local
from .selected_plan import CASE_DIRECTORIES, PROFILE_PATHS, ROOT, TASK_PATHS, encoded, prepare, require

CORE_FILES = (
    'tools/agent_eval/selected_tasks.py', 'tools/agent_eval/comparison_grade.py',
    'tools/agent_eval/oracle.py', 'tools/agent-eval.py',
    'tools/agent_eval/regex_workspace.py', 'tools/agent_eval/regex_escape_len.py',
    'tools/agent_eval/investigation_rehearsal.py', 'tools/agent_eval/investigation.py',
    'tools/agent_eval/investigation_run.py', 'tools/agent_eval/investigation_lock.py',
    'tools/project_change_sources.py', 'tools/evidence_basis.py',
    'tests/agent-eval/project-changes/oracle.py', 'tests/agent-eval/project-changes/tasks.json',
    'tests/agent-eval/unknown-target/task.json',
    '.github/actions/lean-toolchain/action.yml', '.github/actions/native-tools/python-wheels.txt',
)


def core_digest(milestone):
    require(milestone in {'M1', 'M2', 'M3', 'M3-feedback', 'M3-construction'}, 'choose an explicit checker study')
    paths = [ROOT / name for name in CORE_FILES] + list((ROOT / 'sdk/python/src').rglob('*.py'))
    if milestone == 'M2':
        paths += [ROOT / 'tools/agent_eval' / name for name in (
            'project_edits_tasks.py', 'selected_grade_probe.py', 'selected_grading.py',
            'selected_local_grade.py', 'project_edits_results.py', 'container_resources.py',
            'bounded_host.py', 'selected_plan.py', 'isolated_grade.py', 'selected_native.py')]
        directory = ROOT / CASE_DIRECTORIES[milestone]
        paths += [directory / 'tasks.json', directory / 'checkers.json']
        paths += [path for path in (directory / 'fixtures').rglob('*') if path.is_file()]
        paths += [ROOT / 'tests/agent-eval/selected-agents/Grader.Dockerfile',
                  ROOT / 'tests/agent-eval/regex/workspace.tar.gz',
                  ROOT / 'tests/agent-eval/regex/Cargo.lock']
    elif milestone in {'M3', 'M3-feedback', 'M3-construction'}:
        paths += [ROOT / 'tools/agent_eval' / name for name in (
            'proof_writing_tasks.py', 'proof_writing_controls.py', 'proof_writing_runtime.py',
            'proof_writing_collect.py', 'proof_writing_results.py', 'selected_grade_probe.py',
            'selected_grading.py', 'selected_local_grade.py', 'container_resources.py',
            'bounded_host.py', 'selected_plan.py', 'isolated_grade.py', 'selected_native.py')]
        directory = ROOT / CASE_DIRECTORIES['M3']
        paths += [directory / name for name in ('tasks.json', 'checkers.json', 'cases.json', 'profiles.json')]
        paths += [path for path in (directory / 'fixtures').rglob('*') if path.is_file()]
        paths += list((directory / 'controls').glob('*.lean'))
        paths += [ROOT / 'tests/agent-eval/selected-agents/Grader.Dockerfile']
        if milestone in {'M3-feedback', 'M3-construction'}:
            paths += [ROOT / 'tools/agent_eval/proof_feedback.py']
            paths += [ROOT / '.github/workflows/proof-feedback.yml']
            paths += [ROOT / '.github/workflows/refinement-evidence.yml']
            feedback = ROOT / CASE_DIRECTORIES[milestone]
            paths += [ROOT / TASK_PATHS[milestone], feedback / 'checkers.json', ROOT / PROFILE_PATHS[milestone]]
            paths += [path for path in (feedback / 'fixtures').rglob('*') if path.is_file()]
            paths += list((feedback / 'controls').glob('*.lean'))
    bindings = {str(path.relative_to(ROOT)): local.hash_file(path) for path in sorted(paths)}
    return hashlib.sha256(encoded(bindings)).hexdigest()


def verify(summary, milestone):
    expected = prepare(ROOT, milestone)
    require(summary['study'] == milestone and summary['profiles'] == expected['profiles'], 'public model settings differ')
    require(summary['task_bindings'] == expected['bindings'], 'public task requirements differ')
    require(summary['core_checker_sha256'] == core_digest(milestone), 'independent task checks changed after collection')
    require(summary['native_inputs'] == local.identity()['native_inputs'], 'fr source differs from the measured source')
    if milestone in {'M2', 'M3', 'M3-feedback', 'M3-construction'}:
        require(summary['limits'] == local.LIMITS and summary['provider_request_cap_enforced'] is False and
                summary['token_cap_enforced'] is False, 'retain the actual limits and unavailable token/request caps')
    rows = summary['attempts']
    require(len(rows) == len(expected['attempts']), 'public summary must retain all scheduled slots')
    for supplied, planned in zip(rows, expected['attempts']):
        require(all(supplied[key] == planned[key] for key in ('id', 'profile', 'task', 'setup', 'repetition')),
                'public attempt identity or order differs')
        require(supplied['native_status'] in {'unstarted', 'completed', 'failed'}, 'finish recording interrupted work')
        if supplied['native_status'] != 'unstarted':
            require(supplied['actual_cost'] is None, 'no observed account charges were collected by this CLI study')
            require(supplied['patch_file'] in {None, 'patches/' + supplied['id'] + '.patch'}, 'unsafe public patch path')
            if milestone in {'M2', 'M3', 'M3-feedback', 'M3-construction'} and supplied['native_status'] == 'completed':
                require(0 <= supplied['total_seconds'] <= local.LIMITS['wall_seconds'] and
                        supplied['stop_reason'] is None and not supplied['complete_attempt_deadline_missed'],
                        'a stopped or late native attempt must remain failed')
    return rows


def run(data, binary, output, completion):
    require(os.environ.get('GITHUB_ACTIONS') == 'true', 'independently grade public patches on GitHub')
    require(completion in {'partial', 'complete'}, 'choose whether all 36 checked attempts are required')
    output.mkdir(parents=True, exist_ok=False)
    supplied = json.loads((data / 'summary.json').read_text())
    rows = verify(supplied, 'M1')
    rust = Path(subprocess.check_output(['rustc', '--print', 'sysroot'], timeout=10).decode().strip())
    image = grading.build(output / 'image')
    summary = {'passed': False, 'core_checker_sha256': supplied['core_checker_sha256'], 'live_model_calls': 0,
        'scope': 'Independent checks of the submitted public patches. CLI metrics are locally collected summaries; raw records stay local.',
        'attempts': {row['id']: {'status': 'unstarted', 'outcome': None} for row in rows}}
    try:
        with tempfile.TemporaryDirectory(prefix='fr-m1-patch-grade-') as temporary:
            root = Path(temporary)
            prepared = {}
            for task in local.tasks.TASKS:
                original = root / task
                record = local.tasks.prepare(task, original, binary, root / (task + '-preparation'))
                require(hashlib.sha256(encoded(record['original'])).hexdigest() == supplied['task_source_sha256'][task],
                        'private original differs from the task supplied to the agents: ' + task)
                prepared[task] = (original, record)
            for row in rows:
                if row['native_status'] == 'unstarted':
                    continue
                identity, task = row['id'], row['task']
                patch = data / row['patch_file'] if row['patch_file'] is not None else None
                if patch is not None:
                    require(patch.is_file() and not patch.is_symlink() and local.hash_file(patch) == row['patch_sha256'],
                            'submitted patch differs from the locally retained patch')
                original, record = prepared[task]
                verdict = grading.grade(image, task, original, record, binary, rust, patch, output / identity,
                                        milestone='M1', tools={})
                accepted = row['native_status'] == 'completed' and verdict['passed']
                summary['attempts'][identity] = {'status': 'checked', 'outcome': 'passed' if accepted else 'failed',
                    'native_status': row['native_status'], 'patch_accepted': verdict['passed'],
                    'check_stage': verdict['verdict']['stage'], 'undo_exact': verdict['verdict']['undo_exact'],
                    'redo_exact': verdict['verdict']['redo_exact'], 'actual_cost': row['actual_cost']}
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
