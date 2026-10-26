"""Export only approved patches and aggregate facts from local native attempts."""
import argparse
import hashlib
import json
from pathlib import Path
import shlex
import shutil

from . import selected_local as local
from . import selected_local_grade as grading
from . import selected_tasks as common
from .project_edits_results import encoded_summary, DETAILS
from .selected_plan import encoded, require


def command_tokens(command):
    def split(text):
        lexer = shlex.shlex(text, posix=True, punctuation_chars=';&|\n')
        lexer.whitespace = ' \t\r'
        lexer.whitespace_split = True
        lexer.commenters = ''
        return list(lexer)

    tokens = split(command)
    while len(tokens) == 3 and tokens[0].rsplit('/', 1)[-1] in {'sh', 'bash', 'zsh'} and tokens[1] in {'-c', '-lc'}:
        tokens = split(tokens[2])
    return tokens


def observed_fr(events):
    operations, count = set(), 0
    for event in events:
        command = None
        if event.get('type') == 'item.completed' and event['item'].get('type') == 'command_execution':
            command = event['item']['command']
        elif event.get('type') == 'tool_use' and event['part'].get('tool') == 'shell':
            command = event['part']['state']['input']['command']
        if command is None:
            continue
        try:
            tokens = command_tokens(command)
        except ValueError:
            continue
        for index, token in enumerate(tokens):
            if token.rsplit('/', 1)[-1] != 'fr' or (index and tokens[index - 1] not in
                    {'&&', ';', '|', '||', '\n', 'command', 'time'}):
                continue
            count += 1
            arguments = tokens[index + 1:]
            while arguments and arguments[0].startswith('-'):
                option = arguments.pop(0)
                if option in {'-C', '--root', '--max-file-size', '--context-basis', '--plan-basis'} and arguments:
                    arguments.pop(0)
            if arguments:
                verb = arguments[0]
                if verb in {'project', 'history', 'spec', 'author', 'file', 'audit'}:
                    subverb = arguments[1] if len(arguments) > 1 else ''
                    operations.add(verb + ' ' + subverb if subverb in {'find', 'show', 'map', 'apply', 'undo', 'redo',
                        'list', 'proof-check', 'prove', 'check', 'evidence', 'task', 'batch', 'replace', 'read',
                        'capabilities'} else verb)
                else:
                    operations.add(verb if verb in {'rename', 'refs', 'def', 'symbols', 'checks', 'check',
                        'callers', 'callees', 'workflow'} else 'other fr command')
    return {'commands_observed': count, 'operations_observed': sorted(operations),
            'method': 'Observed shell fr invocations, including shell -c wrappers and newline-separated commands. Commands constructed inside another program may be unobserved. A count is not a benefit.'}


def sum_counts(values):
    if not values:
        return None
    keys = set().union(*(value.keys() for value in values if isinstance(value, dict)))
    result = {}
    for key in sorted(keys):
        parts = [value.get(key) if isinstance(value, dict) else None for value in values]
        if all(isinstance(value, dict) for value in parts):
            result[key] = sum_counts(parts)
        elif all(type(value) is int for value in parts):
            result[key] = sum(parts)
        else:
            result[key] = None
    return result


def completed_tool_time(events):
    intervals = []
    for event in events:
        if event.get('type') != 'tool_use':
            continue
        timing = event['part']['state'].get('time', {})
        start, end = timing.get('start'), timing.get('end')
        if isinstance(start, (int, float)) and isinstance(end, (int, float)) and end >= start:
            intervals.append((start, end))
    merged = []
    for start, end in sorted(intervals):
        if merged and start <= merged[-1][1]:
            merged[-1] = (merged[-1][0], max(end, merged[-1][1]))
        else:
            merged.append((start, end))
    return {'completed_tool_calls_with_timing': len(intervals),
            'completed_tool_wall_seconds': sum(end - start for start, end in merged) / 1000 if intervals else None,
            'method': 'Union of recorded completed tool intervals. Missing intervals are unknown. Other elapsed time includes replies, CLI overhead and unrecorded unfinished calls.'}


def export(plan_path, review, destination):
    plan = json.loads(plan_path.read_text())
    milestone = plan['schedule']['milestone']
    require(milestone in {'M2', 'M3'} and
            hashlib.sha256(encoded({key: value for key, value in plan.items() if key != 'sha256'})).hexdigest() == plan['sha256'],
            'export the unchanged execution plan')
    require(plan['implementation'] == local.implementation_bindings(milestone), 'collection code changed after freezing')
    require(not destination.exists(), 'retain the original publication; write a new destination')
    destination.mkdir(parents=True)
    (destination / 'patches').mkdir()
    rows = []
    expected = {row['id'] for row in plan['schedule']['attempts']}
    require(set(review) == expected, 'review the contribution or lack of contribution for every scheduled slot')
    for planned in plan['schedule']['attempts']:
        identity = planned['id']
        directory = plan_path.parent / 'attempts' / identity
        record = json.loads((directory / 'result.json').read_text())
        require(record['status'] in {'completed', 'failed'} and record['plan_sha256'] == plan['sha256'] and
                all(record[key] == planned[key] for key in ('id', 'profile', 'task', 'setup')),
                'retain every completed or failed attempt from the original packet')
        require(review[identity]['record_sha256'] == common.digest(directory / 'result.json'),
                'contribution review refers to another local record')
        require(set(review[identity]) == {'record_sha256', 'contributing_actions', 'failure_summary'},
                'review the fr contribution and failure summary explicitly')
        failure_summary = review[identity]['failure_summary']
        require(failure_summary is None or (isinstance(failure_summary, str) and
                0 < len(failure_summary) <= 240 and '\n' not in failure_summary),
                'publish a concise failure explanation, not a raw CLI exception or command')
        actions = review[identity]['contributing_actions']
        require(isinstance(actions, list) and len(actions) <= 8 and
                all(isinstance(action, str) and 0 < len(action) <= 240 and '\n' not in action for action in actions),
                'publish concise reviewed contributions, not native transcript excerpts')
        require(planned['setup'] == 'fr' or not actions, 'ordinary-tool attempts cannot claim fr contributions')
        events = []
        stream = directory / 'native.stdout'
        if stream.exists():
            for line in stream.read_bytes().splitlines():
                try:
                    events.append(json.loads(line))
                except ValueError:
                    pass  # Interrupted JSON remains in the private record; usage stays incomplete.
        client = plan['schedule']['profiles'][planned['profile']]['client']
        if client == 'codex':
            counts = [event['usage'] for event in events if event.get('type') == 'turn.completed']
            complete = len(counts) == 1 and record['status'] == 'completed'
            model_recorded = False
        else:
            counts = [event['part']['tokens'] for event in events if event.get('type') == 'step_finish']
            observed = record['usage']['exported'] if record['usage'] is not None else None
            complete = (observed is not None and observed['outcome'] == 'succeeded' and
                        observed['unfinished_assistant_responses'] == 0 and len(counts) > 0 and
                        len(counts) == observed['exported_assistant_responses'] == observed['stream_finished_responses'] and
                        all(isinstance(value, dict) for value in counts))
            model_recorded = observed['model_observed'] if observed is not None else False
        patch = directory / 'change.patch'
        patch_file = None
        if record['patch_delivered']:
            require(patch.is_file() and not patch.is_symlink() and common.digest(patch) == record['patch_sha256'],
                    'publish the exact submitted patch')
            patch_file = 'patches/' + identity + '.patch'
            shutil.copyfile(patch, destination / patch_file)
        process = record['process']
        fr_use = {**observed_fr(events), 'contributing_actions': actions,
                  'contribution_review': 'Recorded review of observed commands, results and submitted patch; command counts alone are not credited.'}
        row = {key: planned[key] for key in ('id', 'profile', 'task', 'setup', 'repetition')}
        rows.append({**row, 'native_status': record['status'], 'actual_cost': None,
            'collection_receipt': plan['sha256'], 'record_sha256': common.digest(directory / 'result.json'),
            'total_seconds': record['elapsed_seconds'], 'native_seconds': process['elapsed_seconds'] if process else None,
            'verification_seconds': record['verification_seconds'], 'stop_reason': process['stop_reason'] if process else None,
            'complete_attempt_deadline_missed': record['complete_attempt_deadline_missed'],
            'checking_input_changes': record['checking_input_changes'],
            'sampled_rss_bytes': process['sampled_aggregate_rss_bytes'] if process else None,
            'reported_tokens': sum_counts(counts), 'reported_usage_complete': complete,
            'client_model_recorded': model_recorded, 'provider_response_model_observed': False, 'fr_use': fr_use,
            'patch_file': patch_file, 'patch_sha256': common.digest(patch) if patch_file else None,
            'completed_tools': completed_tool_time(events), 'independent_outcome': None,
            'collection_error_recorded': bool(record['error'] or record['collection_errors']),
            'failure_summary': failure_summary})
    summary = {'study': milestone, 'comparison': 'The same native app/model using ordinary tools and ordinary tools plus fr, twice per task.',
        'publication': 'User approved patches and summary only. Full logs, prompts, exports, source archives and execution packets stay local.',
        'profiles': plan['schedule']['profiles'], 'task_bindings': plan['schedule']['bindings'],
        'native_inputs': plan['source']['native_inputs'], 'core_checker_sha256': grading.core_digest(milestone),
        'task_source_sha256': {task: hashlib.sha256(encoded(seed['receipt']['original'])).hexdigest()
                              for task, seed in plan['seeds'].items()},
        'native_app_versions': plan['versions'], 'local_checker_versions': plan['checker_versions'],
        'native_binary_sha256': {name: value['sha256'] for name, value in plan['binaries'].items()},
        'native_companion_sha256': {name: value['sha256'] for name, value in plan['components'].items()},
        'fr_build': {name: plan['fr_build'][name] for name in ('source_commit', 'source_tree', 'executable_sha256')},
        'limits': plan['limits'], 'client_finish_seconds': plan['client_finish_seconds'],
        'public_check_environment': plan['public_check_environment'],
        'provider_request_cap_enforced': False, 'token_cap_enforced': False,
        'accounting': 'CLI-reported categories retain client meanings. Unknown account charges and incomplete usage are not replaced with zero.',
        'cache_state': 'Fresh conversations and projects. Operating-system and provider caches are uncontrolled.',
        'resource_scope': 'Elapsed time covers the complete controller attempt. CPU and RSS samples describe the native CLI process group. The outer workstation guard also covers controller preparation and capture.',
        'unmeasured_per_attempt': {'controller_python_startup_seconds': None, 'outer_guard_startup_seconds': None},
        'controls': plan['controls'],
        'collection_receipts': {plan['sha256']: plan['source']['commit']}, 'attempts': rows}
    if milestone == 'M2':
        summary['shared_setup'] = plan['shared_setup']
    (destination / 'summary.json').write_bytes(encoded_summary(summary))
    return summary


def accept_checks(data, checked_path, run):
    require(run.startswith('https://github.com/e6qu/fun-refactor/actions/runs/') and run.rsplit('/', 1)[-1].isdigit(),
            'supply the exact independent GitHub check run')
    summary = json.loads((data / 'summary.json').read_text())
    checked = json.loads(checked_path.read_text())
    milestone = summary['study']
    require(milestone in {'M2', 'M3'}, 'choose the explicit public study')
    count = 48 if milestone == 'M2' else 36
    require(checked['passed'] is True and checked[f'all_{count}_checked'] is True and checked['checked_attempts'] == count and
            checked['core_checker_sha256'] == summary['core_checker_sha256'] and
            set(checked['attempts']) == {row['id'] for row in summary['attempts']}, 'check every original submission')
    require(checked['control_policy'] == 'frozen' and checked['image'] == summary['controls']['private']['image'],
            'use the independent checker frozen before model calls')
    for row in summary['attempts']:
        verdict = checked['attempts'][row['id']]
        require(verdict['status'] == 'checked' and verdict['native_status'] == row['native_status'] and
                verdict['patch_sha256'] == row['patch_sha256'], 'independent checking examined another patch or native outcome')
        row.update(independent_outcome=verdict['outcome'], independent_check_run=run,
                   independent_check={key: verdict[key] for key in DETAILS})
    (data / 'summary.json').write_bytes(encoded_summary(summary))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='operation', required=True)
    exporting = commands.add_parser('export-m2')
    for name in ('plan', 'review', 'output'):
        exporting.add_argument('--' + name, type=Path, required=True)
    exporting_m3 = commands.add_parser('export-m3')
    for name in ('plan', 'review', 'output'):
        exporting_m3.add_argument('--' + name, type=Path, required=True)
    checking = commands.add_parser('accept-checks')
    for name in ('data', 'checks'):
        checking.add_argument('--' + name, type=Path, required=True)
    checking.add_argument('--run', required=True)
    args = parser.parse_args()
    if args.operation in {'export-m2', 'export-m3'}:
        require(json.loads(args.plan.read_text())['schedule']['milestone'] ==
                ('M2' if args.operation == 'export-m2' else 'M3'), 'export operation differs from the frozen milestone')
        summary = export(args.plan.resolve(), json.loads(args.review.read_text()), args.output.resolve())
        print(json.dumps({'study': summary['study'], 'attempts': len(summary['attempts']), 'publication': 'patches and summary only'}))
    else:
        accept_checks(args.data.resolve(), args.checks.resolve(), args.run)
