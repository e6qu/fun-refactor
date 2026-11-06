"""Export only approved patches and aggregate facts from local native attempts."""
import argparse
import hashlib
import json
import math
from pathlib import Path
import shlex
import shutil

from . import selected_local as local
from . import selected_local_grade as grading
from . import selected_tasks as common
from .project_edits_results import encoded_summary, DETAILS
from .selected_plan import ADMISSION_STUDIES, ALGORITHM_TASKS, DELIVERY_STUDY, CASE_DIRECTORIES, PROOF_STUDIES, PROOF_DIAGNOSTICS, REPAIR_STUDY, encoded, require


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
                if verb in {'project', 'history', 'spec', 'proof', 'author', 'file', 'audit'}:
                    subverb = arguments[1] if len(arguments) > 1 else ''
                    operations.add(verb + ' ' + subverb if subverb in {'find', 'show', 'map', 'apply', 'undo', 'redo',
                        'list', 'proof-task', 'proof-state', 'proof-check', 'prove', 'check', 'evidence', 'task', 'batch', 'replace', 'read',
                        'capabilities', 'show', 'start', 'state', 'save'} else verb)
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
    require(milestone in PROOF_STUDIES | {'M2'} and
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
        require(record['timing_clock'] == 'includes-system-sleep' and
                type(record['suspended_seconds']) in {int, float} and
                0 <= record['suspended_seconds'] <= record['elapsed_seconds'],
                'retain the observed complete-attempt clock and detected system sleep')
        require(review[identity]['record_sha256'] == common.digest(directory / 'result.json'),
                'contribution review refers to another local record')
        fields = {'record_sha256', 'contributing_actions', 'failure_summary'}
        if milestone == 'M3-feedback':
            fields.add('feedback_observations')
        elif milestone in {'M3-construction', 'M3-page-count', 'M3-facts', REPAIR_STUDY}:
            fields.add('proof_observations')
            if milestone == REPAIR_STUDY:
                fields.add('argument_repair_observations')
        elif milestone in ALGORITHM_TASKS:
            fields.add('algorithm_observations')
        require(set(review[identity]) == fields,
                'review the fr contribution and failure summary explicitly')
        failure_summary = review[identity]['failure_summary']
        require(failure_summary is None or (isinstance(failure_summary, str) and
                0 < len(failure_summary) <= 240 and '\n' not in failure_summary),
                'publish a concise failure explanation, not a raw CLI exception or command')
        actions = review[identity]['contributing_actions']
        require(isinstance(actions, list) and len(actions) <= 8 and
                all(isinstance(action, str) and 0 < len(action) <= 240 and '\n' not in action for action in actions),
                'publish concise reviewed contributions, not native transcript excerpts')
        require(planned['setup'] == 'fr' or milestone in PROOF_DIAGNOSTICS | ALGORITHM_TASKS or not actions,
                'ordinary-tool attempts cannot claim fr contributions')
        events = []
        stream = directory / 'native.stdout'
        if milestone in PROOF_DIAGNOSTICS | ALGORITHM_TASKS and stream.exists():
            require(common.digest(stream) == record['native_stdout_sha256'], 'diagnostic native stream changed after collection')
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
                        observed['usage_complete'] and
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
            'timing_clock': record['timing_clock'], 'suspended_seconds': record['suspended_seconds'],
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
        if milestone == 'M3-feedback':
            observation = review[identity]['feedback_observations']
            require(set(observation) == {'first_correct_repair_seconds', 'repeated_argument_type_errors', 'execution_validity'},
                    'review the declared feedback observations explicitly')
            seconds = observation['first_correct_repair_seconds']
            errors = observation['repeated_argument_type_errors']
            require(seconds is None or (type(seconds) in {int, float} and math.isfinite(seconds) and
                                       0 <= seconds <= record['elapsed_seconds']), 'invalid observed repair time')
            require(errors is None or (type(errors) is int and errors >= 0), 'invalid observed repeated error count')
            require(observation['execution_validity'] in {'valid', 'service-failed', 'collection-failed', 'unknown'},
                    'review diagnostic execution validity explicitly')
            rows[-1]['feedback_observations'] = observation
            rows[-1]['initial_check'] = {key: record[key] for key in
                ('initial_check_seconds', 'initial_check_bytes', 'initial_check_sha256', 'initial_feedback_supplied')}
        elif milestone in {'M3-construction', 'M3-page-count', 'M3-facts', REPAIR_STUDY}:
            observation = review[identity]['proof_observations']
            timings = {'first_passing_proof_seconds', 'package_check_seconds', 'patch_delivery_seconds'}
            if milestone in {'M3-page-count', 'M3-facts', REPAIR_STUDY}:
                timings.add('first_proof_check_seconds')
            if milestone == 'M3-facts':
                timings.add('proof_write_seconds')
            checks = {'proof_check_passed', 'proof_written_with_fr', 'package_check_passed'}
            require(set(observation) == timings | checks | {'execution_validity'}, 'review each construction stage explicitly')
            require(all(observation[key] is None or (type(observation[key]) in {int, float} and
                        math.isfinite(observation[key]) and 0 <= observation[key] <= record['elapsed_seconds'])
                        for key in timings), 'construction stage times must be observed within the attempt')
            require(all(observation[key] is None or type(observation[key]) is bool for key in checks),
                    'construction checks need observed pass/fail or explicit unknown')
            require(observation['execution_validity'] in {'valid', 'service-failed', 'collection-failed', 'unknown'},
                    'review construction execution explicitly')
            rows[-1]['proof_observations'] = observation
            if milestone == REPAIR_STUDY:
                from .proof_feedback import validate_repair_observations
                validate_repair_observations(review[identity]['argument_repair_observations'], record['elapsed_seconds'])
                rows[-1]['argument_repair_observations'] = review[identity]['argument_repair_observations']
        elif milestone in ALGORITHM_TASKS:
            from .algorithm_results import validate_observations
            rows[-1]['algorithm_observations'] = validate_observations(
                review[identity]['algorithm_observations'], record['elapsed_seconds'])
        if milestone in ADMISSION_STUDIES:
            resources = record['complete_attempt_resources']
            require(resources['limits'] == {'wall_seconds': 150, 'rss_bytes': 805306368,
                    'cpu_seconds': 75, 'disk_bytes': 536870912, 'transcript_bytes': 1048576},
                    'retain the complete guarded attempt measurement')
            receipt = plan_path.parent / 'attempt-monitors' / identity / 'resources.json'
            require(common.digest(receipt) == record['complete_attempt_resources_sha256'] and
                    json.loads(receipt.read_text()) == resources, 'complete-attempt resource receipt changed')
            rows[-1]['complete_attempt_resources'] = resources
            rows[-1]['complete_attempt_resources_sha256'] = record['complete_attempt_resources_sha256']
            if milestone == DELIVERY_STUDY:
                rows[-1]['delivery_capture_bound_seconds'] = resources['elapsed_seconds'] if record['patch_delivered'] else None
            rows[-1]['stop_reason'] = resources['stop_reason'] or rows[-1]['stop_reason']
            rows[-1]['sampled_rss_bytes'] = resources['sampled_aggregate_rss_bytes']
    comparison = ({'M3-feedback': 'Assisted repair with the same fr, task, draft and instructions; initial feedback visibility differs.',
                   'M3-construction': 'Four fresh construction attempts with fr and corrected shared instructions; no timing baseline.',
                   'M3-facts': 'Eight attempts compare selecting current division facts with receiving four declaration types; no completed proof is supplied.',
                   REPAIR_STUDY: 'Eight fresh complete page-count attempts; only the post-error argument-repair instruction differs.',
                   'M3-page-count': 'Four page-count attempts with fr and the unchanged M3 claim; no timing baseline.'}[milestone]
                  if milestone in PROOF_DIAGNOSTICS else
                  'The same native app/model using ordinary tools and ordinary tools plus fr, twice per task.')
    if milestone in ALGORITHM_TASKS:
        comparison = (plan['schedule']['requirements']['cost_comparison'] if milestone == DELIVERY_STUDY else
                      'The same executable Euclid algorithm, fr and shared skills. Compare selecting library facts '
                      'with supplied declarations.' if milestone == 'M3-algorithm-facts' else
                      'The same executable Euclid algorithm, fr, shared skills, declarations, plan nodes and dependencies. '
                      'Compare checklist and graph rendering only; no automatic search controller.')
    summary = {'study': milestone, 'comparison': comparison,
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
        'cache_state': 'Fresh conversations and projects, with a separate new .fr/cache for each attempt. Operating-system and provider caches are uncontrolled.',
        'resource_scope': 'Elapsed time covers the complete controller attempt. CPU and RSS samples describe the native CLI process group. The outer workstation guard also covers controller preparation and capture.',
        'unmeasured_per_attempt': {'controller_python_startup_seconds': None, 'outer_guard_startup_seconds': None},
        'controls': plan['controls'],
        'collection_receipts': {plan['sha256']: plan['source']['commit']}, 'attempts': rows,
        'collection_notes': {
            'system_sleep': [{'attempt': row['id'], 'suspended_seconds': row['suspended_seconds'],
                              'included_in_total_seconds': True} for row in rows if row['suspended_seconds'] >= 1],
            'interpretation': 'Complete elapsed time includes detected system sleep. Notes list pauses of at least one second; per-attempt fields retain all observations. Do not subtract pauses or describe these numbers as isolated tool overhead.'}}
    if milestone == 'M2':
        summary['shared_setup'] = plan['shared_setup']
    elif milestone in PROOF_STUDIES:
        summary['formal_inputs'] = plan['formal_inputs']
        summary['proof_trust'] = json.loads((local.ROOT / local.CASE_DIRECTORIES[milestone] / 'checkers.json').read_text())
    if milestone == 'M3-feedback':
        summary['scope'] = 'Eight assisted development-example repairs. No full M3 completion or general success-rate claim.'
    elif milestone == 'M3-construction':
        summary['scope'] = 'Four familiar development-example construction attempts. No causal improvement or full M3 claim.'
    elif milestone == 'M3-page-count':
        summary['scope'] = 'Four attempts on the unchanged M3 page-count claim. No ordinary-tool baseline, causal improvement or full M3 claim.'
    elif milestone == REPAIR_STUDY:
        summary['native_admission'] = plan['admission']
        summary['scope'] = 'Eight fresh full page-count attempts; no ordinary-tool overhead baseline or M3 completion claim.'
    elif milestone == 'M3-facts':
        summary['native_admission'] = plan['admission']
        summary['scope'] = 'Eight development-example attempts on selecting and applying division facts. No ordinary-tool timing baseline or full M3 claim.'
        summary['provider_request_count'] = None
    elif milestone in ALGORITHM_TASKS:
        summary['native_admission'] = plan['admission']
        summary['scope'] = ('Controlled development comparison on executable Lean Euclid, including termination and correctness. '
                            'No ordinary-tool overhead baseline, Rust/Python translation proof or M3 completion claim.')
        if milestone == DELIVERY_STUDY:
            summary['scope'] = ('Delivery of two supplied correct proofs. This does not measure proof discovery, '
                                'fr overhead against ordinary Lean, or completion of M3.')
        summary['guidance_bytes'] = {}
        from .algorithm_guidance import install
        import tempfile
        with tempfile.TemporaryDirectory() as temporary:
            for condition in plan['schedule']['requirements']['setups']:
                project = Path(temporary) / condition
                project.mkdir()
                if milestone == DELIVERY_STUDY:
                    from .proof_delivery import install as install_delivery
                    files = install_delivery(project, condition, Path(plan['checkers']['lake']['path']))
                else:
                    files = install(project, condition, milestone)
                summary['guidance_bytes'][condition] = {name: (project / name).stat().st_size for name in files}
    if milestone in ADMISSION_STUDIES:
        summary['resource_scope'] = ('Independent sampled limits cover the guard, controller, native CLI and workers, '
            'including preparation and accounting. The observer stays outside the CPU throttle. '
            'Short peaks and children reparented before discovery can still be missed.')
    (destination / 'summary.json').write_bytes(encoded_summary(summary))
    return summary


def accept_checks(data, checked_path, run):
    require(run.startswith('https://github.com/e6qu/fun-refactor/actions/runs/') and run.rsplit('/', 1)[-1].isdigit(),
            'supply the exact independent GitHub check run')
    summary = json.loads((data / 'summary.json').read_text())
    checked = json.loads(checked_path.read_text())
    milestone = summary['study']
    require(milestone in PROOF_STUDIES | {'M2'}, 'choose the explicit public study')
    count = len(local.prepare(local.ROOT, milestone)['attempts'])
    require(checked['passed'] is True and checked[f'all_{count}_checked'] is True and checked['checked_attempts'] == count and
            checked['core_checker_sha256'] == summary['core_checker_sha256'] and
            set(checked['attempts']) == {row['id'] for row in summary['attempts']}, 'check every original submission')
    image = (summary['controls']['private_image'] if milestone in ALGORITHM_TASKS else summary['controls']['private']['image'])
    require(checked['control_policy'] == 'frozen' and checked['image'] == image,
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
    exporting_feedback = commands.add_parser('export-feedback')
    for name in ('plan', 'review', 'output'):
        exporting_feedback.add_argument('--' + name, type=Path, required=True)
    exporting_construction = commands.add_parser('export-construction')
    for name in ('plan', 'review', 'output'):
        exporting_construction.add_argument('--' + name, type=Path, required=True)
    exporting_page = commands.add_parser('export-page-count')
    for name in ('plan', 'review', 'output'):
        exporting_page.add_argument('--' + name, type=Path, required=True)
    exporting_facts = commands.add_parser('export-facts')
    for name in ('plan', 'review', 'output'):
        exporting_facts.add_argument('--' + name, type=Path, required=True)
    exporting_repair = commands.add_parser('export-argument-repair')
    for name in ('plan', 'review', 'output'):
        exporting_repair.add_argument('--' + name, type=Path, required=True)
    exporting_algorithm = commands.add_parser('export-algorithm')
    for name in ('plan', 'review', 'output'):
        exporting_algorithm.add_argument('--' + name, type=Path, required=True)
    exporting_algorithm.add_argument('--study', choices=tuple(sorted(ALGORITHM_TASKS)), required=True)
    checking = commands.add_parser('accept-checks')
    for name in ('data', 'checks'):
        checking.add_argument('--' + name, type=Path, required=True)
    checking.add_argument('--run', required=True)
    args = parser.parse_args()
    if args.operation == 'export-algorithm':
        require(json.loads(args.plan.read_text())['schedule']['milestone'] == args.study, 'export the selected algorithm phase')
        summary = export(args.plan.resolve(), json.loads(args.review.read_text()), args.output.resolve())
        print(json.dumps({'study': summary['study'], 'attempts': len(summary['attempts']), 'publication': 'patches and summary only'}))
    elif args.operation in {'export-m2', 'export-m3', 'export-feedback', 'export-construction', 'export-page-count', 'export-facts', 'export-argument-repair'}:
        require(json.loads(args.plan.read_text())['schedule']['milestone'] ==
                {'export-m2': 'M2', 'export-m3': 'M3', 'export-feedback': 'M3-feedback',
                 'export-construction': 'M3-construction', 'export-page-count': 'M3-page-count',
                 'export-facts': 'M3-facts', 'export-argument-repair': REPAIR_STUDY}[args.operation],
                'export operation differs from the frozen study')
        summary = export(args.plan.resolve(), json.loads(args.review.read_text()), args.output.resolve())
        print(json.dumps({'study': summary['study'], 'attempts': len(summary['attempts']), 'publication': 'patches and summary only'}))
    else:
        accept_checks(args.data.resolve(), args.checks.resolve(), args.run)
