"""Publish reviewed product records and independently grade their exact patches."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess

from . import product_plan as plan, product_report as report, product_runtime as runtime
from . import selected_local as local, selected_public as public, selected_tasks as common
from .selected_plan import ROOT, encoded, prepare, require


def resource_pass(resources):
    limits = plan.LIMITS
    return (type(resources.get('exit_code')) is int and resources['exit_code'] == 0 and
            all(name in resources and resources[name] is None for name in ('stop_reason', 'monitor_error', 'launch_error')) and
            all(report.finite(resources.get(name)) and resources[name] <= ceiling for name, ceiling in {
                'elapsed_seconds': limits['wall_seconds'], 'sampled_aggregate_rss_bytes': limits['rss_bytes'],
                'sampled_cpu_seconds': limits['cpu_limit_seconds'], 'sampled_disk_growth_bytes': limits['disk_bytes'],
                'transcript_bytes': limits['transcript_bytes']}.items()))


def export(plan_path, review, destination):
    frozen = json.loads(plan_path.read_text())
    study = frozen['schedule']['milestone']
    phase = plan.STUDIES[study]
    schedule = prepare(ROOT, study)
    require(frozen['schedule'] == schedule and frozen['implementation'] == local.implementation_bindings(study) and
            hashlib.sha256(encoded({key: value for key, value in frozen.items() if key != 'sha256'})).hexdigest() == frozen['sha256'],
            'export the unchanged execution plan and collector')
    require(set(review) == {row['id'] for row in schedule['attempts']}, 'review every scheduled slot, including unstarted work')
    destination.mkdir(parents=True, exist_ok=False)
    (destination / 'patches').mkdir()
    observations, receipts = {}, {}
    for row in schedule['attempts']:
        identity = row['id']
        directory = plan_path.parent / 'attempts' / identity
        record_path = directory / 'result.json'
        reviewed = review[identity]
        require(set(reviewed) == {'record_sha256', 'contributing_actions', 'failure_summary'}, 'publish only reviewed contribution and failure notes')
        note = reviewed['failure_summary']
        require(note is None or isinstance(note, str) and 0 < len(note) <= 240 and '\n' not in note,
                'publish a short failure explanation, not a raw exception')
        if not record_path.exists():
            require(reviewed['record_sha256'] is None and not reviewed['contributing_actions'], 'unstarted work has no native result or fr contribution')
            observations[identity] = {'failure_summary': note}
            continue
        record = json.loads(record_path.read_text())
        require(common.digest(record_path) == reviewed['record_sha256'] and record['plan_sha256'] == frozen['sha256'] and
                all(record[key] == row[key] for key in ('id', 'profile', 'task', 'setup')) and
                record['status'] in {'completed', 'failed'}, 'review the exact terminal result for this frozen slot')
        resources = record['complete_attempt_resources']
        measured = plan_path.parent / 'attempt-monitors' / identity / 'resources.json'
        require(common.digest(measured) == record['complete_attempt_resources_sha256'] and
                json.loads(measured.read_text()) == resources and record['elapsed_seconds'] == resources['elapsed_seconds'] and
                resources['limits'] == {'wall_seconds': 150, 'rss_bytes': plan.LIMITS['rss_bytes'], 'cpu_seconds': 75,
                    'disk_bytes': plan.LIMITS['disk_bytes'], 'transcript_bytes': plan.LIMITS['transcript_bytes']},
                'keep the original complete-attempt measurement and limits')
        stream = directory / 'native.stdout'
        events = []
        if stream.exists():
            require(common.digest(stream) == record['native_stdout_sha256'], 'native stream changed after observation')
            for line in stream.read_bytes().splitlines():
                try:
                    events.append(json.loads(line))
                except ValueError:
                    pass
        client = schedule['profiles'][row['profile']]['client']
        if client == 'codex':
            values = [event['usage'] for event in events if event.get('type') == 'turn.completed']
            accounted = len(values) == 1 and record['status'] == 'completed'
        else:
            values = [event['part']['tokens'] for event in events if event.get('type') == 'step_finish']
            usage = record['usage'].get('exported') if record.get('usage') else None
            accounted = bool(usage and usage['outcome'] == 'succeeded' and usage['usage_complete'] and
                usage['unfinished_assistant_responses'] == 0 and len(values) > 0 and
                len(values) == usage['exported_assistant_responses'] == usage['stream_finished_responses'])
        if record['patch_delivered']:
            source = directory / 'change.patch'
            require(source.is_file() and not source.is_symlink() and common.digest(source) == record['patch_sha256'],
                    'publish the exact submitted patch')
            shutil.copyfile(source, destination / 'patches' / (identity + '.patch'))
        actions = reviewed['contributing_actions']
        observed = public.observed_fr(events)
        require(not actions or observed['commands_observed'] > 0, 'a reviewed contribution needs an observed fr invocation')
        observations[identity] = {'status': record['status'], 'independent_outcome': None,
            'patch_delivered': record['patch_delivered'], 'patch_sha256': record['patch_sha256'],
            'checking_inputs_preserved': not record['checking_input_changes'],
            'accounting_complete': accounted and not record['collection_errors'] and not record['error'],
            'resource_stop': resources['stop_reason'] or (None if resource_pass(resources) else 'incomplete-resource-measurement'),
            'elapsed_seconds': record['elapsed_seconds'], 'usage': public.sum_counts(values), 'cost': None,
            'fr_contribution': actions, 'failure_summary': note}
        receipts[identity] = {'record_sha256': reviewed['record_sha256'], 'resources': resources,
            'resources_sha256': record['complete_attempt_resources_sha256'], 'fr_observed': observed,
            'reported_usage_complete': accounted}
    summary = report.summary(ROOT, phase, observations)
    common.save(destination / 'observations.json', observations)
    common.save(destination / 'summary.json', summary)
    common.save(destination / 'collection.json', {'schema': 'fr-product-collection-1', 'study': study,
        'plan_sha256': frozen['sha256'], 'schedule_sha256': schedule['sha256'], 'implementation': frozen['implementation'],
        'controls': frozen['controls'], 'admission': frozen['admission'], 'receipts': receipts,
        'binary_sha256': {name: entry['sha256'] for name, entry in frozen['binaries'].items()},
        'versions': frozen['versions'], 'checker_versions': frozen['checker_versions'],
        'publication': 'Reviewed summary and submitted patches only; raw prompts, transcripts and account exports remain local.'})
    return summary


def grade(data, inputs, qualification, rust, output, study):
    from . import selected_grading as grader
    require(os.environ.get('GITHUB_ACTIONS') == 'true', 'independently grade on GitHub')
    schedule = prepare(ROOT, study)
    controls = runtime.validate_controls(json.loads((qualification / 'controls.json').read_text()), study)
    collection = json.loads((data / 'collection.json').read_text())
    require(collection['study'] == study and collection['schedule_sha256'] == schedule['sha256'] and
            collection['controls'] == controls and collection['implementation'] == local.implementation_bindings(study),
            'grade the originally frozen source, collector and task controls')
    observations = json.loads((data / 'observations.json').read_text())
    original_summary = report.summary(ROOT, plan.STUDIES[study], observations)
    require(json.loads((data / 'summary.json').read_text()) == original_summary and
            all(row['status'] in {'completed', 'failed'} for row in original_summary['attempts']),
            'retain and finish all 48 slots before final independent grading')
    archive = qualification / 'grader-image.tar.gz'
    require(common.digest(archive) == controls['private_image_archive_sha256'], 'private checker archive changed')
    subprocess.run(grader.DOCKER + ['load', '--input', str(archive)], check=True, timeout=60)
    require(subprocess.check_output(grader.DOCKER + ['image', 'inspect', '--format', '{{.Id}}', controls['image']],
                                   text=True, timeout=10).strip() == controls['image'], 'private checker identity differs')
    output.mkdir(parents=True, exist_ok=False)
    result = {'schema': 'fr-product-grading-1', 'round': plan.STUDIES[study], 'run': os.environ['GITHUB_RUN_ID'],
              'source_commit': common.comparison_grade.BASE.git(ROOT, 'rev-parse', 'HEAD').stdout.decode().strip(),
              'checker_bindings': controls['bindings'], 'attempts': {}}
    originals, prepared = {}, {}
    for task in schedule['requirements']['tasks']:
        directory = inputs / task
        seed = json.loads((directory / 'seed.json').read_text())
        require(seed['archive_sha256'] == controls['seeds'][task], 'use the original qualified task archive')
        originals[task] = output / 'originals' / task
        local.unpack(directory / 'original.tar.gz', seed, originals[task])
        prepared[task] = json.loads((directory / 'project.json').read_text())
    try:
        for row in original_summary['attempts']:
            patch = data / 'patches' / (row['id'] + '.patch') if row['patch_delivered'] else None
            if patch is not None:
                require(patch.is_file() and not patch.is_symlink() and common.digest(patch) == row['patch_sha256'], 'submitted patch changed')
            checked = grader.grade(controls['image'], row['task'], originals[row['task']], prepared[row['task']],
                None, rust, patch, output / row['id'], milestone=study, tools={})
            require('error' not in checked, 'independent checker failed; preserve the original attempt and inspect its checker record')
            result['attempts'][row['id']] = {'passed': checked['passed'], 'patch_sha256': row['patch_sha256'],
                'stage': checked['verdict']['stage'], 'undo_exact': checked['verdict']['undo_exact'],
                'redo_exact': checked['verdict']['redo_exact']}
            observations[row['id']]['independent_outcome'] = 'passed' if checked['passed'] else 'failed'
        common.save(output / 'summary.json', report.summary(ROOT, plan.STUDIES[study], observations, result))
    finally:
        common.save(output / 'grading.json', result)
    return result


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='operation', required=True)
    exporting = commands.add_parser('export')
    for name in ('plan', 'review', 'output'):
        exporting.add_argument('--' + name, type=Path, required=True)
    grading = commands.add_parser('grade')
    for name in ('data', 'inputs', 'qualification', 'rust', 'output'):
        grading.add_argument('--' + name, type=Path, required=True)
    grading.add_argument('--study', choices=tuple(plan.STUDIES), required=True)
    args = parser.parse_args()
    if args.operation == 'export':
        result = export(args.plan.resolve(), json.loads(args.review.read_text()), args.output.resolve())
    else:
        result = grade(args.data.resolve(), args.inputs.resolve(), args.qualification.resolve(),
                       args.rust.resolve(), args.output.resolve(), args.study)
    print(json.dumps({'attempts': len(result['attempts'])}))
