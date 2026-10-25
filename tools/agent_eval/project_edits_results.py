"""Independently check all 48 public M2 submissions and explain their outcomes."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tarfile

from . import project_edits_tasks as cases
from . import selected_grading as grading
from . import selected_local_grade as public
from . import selected_report as report
from . import selected_tasks as common
from .selected_plan import encoded, require

TASKS = {'rust-parameter': 'Remove an unused Rust parameter',
         'typescript-alias': 'Rename a TypeScript export',
         'regex-escape-len': 'Add the escaped-text byte count',
         'python-invoice': 'Include the invoice fee'}
DETAILS = ('patch_accepted', 'check_stage', 'undo_exact', 'redo_exact')


def unpack_cache(archive, destination, expected, prefix, maximum):
    require(common.digest(archive) == expected, 'compiler archive differs from the successful controls')
    require(not (destination / prefix).exists(), 'extract the compiler archive once into a fresh destination')
    with tarfile.open(archive) as stream:
        members = stream.getmembers()
        require(0 < len(members) <= 50000 and sum(member.size for member in members) <= maximum,
                'compiler archive exceeds its file or expanded-byte limit')
        names = set()
        for member in members:
            path = Path(member.name)
            require(not path.is_absolute() and '..' not in path.parts and path.parts[0] == prefix and
                    (member.isfile() or member.isdir()) and member.name not in names,
                    'unsafe or repeated compiler archive entry')
            names.add(member.name)
        stream.extractall(destination, members=members, filter='data')


def controls(artifact, image):
    summary = json.loads((artifact / 'summary.json').read_text())
    identities = {task + '-' + variant for task in TASKS for variant in
                  ('unchanged', 'incomplete', 'reference', 'missing-patch', 'wrong-patch')}
    require(summary['passed'] is True and summary['image'] == image and summary['live_model_calls'] == 0 and
            set(summary['controls']) == identities and
            all(row['status'] == 'passed' for row in summary['controls'].values()),
            'need all twenty successful private controls before grading agent patches')
    cache = artifact / 'regex-target-cache'
    receipt = json.loads((cache / 'cache.json').read_text())
    require(receipt['image'] == image and receipt['removed_workspace_packages'] == ['regex', 'regex-syntax'],
            'compiler cache must rebuild both packages under change')
    unpack_cache(cache / 'cache.tar.gz', cache, receipt['cache_archive_sha256'], 'target', 512 * 1024**2)
    unpack_cache(cache / 'registry.tar.gz', cache, receipt['registry_archive_sha256'], 'registry', 512 * 1024**2)
    actual = subprocess.check_output(grading.DOCKER + ['image', 'inspect', '--format', '{{.Id}}', image],
                                     text=True, timeout=10).strip()
    require(actual == image, 'load the exact successful private grader image')
    return cache, receipt


def grade_cell(data, inputs, artifact, image, rust, node, typescript, profile, task, policy, output):
    require(os.environ.get('GITHUB_ACTIONS') == 'true', 'check public patches on GitHub runners')
    require(profile in report.AGENTS and task in TASKS, 'select one explicit profile/task cell')
    output.mkdir(parents=True, exist_ok=False)
    supplied = json.loads((data / 'summary.json').read_text())
    rows = public.verify(supplied, 'M2')
    cache, receipt = controls(artifact, image)
    require(policy in {'frozen', 'fresh'}, 'choose the original frozen controls or a newly checked replay explicitly')
    if policy == 'frozen':
        require(supplied['controls']['private']['image'] == image and
                supplied['controls']['private']['cache_archive_sha256'] == receipt['cache_archive_sha256'] and
                supplied['controls']['private']['registry_archive_sha256'] == receipt['registry_archive_sha256'],
                'initial grading must use the independent compiler inputs frozen before live attempts')
    original = inputs / task / 'original'
    prepared = json.loads((inputs / task / 'preparation/project.json').read_text())
    require(hashlib.sha256(encoded(common.sources(original))).hexdigest() == supplied['task_source_sha256'][task] and
            prepared['original'] == common.sources(original), 'independent original differs from the agent task')
    selected = [row for row in rows if row['profile'] == profile and row['task'] == task]
    require(len(selected) == 4, 'one cell must retain both setups and both repetitions')
    result = {'passed': False, 'study': 'M2', 'profile': profile, 'task': task, 'live_model_calls': 0,
              'summary_sha256': common.digest(data / 'summary.json'),
              'core_checker_sha256': supplied['core_checker_sha256'], 'image': image, 'control_policy': policy,
              'cache_archive_sha256': receipt['cache_archive_sha256'], 'attempts': {}}
    try:
        for row in selected:
            require(row['native_status'] in {'completed', 'failed'}, 'finish every native attempt before grading')
            patch = data / row['patch_file'] if row['patch_file'] is not None else None
            if patch is not None:
                require(patch.is_file() and not patch.is_symlink() and common.digest(patch) == row['patch_sha256'],
                        'submitted patch differs from the locally retained patch')
            tools = {'node': node, 'typescript_root': typescript, 'registry': cache / 'registry',
                     'target_cache': cache if task == 'regex-escape-len' else None}
            verdict = grading.grade(image, task, original, prepared, None, rust, patch, output / row['id'],
                                    milestone='M2', tools=tools)
            accepted = row['native_status'] == 'completed' and verdict['passed']
            result['attempts'][row['id']] = {'status': 'checked', 'outcome': 'passed' if accepted else 'failed',
                'native_status': row['native_status'], 'patch_accepted': verdict['passed'],
                'check_stage': verdict['verdict']['stage'], 'undo_exact': verdict['verdict']['undo_exact'],
                'redo_exact': verdict['verdict']['redo_exact'], 'patch_sha256': row['patch_sha256'],
                'checker_versions': verdict['verdict'].get('checker_versions'),
                'grading_seconds': verdict['elapsed_seconds'], 'actual_cost': None}
        result['passed'] = True
        return result
    finally:
        common.save(output / 'summary.json', result)


def merge(supplied, cells):
    rows = public.verify(supplied, 'M2')
    require(len(cells) == 12 and {(cell['profile'], cell['task']) for cell in cells} ==
            {(profile, task) for profile in report.AGENTS for task in TASKS},
            'need all twelve distinct profile/task cells')
    attempts = {}
    environments = {(cell['image'], cell['control_policy'], cell['cache_archive_sha256']) for cell in cells}
    require(len(environments) == 1, 'all cells must use the same independently checked compiler environment')
    for cell in cells:
        require(cell['passed'] is True and cell['study'] == 'M2' and cell['live_model_calls'] == 0 and
                cell['core_checker_sha256'] == supplied['core_checker_sha256'] and
                cell['summary_sha256'] == hashlib.sha256(encoded_summary(supplied)).hexdigest(),
                'cell source, image or submitted summary differs')
        expected = {row['id'] for row in rows if row['profile'] == cell['profile'] and row['task'] == cell['task']}
        require(set(cell['attempts']) == expected and not attempts.keys() & cell['attempts'].keys(),
                'cell repeats, omits or substitutes an attempt')
        attempts.update(cell['attempts'])
    for row in rows:
        checked = attempts[row['id']]
        require(checked['status'] == 'checked' and checked['native_status'] == row['native_status'] and
                checked['patch_sha256'] == row['patch_sha256'], 'independent patch or native outcome differs')
        require(checked['outcome'] == ('passed' if row['native_status'] == 'completed' and
                                      checked['patch_accepted'] else 'failed'),
                'failed or timed-out native work cannot become an accepted attempt')
    image, policy, cache = next(iter(environments))
    require(policy in {'frozen', 'fresh'} and (policy != 'frozen' or
            (image == supplied['controls']['private']['image'] and
             cache == supplied['controls']['private']['cache_archive_sha256'])), 'initial compiler environment differs')
    return {'passed': True, 'all_48_checked': True, 'checked_attempts': len(attempts),
            'image': image, 'control_policy': policy, 'cache_archive_sha256': cache,
            'core_checker_sha256': supplied['core_checker_sha256'], 'attempts': attempts, 'live_model_calls': 0}


def encoded_summary(value):
    return (json.dumps(value, indent=2, ensure_ascii=False, allow_nan=False) + '\n').encode()


def completion(summary):
    missing = []
    for profile in report.AGENTS:
        for task in TASKS:
            if not any(row['profile'] == profile and row['task'] == task and row['setup'] == 'fr' and
                       row['native_status'] == 'completed' and row['independent_outcome'] == 'passed' and
                       row['fr_use']['contributing_actions'] for row in summary['attempts']):
                missing.append({'profile': profile, 'task': task})
    return {'complete': not missing, 'missing_accepted_fr_cases': missing}


def markdown(summary):
    groups, matched = report.comparisons(summary, TASKS)
    rows = summary['attempts']
    accepted = sum(row['independent_outcome'] == 'passed' for row in rows)
    finished = completion(summary)
    lines = ['# M2: four code changes with the same agents', '',
        f'All 48 scheduled local CLI attempts were independently checked: {accepted} passed and {48 - accepted} failed.',
        'The baseline is the same app and model using ordinary reading, search, editing and shell tools.',
        'The other setup adds fr and its project skill. Each setup gets two attempts at each request:', '',
        '- Remove an unused Boolean parameter across two Rust crates and update five callers.',
        '- Rename a TypeScript export while keeping an imported local alias and an unrelated function unchanged.',
        '- Add an escaped-text byte count in both regex crates. `a.b` needs four bytes; `é` needs two. Check allocations too.',
        '- Include the invoice fee: `invoice_total(5, 2)` must return 7.', '',
        'These are familiar development tasks. Two repetitions cannot establish general savings or a general success rate.',
        'An accepted result must finish within the frozen limits and deliver a patch that passes the independent checks.',
        'The checker applies each patch to the original source, runs compiler and behavior checks, and verifies exact undo and redo.',
        'Timeouts stay failed even if their patches pass. Supplied controls are checked separately and are not agent successes.', '',
        '## Results and complete attempt time', '',
        '| Agent | Tools | Passed / attempted | Mean successful seconds | All attempt seconds | Seconds spent per success |',
        '| --- | --- | ---: | ---: | ---: | ---: |']
    for group in groups:
        lines.append(f'| {report.AGENTS[group["profile"]]} | {"ordinary" if group["setup"] == "files" else "ordinary + fr"} | '
            f'{group["accepted"]} / 8 | {report.number(group["successful_mean_seconds"])} | '
            f'{report.number(group["total_seconds"])} | {report.number(group["seconds_per_success"])} |')
    lines += ['', 'Seconds spent per success includes all eight attempts, including failures.',
        'The next table compares only the same requests and repetitions where both setups passed.', '',
        '| Agent | Matching successful pairs | Ordinary mean seconds | With fr mean seconds | Observed time change |',
        '| --- | ---: | ---: | ---: | ---: |']
    for pair in matched:
        change = 'undefined' if pair['fr_change_percent'] is None else f'{pair["fr_change_percent"]:+.1f}%'
        lines.append(f'| {report.AGENTS[pair["profile"]]} | {pair["pairs"]} | {report.number(pair["ordinary_mean_seconds"])} | '
            f'{report.number(pair["fr_mean_seconds"])} | {change} |')
    lines += ['', 'Positive time change means that adding fr took longer on those matching successes.',
        'These are complete code-change times, not execution-speed benchmarks of the changed programs.',
        'Preparation, instructions, indexing, model replies, tools, checks and result export share the 150-second deadline.',
        'Controller Python startup and outer guard startup remain unmeasured; shared preparation is separate in [summary.json](summary.json).',
        'A command count alone establishes no benefit. Contributing actions were reviewed against the delivered patch.', '',
        '## Requests completed', '',
        '| Agent | Request | Ordinary passes / 2 | With fr passes / 2 |', '| --- | --- | ---: | ---: |']
    for profile in report.AGENTS:
        for task in TASKS:
            counts = [sum(row['independent_outcome'] == 'passed' for row in rows if row['profile'] == profile and
                          row['task'] == task and row['setup'] == setup) for setup in ('files', 'fr')]
            lines.append(f'| {report.AGENTS[profile]} | {TASKS[task]} | {counts[0]} | {counts[1]} |')
    lines += ['', '## Reported tokens and limits', '',
        'A token is a unit of text counted by a model. Means below use the same matching successful pairs.',
        'CLI categories differ and can overlap. Do not add columns or compare token totals across clients.',
        'Missing usage and actual account charges remain unknown. A zero catalog price does not establish free billing.', '',
        '| Agent | Tools | Mean reported input | Mean cached input | Mean reported output | Mean reasoning |',
        '| --- | --- | ---: | ---: | ---: | ---: |']
    for profile in report.AGENTS:
        for setup in ('files', 'fr'):
            means = report.token_means(rows, profile, setup)
            lines.append(f'| {report.AGENTS[profile]} | {"ordinary" if setup == "files" else "ordinary + fr"} | ' +
                         ' | '.join('unknown' if value is None else report.number(value) for value in means) + ' |')
    lines += ['', 'All attempts ran serially with the same 150-second deadline. No failed slot was retried.',
        'The outer workstation guard kept its 180-second deadline and resource limits.',
        'The native CLIs expose no shared hard token/request cap. Fresh conversations do not control operating-system or provider caches.',
        'Client-selected models are recorded; provider backend identity and billed charges are not independently observed.',
        'Full CLI logs, prompts, session exports, source archives and execution packets stay local. Only patches and summaries are published.', '',
        '## Every attempt', '',
        '| Agent | Request | Tools | Repeat | Outcome | Local seconds | Fr commands | Patch |',
        '| --- | --- | --- | ---: | --- | ---: | ---: | --- |']
    for row in rows:
        outcome = row['independent_outcome']
        if row['stop_reason'] == 'wall_seconds' or row['complete_attempt_deadline_missed']:
            outcome = 'time limit'
        if row['native_status'] == 'failed' and row['independent_check']['patch_accepted']:
            outcome += '; patch passed'
        patch = f'[patch]({row["patch_file"]})' if row['patch_file'] else 'none'
        lines.append(f'| {report.AGENTS[row["profile"]]} | {TASKS[row["task"]]} | '
            f'{"ordinary" if row["setup"] == "files" else "ordinary + fr"} | {row["repetition"]} | '
            f'{outcome} | {row["total_seconds"]:.3f} | {row["fr_use"]["commands_observed"]} | {patch} |')
    lines += ['', '## Milestone requirement', '',
        'Every profile needs an accepted fr result for every request, with reviewed contributing fr actions.']
    lines.append('That requirement passed.' if finished['complete'] else 'That requirement remains open for:')
    lines.append('')
    lines += [f'- {report.AGENTS[row["profile"]]}: {TASKS[row["task"]]}.' for row in finished['missing_accepted_fr_cases']]
    lines += ['', 'Exact settings, versions, control runs, timing, token categories and original local record hashes are in [summary.json](summary.json).', '']
    return '\n'.join(lines)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='operation', required=True)
    checking = commands.add_parser('grade-cell')
    for name in ('data', 'inputs', 'artifact', 'rust', 'node', 'typescript', 'output'):
        checking.add_argument('--' + name, required=True, type=Path)
    for name in ('image', 'profile', 'task'):
        checking.add_argument('--' + name, required=True)
    checking.add_argument('--policy', choices=('frozen', 'fresh'), required=True)
    merging = commands.add_parser('merge')
    for name in ('data', 'cells', 'output'):
        merging.add_argument('--' + name, required=True, type=Path)
    rendering = commands.add_parser('report')
    for name in ('data', 'checks'):
        rendering.add_argument('--' + name, required=True, type=Path)
    rendering.add_argument('--mode', choices=('write', 'check'), required=True)
    args = parser.parse_args()
    if args.operation == 'grade-cell':
        result = grade_cell(args.data.resolve(), args.inputs.resolve(), args.artifact.resolve(), args.image,
                            args.rust.resolve(), args.node.resolve(), args.typescript.resolve(),
                            args.profile, args.task, args.policy, args.output.resolve())
        print(json.dumps({'passed': result['passed'], 'checked': len(result['attempts'])}))
    elif args.operation == 'merge':
        supplied = json.loads((args.data / 'summary.json').read_text())
        cells = [json.loads(path.read_text()) for path in args.cells.glob('*/summary.json')]
        common.save(args.output, merge(supplied, cells))
    else:
        supplied = json.loads((args.data / 'summary.json').read_text())
        report.verify_checks(supplied, json.loads(args.checks.read_text()), 48)
        content = markdown(supplied)
        path = args.data / 'README.md'
        if args.mode == 'write':
            path.write_text(content)
        else:
            require(path.read_text() == content, 'regenerate the report from the independently checked summary')
