"""Grade original algorithm patches and apply decisions fixed before model calls."""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import subprocess
import tempfile

from . import algorithm_guidance as guidance
from . import proof_algorithms as algorithms
from . import selected_grading as grading
from . import selected_local as local
from . import selected_local_grade as public
from . import selected_tasks as common
from .selected_plan import ALGORITHM_STUDIES, ROOT, encoded, prepare, require

CHECKS = {'termination_check_passed', 'termination_saved', 'correctness_check_passed',
          'correctness_saved', 'package_check_passed'}
TIMINGS = {'first_check_seconds', 'termination_accepted_seconds', 'correctness_accepted_seconds',
           'package_check_seconds', 'patch_delivery_seconds'}


def validate_observations(value, seconds):
    require(set(value) == CHECKS | TIMINGS | {'execution_validity'}, 'review both proofs, delivery and validity')
    require(all(value[name] is None or type(value[name]) is bool for name in CHECKS), 'checks need observed pass/fail or unknown')
    require(all(value[name] is None or (type(value[name]) in {int, float} and math.isfinite(value[name])
                and 0 <= value[name] <= seconds) for name in TIMINGS), 'keep observed times within the attempt')
    require(value['execution_validity'] in {'valid', 'service-failed', 'collection-failed', 'unknown'},
            'review execution validity without replacing service failures')
    return value


def complete(row, patch_accepted):
    observation = validate_observations(row['algorithm_observations'], row['total_seconds'])
    resources = row['complete_attempt_resources']
    return (patch_accepted and row['native_status'] == 'completed' and row['stop_reason'] is None and
            resources['stop_reason'] is None and resources['exit_code'] == 0 and
            resources['sampled_aggregate_rss_bytes'] <= 805306368 and resources['sampled_cpu_seconds'] <= 75 and
            not row['complete_attempt_deadline_missed'] and row['total_seconds'] <= 150 and
            not row['checking_input_changes'] and not row['collection_error_recorded'] and
            observation['execution_validity'] == 'valid' and all(observation[name] is True for name in CHECKS))


def decisions(summary):
    plan = prepare(ROOT, summary['study'])
    first, second = plan['requirements']['setups']
    rows = summary['attempts']
    require([row['id'] for row in rows] == [row['id'] for row in plan['attempts']], 'retain all original slots in order')
    result = {}
    for profile in plan['profiles']:
        selected = [row for row in rows if row['profile'] == profile]
        require(all(row['independent_outcome'] in {'passed', 'failed'} for row in selected), 'grade every attempt before deciding')
        counts = {condition: sum(row['independent_outcome'] == 'passed' for row in selected
                               if row['setup'] == condition) for condition in (first, second)}
        for row in selected:
            if row['independent_outcome'] == 'passed':
                require(complete(row, row['independent_check']['patch_accepted']), 'late or incomplete work cannot become success')
        key = f'{counts[second]},{counts[first]}'
        usable = all(row['algorithm_observations']['execution_validity'] == 'valid' and
                     not row['collection_error_recorded'] and row['suspended_seconds'] < 1 for row in selected)
        decision = (plan['requirements']['decisions'].get(key, plan['requirements']['decisions']['mixed'])
                    if usable else
                    'Execution is invalid or unverified in at least one attempt. '
                    'Review its recorded cause before drawing conclusions about guidance or agent ability. '
                    'Preserve every original outcome; do not replace failed slots.')
        result[profile] = {'accepted': counts, 'decision': decision, 'execution_usable': usable,
            'total_seconds_including_failures': sum(row['total_seconds'] for row in selected),
            'transfer_candidate': usable and counts[second] == 2 and counts[first] == 0}
    return result


def grade(data, artifacts, binary, output, control_run, study):
    require(os.environ.get('GITHUB_ACTIONS') == 'true' and study in ALGORITHM_STUDIES,
            'independently grade the chosen algorithm study on GitHub')
    output.mkdir(parents=True, exist_ok=False)
    summary = json.loads((data / 'summary.json').read_text())
    rows = public.verify(summary, study)
    receipt = algorithms.validate_controls(json.loads((artifacts / 'controls.json').read_text()), study)
    require(receipt['run'] == control_run and receipt == summary['controls'] and common.digest(binary) == receipt['fr_sha256'],
            'use the exact qualified checker and native tool frozen before calls')
    archive = artifacts / 'grader-image.tar.gz'
    require(common.digest(archive) == receipt['private_image_archive_sha256'], 'private checker archive changed')
    subprocess.run(grading.DOCKER + ['load', '--input', str(archive)], check=True, timeout=60)
    image = receipt['private_image']
    require(subprocess.check_output(grading.DOCKER + ['image', 'inspect', '--format', '{{.Id}}', image],
                                    text=True, timeout=10).strip() == image, 'the restored private checker differs')
    inputs = artifacts / 'inputs/euclid'
    prepared = json.loads((inputs / 'project.json').read_text())
    seed = json.loads((inputs / 'seed.json').read_text())
    require(hashlib.sha256(encoded(seed['original'])).hexdigest() == summary['task_source_sha256']['euclid'],
            'grade the original task source, never a later revised fixture')
    rust = Path(subprocess.check_output(['rustc', '--print', 'sysroot'], text=True).strip())
    count = len(prepare(ROOT, study)['attempts'])
    result = {'passed': False, f'all_{count}_checked': False, 'checked_attempts': 0, 'live_model_calls': 0,
              'core_checker_sha256': summary['core_checker_sha256'], 'control_policy': 'frozen',
              'image': image, 'attempts': {}}
    try:
        with tempfile.TemporaryDirectory(prefix='algorithm-independent-') as temporary:
            original = Path(temporary) / 'original'
            local.unpack(inputs / 'original.tar.gz', seed, original)
            for row in rows:
                require(row['native_status'] in {'completed', 'failed'}, 'finish recording every original session')
                patch = data / row['patch_file'] if row['patch_file'] is not None else None
                if patch is not None:
                    require(patch.is_file() and not patch.is_symlink() and common.digest(patch) == row['patch_sha256'],
                            'the submitted patch changed after review')
                verdict = grading.grade(image, 'euclid', original, prepared, binary, rust, patch,
                                        output / row['id'], milestone=study, tools={})
                accepted = complete(row, verdict['passed'])
                result['attempts'][row['id']] = {'status': 'checked', 'native_status': row['native_status'],
                    'patch_sha256': row['patch_sha256'], 'patch_accepted': verdict['passed'],
                    'outcome': 'passed' if accepted else 'failed', 'check_stage': verdict['verdict']['stage'],
                    'undo_exact': verdict['verdict']['undo_exact'], 'redo_exact': verdict['verdict']['redo_exact']}
            require(len(rows) == count, 'grade every original slot')
            result.update(passed=True, checked_attempts=count)
            result[f'all_{count}_checked'] = True
    finally:
        common.save(output / 'checked.json', result)
    return result


def markdown(summary):
    result = decisions(summary)
    first, second = prepare(ROOT, summary['study'])['requirements']['setups']
    lines = ['# Can guidance help the agents prove an executable algorithm?', '', summary['comparison'], '',
             'The fixed Lean function must terminate and return the correct GCD for every pair of Nat inputs.',
             'Each complete attempt has 150 seconds including tools, checks, patch delivery and session accounting.',
             'These small development comparisons do not close M3 or measure fr overhead against ordinary tools.',
             'Two repetitions cannot establish a general success rate. Keep every failure in the total work.', '',
             f'| Agent | {first}: accepted / 2 | {second}: accepted / 2 | Total seconds, including failures |',
             '| --- | ---: | ---: | ---: |']
    for profile, row in result.items():
        lines.append(f'| {profile} | {row["accepted"][first]} | {row["accepted"][second]} | '
                     f'{row["total_seconds_including_failures"]:.3f} |')
    lines.append('')
    for profile, row in result.items():
        lines.append(f'{profile}: {row["decision"]}')
    lines += ['', 'Guidance size in UTF-8 bytes (not model tokens):', '', '| Condition | Declarations | Plan |',
              '| --- | ---: | ---: |']
    for condition, files in summary['guidance_bytes'].items():
        lines.append(f'| {condition} | {files[".fr/declarations.txt"]} | {files[".fr/proof-plan.txt"]} |')
    lines += ['', 'Only reviewed summaries and submitted patches are public. Raw local records remain private.', '',
              '| Attempt | Complete outcome | Seconds | Failure or missing result | Patch |',
              '| --- | --- | ---: | --- | --- |']
    for row in summary['attempts']:
        patch = '[patch](' + row['patch_file'] + ')' if row['patch_file'] else 'none'
        explanation = (row['failure_summary'] or '').replace('|', '\\|')
        lines.append(f'| {row["id"]} | {row["independent_outcome"]} | {row["total_seconds"]:.3f} | {explanation} | {patch} |')
    return '\n'.join(lines) + '\n'


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    operations = parser.add_subparsers(dest='operation', required=True)
    grading_parser = operations.add_parser('grade')
    for name in ('data', 'artifacts', 'fr', 'output'):
        grading_parser.add_argument('--' + name, type=Path, required=True)
    grading_parser.add_argument('--control-run', required=True)
    grading_parser.add_argument('--study', choices=tuple(sorted(ALGORITHM_STUDIES)), required=True)
    report_parser = operations.add_parser('report')
    report_parser.add_argument('--data', type=Path, required=True)
    arguments = parser.parse_args()
    if arguments.operation == 'grade':
        print(json.dumps(grade(arguments.data.resolve(), arguments.artifacts.resolve(), arguments.fr.resolve(),
                               arguments.output.resolve(), arguments.control_run, arguments.study)))
    else:
        summary = json.loads((arguments.data / 'summary.json').read_text())
        (arguments.data / 'README.md').write_text(markdown(summary))
