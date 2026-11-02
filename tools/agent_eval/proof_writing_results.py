"""Reject bad proof patches in isolation and check every scheduled M3 submission."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile

from . import proof_writing_tasks as cases
from . import selected_grading as grading
from . import selected_local as local
from . import selected_local_grade as public
from . import selected_report as report
from . import selected_tasks as common
from .project_edits_results import DETAILS, encoded_summary
from .selected_plan import encoded, require

TASKS = {'access-rust': 'Access for staff or the owner', 'page-count': 'Safe page counts',
         'fits-window': 'A range fits within its limit'}
VARIANTS = ('unfinished', 'incomplete', 'reference', 'missing-patch', 'wrong-patch', 'changed-claim')


def private_controls(value):
    expected = {task + '-' + variant for task in TASKS for variant in VARIANTS}
    require(value['passed'] is True and value['live_model_calls'] == 0 and value['checked_controls'] == 18 and
            set(value['controls']) == expected and all(
                row == {'status': 'passed', 'accepted': identity.endswith('-reference')}
                for identity, row in value['controls'].items()), 'all eighteen isolated proof controls must pass')
    require(value['bindings'] == local.implementation_bindings('M3'), 'isolated checking code or fixed inputs changed')
    require(re.fullmatch(r'sha256:[0-9a-f]{64}', value['image']) and
            all(re.fullmatch(r'[0-9a-f]{64}', value[key]) for key in ('image_archive_sha256', 'fr_sha256')),
            'retain exact isolated-image, archive and fr identities')
    return value


def load_checker(data, artifact, run, policy):
    require(os.environ.get('GITHUB_ACTIONS') == 'true' and run.isdigit(), 'load an explicitly named hosted checker')
    require(policy in {'frozen', 'fresh'}, 'choose the initial frozen checker or an explicit fresh historical audit')
    summary = json.loads((data / 'summary.json').read_text())
    receipt = cases.validate_controls(summary['controls'])
    private = private_controls(json.loads((artifact / 'summary.json').read_text()))
    require(receipt['bindings'] == private['bindings'] and private['source_commit'] == receipt['source_commit'] and
            private['run'] == run, 'recheck the exact original proof-checking source and fixed task inputs')
    if policy == 'frozen':
        require(run == receipt['run'] and all(private[key] == value for key, value in receipt['private'].items()),
                'initial grading must use the exact checker frozen before model calls')
    repository = os.environ['GITHUB_REPOSITORY']
    metadata = json.loads(subprocess.check_output(['gh', 'api', f'repos/{repository}/actions/runs/{run}'],
                                                 text=True, timeout=30))
    require(metadata['head_repository']['full_name'] == repository, 'control receipt refers to another repository')
    if policy == 'frozen':
        require(metadata['head_sha'] == receipt['source_commit'], 'control receipt refers to another hosted source')
    pages = json.loads(subprocess.check_output(['gh', 'api', '--paginate', '--slurp',
                       f'repos/{repository}/actions/runs/{run}/jobs?per_page=100'], text=True, timeout=30))
    name = ('Require every proof, source and native-client control' if policy == 'frozen' else
            'Rebuild the original M3 checker and reject bad patches')
    producers = [job for page in pages for job in page['jobs']
                 if job['name'] == name or job['name'].endswith('/ ' + name)]
    require(len(producers) == 1 and producers[0]['conclusion'] == 'success', 'the selected hosted controls must pass')
    archive = artifact / 'grader-image.tar.gz'
    require(archive.is_file() and not archive.is_symlink() and archive.stat().st_size <= 512 * 1024**2 and
            common.digest(archive) == private['image_archive_sha256'], 'isolated checker archive differs from its frozen receipt')
    subprocess.run(grading.DOCKER + ['load', '--input', str(archive)], check=True, timeout=60)
    actual = subprocess.check_output(grading.DOCKER + ['image', 'inspect', '--format', '{{.Id}}', private['image']],
                                     text=True, timeout=10).strip()
    require(actual == private['image'], 'loaded isolated checker differs from its immutable image identity')
    return {'image': actual, 'live_model_calls': 0, 'control_policy': policy}


def original(inputs, task, destination):
    source = inputs / ('m3-prepared-' + task)
    prepared = json.loads((source / 'project.json').read_text())
    receipt = json.loads((source / 'seed.json').read_text())
    local.unpack(source / 'original.tar.gz', receipt, destination)
    require(prepared['task'] == task and common.sources(destination) == prepared['original'],
            'hosted original differs from the fixed task')
    return prepared


def prepare_audit(binary, output):
    require(os.environ.get('GITHUB_ACTIONS') == 'true', 'prepare original-source audit inputs on GitHub')
    from . import proof_writing_controls as controller
    output.mkdir(parents=True, exist_ok=False)
    for task in TASKS:
        project, inputs = output / 'working' / task, output / ('m3-prepared-' + task)
        prepared = cases.prepare(task, binary, project, inputs)
        controller.exercise(task, binary, project, prepared,
                            output / ('m3-proof-controls-' + task) / 'controls', inputs)


def controls(inputs, binary, output):
    require(os.environ.get('GITHUB_ACTIONS') == 'true', 'check private grading on GitHub')
    output.mkdir(parents=True, exist_ok=False)
    result = {'passed': False, 'live_model_calls': 0, 'image': None, 'checked_controls': 0, 'controls': {},
              'source_commit': local.identity()['commit'], 'run': os.environ['GITHUB_RUN_ID'],
              'bindings': local.implementation_bindings('M3'), 'fr_sha256': common.digest(binary)}
    try:
        rust = Path(subprocess.check_output(['rustc', '--print', 'sysroot'], text=True, timeout=10).strip())
        image = grading.build(output / 'image')
        result['image'] = image
        with tempfile.TemporaryDirectory(prefix='fr-m3-private-controls-') as temporary:
            root = Path(temporary)
            for task in TASKS:
                project = root / task
                prepared = original(inputs, task, project)
                accepted = (inputs / ('m3-proof-controls-' + task) / 'controls/reference.patch').read_bytes()
                require(accepted, 'controller proof patch is missing')
                for variant in VARIANTS:
                    patch = root / (task + '-' + variant + '.patch')
                    if variant in {'reference', 'missing-patch'}:
                        patch.write_bytes(accepted)
                    elif variant == 'wrong-patch':
                        patch.write_bytes(b'not a Git patch\n')
                    else:
                        candidate = root / (task + '-' + variant)
                        grading.stage(project, candidate)
                        common.comparison_grade.BASE.initialize(candidate)
                        model = candidate / prepared['spec']
                        text = model.read_text()
                        if variant == 'changed-claim':
                            lines = text.splitlines(keepends=True)
                            index = next(index for index, line in enumerate(lines)
                                         if line.startswith('theorem ' + prepared['obligation'] + ' '))
                            lines[index] = 'theorem ' + prepared['obligation'] + ' : True := by\n'
                            text = ''.join(lines)
                            begin = text.index('\n', text.index('-- fr:proof-begin ' + prepared['obligation'])) + 1
                            end = text.rfind('\n', 0, text.index('-- fr:proof-end ' + prepared['obligation'])) + 1
                            text = text[:begin] + '  trivial\n' + text[end:]
                        else:
                            require(text.count('  sorry\n') == 1, 'negative proof control needs its one fixed gap')
                            text = text.replace('  sorry\n', '  sorry -- still unfinished\n' if variant == 'unfinished'
                                                else '  exact false\n', 1)
                        model.write_text(text)
                        patch.write_bytes(common.comparison_grade.BASE.git(candidate, 'diff', '--binary', 'HEAD').stdout)
                    identity = task + '-' + variant
                    verdict = grading.grade(image, task, project, prepared, binary, rust,
                        None if variant == 'missing-patch' else patch, output / identity, milestone='M3', tools={})
                    require(verdict['passed'] == (variant == 'reference'), 'incorrect isolated proof-control outcome: ' + identity)
                    stage = verdict['verdict']['stage']
                    if variant in {'unfinished', 'incomplete', 'changed-claim'}:
                        require(stage == 'source checks' and verdict['verdict']['changed']['passed'] is False,
                                'negative proof did not reach the independent checker: ' + identity)
                    if variant in {'missing-patch', 'wrong-patch'}:
                        require(stage == 'delivery', 'bad delivery did not fail at patch inspection')
                    result['controls'][identity] = {'status': 'passed', 'accepted': verdict['passed']}
        archive = output / 'grader-image.tar'
        subprocess.run(grading.DOCKER + ['save', '--output', str(archive), image], check=True, timeout=60)
        subprocess.run(['gzip', '-1', str(archive)], check=True, timeout=60)
        result.update(passed=True, checked_controls=len(result['controls']),
                      image_archive_sha256=common.digest(output / 'grader-image.tar.gz'))
        return result
    finally:
        common.save(output / 'summary.json', result)


def grade_cell(data, inputs, artifact, image, rust, binary, profile, task, policy, output):
    require(os.environ.get('GITHUB_ACTIONS') == 'true', 'grade public proof patches on GitHub')
    require(profile in report.AGENTS and task in TASKS, 'select one explicit M3 profile/task cell')
    output.mkdir(parents=True, exist_ok=False)
    supplied = json.loads((data / 'summary.json').read_text())
    rows = public.verify(supplied, 'M3')
    cases.validate_controls(supplied['controls'])
    require(policy in {'frozen', 'fresh'}, 'choose the exact initial checker or a fresh original-source audit')
    checked = private_controls(json.loads((artifact / 'summary.json').read_text()))
    require(checked['image'] == image and checked['source_commit'] == supplied['controls']['source_commit'] and
            common.digest(binary) == checked['fr_sha256'], 'use the same original fr source and checked isolated image')
    if policy == 'frozen':
        require(checked['run'] == supplied['controls']['run'] and image == supplied['controls']['private']['image'],
                'initial grading must use the frozen controls')
    selected = [row for row in rows if row['profile'] == profile and row['task'] == task]
    require(len(selected) == 4, 'check both setups and repetitions')
    result = {'passed': False, 'study': 'M3', 'profile': profile, 'task': task, 'live_model_calls': 0,
              'summary_sha256': common.digest(data / 'summary.json'), 'image': image, 'control_policy': policy,
              'control_run': checked['run'], 'control_source_commit': checked['source_commit'],
              'core_checker_sha256': supplied['core_checker_sha256'], 'attempts': {}}
    try:
        with tempfile.TemporaryDirectory(prefix='fr-m3-grade-') as temporary:
            project = Path(temporary) / 'original'
            prepared = original(inputs, task, project)
            require(hashlib.sha256(encoded(prepared['original'])).hexdigest() == supplied['task_source_sha256'][task],
                    'independent original differs from the source supplied to the agents')
            for row in selected:
                require(row['native_status'] in {'completed', 'failed'}, 'retain every finished or failed native attempt')
                patch = data / row['patch_file'] if row['patch_file'] is not None else None
                if patch is not None:
                    require(patch.is_file() and not patch.is_symlink() and common.digest(patch) == row['patch_sha256'],
                            'check the exact delivered patch')
                verdict = grading.grade(image, task, project, prepared, binary, rust, patch,
                                        output / row['id'], milestone='M3', tools={})
                accepted = row['native_status'] == 'completed' and verdict['passed']
                result['attempts'][row['id']] = {'status': 'checked', 'outcome': 'passed' if accepted else 'failed',
                    'native_status': row['native_status'], 'patch_accepted': verdict['passed'],
                    'check_stage': verdict['verdict']['stage'], 'undo_exact': verdict['verdict']['undo_exact'],
                    'redo_exact': verdict['verdict']['redo_exact'], 'patch_sha256': row['patch_sha256'], 'actual_cost': None}
        result['passed'] = True
        return result
    finally:
        common.save(output / 'summary.json', result)


def merge(supplied, cells):
    rows = public.verify(supplied, 'M3')
    require(len(cells) == 9 and {(cell['profile'], cell['task']) for cell in cells} ==
            {(profile, task) for profile in report.AGENTS for task in TASKS}, 'require all nine distinct agent/task cells')
    attempts = {}
    environments = {(cell['image'], cell['control_policy'], cell['control_run'], cell['control_source_commit']) for cell in cells}
    require(len(environments) == 1, 'all proof cells must use the same checked original-source environment')
    image, policy, control_run, control_source = next(iter(environments))
    require(policy in {'frozen', 'fresh'} and control_source == supplied['controls']['source_commit'],
            'choose an explicit audit of the original proof-checking source')
    if policy == 'frozen':
        require(image == supplied['controls']['private']['image'] and control_run == supplied['controls']['run'],
                'initial checking environment differs from the frozen controls')
    for cell in cells:
        require(cell['passed'] is True and cell['study'] == 'M3' and cell['live_model_calls'] == 0 and
                cell['core_checker_sha256'] == supplied['core_checker_sha256'] and
                cell['summary_sha256'] == hashlib.sha256(encoded_summary(supplied)).hexdigest(),
                'independent checker, native records or summary differs')
        expected = {row['id'] for row in rows if row['profile'] == cell['profile'] and row['task'] == cell['task']}
        require(set(cell['attempts']) == expected and not attempts.keys() & cell['attempts'].keys(),
                'independent cell repeats or omits an attempt')
        attempts.update(cell['attempts'])
    for row in rows:
        checked = attempts[row['id']]
        require(checked['status'] == 'checked' and checked['native_status'] == row['native_status'] and
                checked['patch_sha256'] == row['patch_sha256'] and
                checked['outcome'] == ('passed' if row['native_status'] == 'completed' and checked['patch_accepted'] else 'failed'),
                'a failed or timed-out native attempt cannot become a successful result')
    return {'passed': True, 'all_36_checked': True, 'checked_attempts': len(attempts), 'live_model_calls': 0,
            'core_checker_sha256': supplied['core_checker_sha256'], 'attempts': attempts,
            'image': image, 'control_policy': policy, 'control_run': control_run, 'control_source_commit': control_source}


def completion(summary):
    missing = [{'profile': profile, 'task': task} for profile in report.AGENTS for task in TASKS
        if not any(row['profile'] == profile and row['task'] == task and row['setup'] == 'fr' and
                   row['native_status'] == 'completed' and row['independent_outcome'] == 'passed' and
                   row['fr_use']['contributing_actions'] for row in summary['attempts'])]
    return {'complete': not missing, 'missing_accepted_fr_cases': missing}


def markdown(summary):
    groups, matched = report.comparisons(summary, TASKS)
    accepted = sum(row['independent_outcome'] == 'passed' for row in summary['attempts'])
    missing = completion(summary)['missing_accepted_fr_cases']
    progress = ['Still missing accepted proofs using fr:', '', *[
        f'- {report.AGENTS[row["profile"]]}: {TASKS[row["task"]]}.' for row in missing], ''] if missing else [
        'Every selected agent has an accepted proof using fr for each task.', '']
    lines = ['# M3: writing three fixed Lean proofs', '',
        f'All 36 scheduled local CLI attempts were independently checked: {accepted} passed and {36 - accepted} failed.',
        '', *progress,
        'Each app and model used ordinary tools, then ordinary tools plus fr, twice per task. The source, generated model,',
        'theorem and limits were the same. Agents supplied only proof steps. Controller reference proofs were not supplied.', '',
        '- Access: the model allows staff or the owner.',
        '- Pages: invalid inputs return -1; valid inputs give a count between zero and the item count; 11 items at 5 per page give 3.',
        '- Ranges: a range fits exactly when its offset plus length is at most the total.', '',
        'Lean checks these model claims. The connection from the Rust implementation to its model remains unproved.',
        'Separate source execution controls test finite input cases, including integer boundaries. They are not a general equivalence proof.', '',
        '| Agent | Tools | Accepted / attempted | Mean successful time | Total time / success |',
        '| --- | --- | --- | --- | --- |']
    for group in groups:
        lines.append(f'| {report.AGENTS[group["profile"]]} | {group["setup"]} | {group["accepted"]} / {group["attempted"]} | '
                     f'{report.number(group["successful_mean_seconds"])} s | {report.number(group["seconds_per_success"])} s |')
    lines += ['', 'Time includes the complete local attempt: CLI startup, model replies, tool calls, checks and patch capture.',
              'Failed attempts stay in the totals. A valid patch delivered after the deadline still counts as a failed attempt.']
    by_id = {row['id']: row for row in summary['attempts']}
    for pause in summary['collection_notes']['system_sleep']:
        row = by_id[pause['attempt']]
        lines += ['', f'`{row["id"]}` includes {report.number(pause["suspended_seconds"])} seconds of detected system sleep '
                  f'in its {report.number(row["total_seconds"])}-second total.']
    lines += ['', '| Agent | Pairs accepted in both setups | Ordinary mean | fr mean | Change with fr |',
              '| --- | --- | --- | --- | --- |']
    for pair in matched:
        lines.append(f'| {report.AGENTS[pair["profile"]]} | {pair["pairs"]} | {report.number(pair["ordinary_mean_seconds"])} s | '
                     f'{report.number(pair["fr_mean_seconds"])} s | {report.number(pair["fr_change_percent"])}% |')
    lines += ['', 'Positive percentages mean fr took longer on matching successful attempts. This small comparison measures these',
              'apps, models and requests; it does not establish general speed or money savings. Account charges are unknown.',
              'Reported token categories retain each CLI’s meaning. Missing counters remain unknown.', '',
              '| Attempt | Result | Elapsed seconds | Reported tokens | fr contribution or failure |',
              '| --- | --- | --- | --- | --- |']
    for row in summary['attempts']:
        details = list(row['fr_use']['contributing_actions'])
        if row['failure_summary'] is not None:
            details.append(row['failure_summary'])
        explanation = '; '.join(details) or 'No fr contribution credited.'
        tokens = json.dumps(row['reported_tokens'], sort_keys=True) if row['reported_usage_complete'] else 'Unknown or incomplete'
        lines.append(f'| {row["id"]} | {row["independent_outcome"]} | {report.number(row["total_seconds"])} | '
                     f'{tokens.replace("|", "&#124;")} | {explanation.replace("|", "&#124;")} |')
    lines += ['', 'The published files contain submitted patches and this summary. Raw prompts, CLI transcripts, source archives',
              'and account data stay local. The summary records the selected model settings, tool versions, checker identity,',
              'source hashes, collection limits and approved axioms; failures and timeouts are retained.', '',
              'The [behavior review](../behavior-review.md) examines GLM and Kimi commands and timing.', '']
    return '\n'.join(lines)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='operation', required=True)
    loading = commands.add_parser('load-checker')
    for name in ('data', 'artifact', 'output'):
        loading.add_argument('--' + name, type=Path, required=True)
    loading.add_argument('--run', required=True)
    loading.add_argument('--policy', choices=('frozen', 'fresh'), required=True)
    preparing = commands.add_parser('prepare-audit')
    for name in ('fr', 'output'):
        preparing.add_argument('--' + name, type=Path, required=True)
    controlling = commands.add_parser('controls')
    for name in ('inputs', 'fr', 'output'):
        controlling.add_argument('--' + name, type=Path, required=True)
    checking = commands.add_parser('grade-cell')
    for name in ('data', 'inputs', 'artifact', 'fr', 'rust', 'output'):
        checking.add_argument('--' + name, type=Path, required=True)
    for name in ('image', 'profile', 'task'):
        checking.add_argument('--' + name, required=True)
    checking.add_argument('--policy', choices=('frozen', 'fresh'), required=True)
    merging = commands.add_parser('merge')
    for name in ('data', 'cells', 'output'):
        merging.add_argument('--' + name, type=Path, required=True)
    rendering = commands.add_parser('report')
    for name in ('data', 'checks'):
        rendering.add_argument('--' + name, type=Path, required=True)
    rendering.add_argument('--mode', choices=('write', 'check'), required=True)
    args = parser.parse_args()
    if args.operation == 'load-checker':
        common.save(args.output, load_checker(args.data.resolve(), args.artifact.resolve(), args.run, args.policy))
    elif args.operation == 'prepare-audit':
        prepare_audit(args.fr.resolve(), args.output.resolve())
    elif args.operation == 'controls':
        result = controls(args.inputs.resolve(), args.fr.resolve(), args.output.resolve())
        print(json.dumps({key: result[key] for key in ('passed', 'checked_controls', 'live_model_calls')}))
    elif args.operation == 'grade-cell':
        result = grade_cell(args.data.resolve(), args.inputs.resolve(), args.artifact.resolve(), args.image, args.rust.resolve(),
                            args.fr.resolve(), args.profile, args.task, args.policy, args.output.resolve())
        print(json.dumps({'passed': result['passed'], 'checked': len(result['attempts'])}))
    elif args.operation == 'merge':
        supplied = json.loads((args.data / 'summary.json').read_text())
        common.save(args.output, merge(supplied, [json.loads(path.read_text()) for path in args.cells.glob('*/summary.json')]))
    else:
        supplied = json.loads((args.data / 'summary.json').read_text())
        report.verify_checks(supplied, json.loads(args.checks.read_text()), 36)
        path, content = args.data / 'README.md', markdown(supplied)
        if args.mode == 'write':
            path.write_text(content)
        else:
            require(path.read_text() == content, 'regenerate the report from the independently checked results')
            require(completion(supplied)['complete'], 'M3 still lacks accepted contributing fr proofs for an agent/task pair')
