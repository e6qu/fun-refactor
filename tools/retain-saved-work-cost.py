"""Publish tracking and readable tables from an audited original measurement."""
import argparse
from datetime import datetime
import importlib.util
import json
import os
from pathlib import Path
import re
import statistics
import urllib.request
import zipfile

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('saved_cost', ROOT / 'tools/saved-work-cost.py')
cost = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cost)


def api(path):
    request = urllib.request.Request('https://api.github.com/repos/' + os.environ['GITHUB_REPOSITORY'] + '/' + path,
        headers={'Authorization': 'Bearer ' + os.environ['GH_TOKEN'], 'Accept': 'application/vnd.github+json'})
    with urllib.request.urlopen(request, timeout=30) as response:
        return json.load(response)


def retain(directory, run_id):
    directory = directory.resolve()
    cost.require(directory.resolve().is_relative_to((ROOT / 'tests/agent-eval/results').resolve()), 'unsafe record directory')
    report = cost.audit(directory)
    cost.require(run_id == report['freeze']['run_id'], 'different collection run')
    run = api(f'actions/runs/{run_id}')
    cost.require(run['status'] == 'completed' and run['conclusion'] == 'success'
        and run['head_sha'] == report['source_commit'] and run['path'] == '.github/workflows/saved-work-cost.yml',
        'wait for the successful original collection')
    jobs = api(f'actions/runs/{run_id}/jobs?per_page=100')['jobs']
    cost.require(all(any(job['name'] == name and job['conclusion'] == 'success' for job in jobs)
        for name in ('Familiar projects', 'Reserved project')), 'missing qualifying original project jobs')
    for attempt in report['reserved']['previous_attempts']:
        if attempt['conclusion'] == 'failure':
            base = ROOT / 'tests/agent-eval/saved-work-cost/failures' / str(attempt['run_id'])
            failure = json.loads(base.with_suffix('.json').read_text())
            archive = base.with_suffix('.zip')
            cost.require(failure['run_id'] == attempt['run_id'] and failure['source_commit'] == attempt['head_sha']
                and failure['artifact_bytes'] == archive.stat().st_size
                and failure['artifact_digest'] == 'sha256:' + cost.file_digest(archive)
                and base.with_suffix('.log').is_file(), 'failed development record is missing or changed')
    relative = directory.relative_to(ROOT).as_posix()
    cost.save(ROOT / 'tests/agent-eval/saved-work-cost/record.json', {
        'directory': relative, 'sha256': cost.file_digest(directory / 'result.json'), 'run_id': run_id,
        'workflow': {key: run[key] for key in ('head_sha', 'path', 'html_url', 'created_at', 'run_started_at', 'updated_at', 'conclusion')},
        'jobs': [{key: job[key] for key in ('name', 'started_at', 'completed_at', 'conclusion', 'steps')} for job in jobs]})
    build, = [job for job in jobs if job['name'] == 'build']
    compilation, = [step for step in build['steps'] if step['name'] == 'Run cargo build --locked --release --bin fr']
    build_seconds = (datetime.fromisoformat(compilation['completed_at'])
        - datetime.fromisoformat(compilation['started_at'])).total_seconds()
    catalog_path = ROOT / 'tests/agent-eval/roadmap.json'
    catalog = json.loads(catalog_path.read_text())
    cost.require(not any(row['id'] == 'saved-work-cost' for row in catalog['evidence']), 'preserve the original cost record')
    catalog['evidence'].append({'id': 'saved-work-cost', 'path': relative + '/result.json',
        'sha256': cost.file_digest(directory / 'result.json'), 'kind': 'measurement',
        'audit': 'python3 -B tools/saved-work-cost.py --audit ' + relative,
        'scope': 'Three repetitions on glob, Boltons and first reserved JMESPath. Saved and fresh answers agree across first use, unchanged reuse and a single-file edit. Includes startup, validation, discovery, storage and complete program requests; no agent savings claim.'})
    gate, = [row for row in catalog['obligations'] if row['id'] == 'C.performance']
    cost.require(gate['requires'] == [], 'review existing performance evidence before replacement')
    gate['requires'] = [{'evidence': 'saved-work-cost', 'assertions': [
        {'pointer': '/passed', 'equals': True}, {'pointer': '/projects', 'equals': 3},
        {'pointer': '/repetitions', 'equals': 3}, {'pointer': '/answer_agreement', 'equals': True},
        {'pointer': '/reserved/first_evaluation', 'equals': True}]}]
    gate['next'] = 'Use the measured setup, validation and repeated discovery costs to choose an improvement. Compare complete sessions on identical files and questions. Keep full agent-task savings separate.'
    cost.save(catalog_path, catalog)
    with zipfile.ZipFile(directory / 'measurements.zip') as archive:
        projects = [json.loads(archive.read(task['id'] + '/result.json')) for task in report['tasks']['projects']]
    lines = ['# What saved project work costs', '',
        'You have already found a function and its callers. Does reopening that answer save work after a pause?', '',
        'This comparison asks the same bounded questions with saved answers and with fresh discovery. '
        'Both approaches start with separate empty native caches and retain those caches between requests.', '',
        'The projects are Rust glob, Python Boltons and Python JMESPath. '
        'JMESPath stayed unread until the procedure and optimized binary were fixed. '
        'Its first independent evaluation passed without changing the checker after reading its code.', '',
        'Each project has three repetitions. Each repetition includes first use, unchanged reuse and a source edit. '
        'The edit changes occurrences of one function name in one declared file. '
        'Both approaches then ask about the original name. '
        'All 27 paired answers agree, including warnings about callers left unchecked.', '',
        'The [comparison guide](saved-work-cost.md) explains the questions, extra work and measurement limits.', '',
        '## Request time', '',
        'Times below are medians in milliseconds. Each request includes a new Python process and SDK imports. '
        'Saved requests include restoring, checking and writing their plan and answer files.', '',
        'Both approaches also hash source files and write diagnostic records. Those measurement costs remain in request totals.', '',
        '| Project | First fresh | First saved | Repeat fresh | Repeat saved | Edited fresh | Edited saved |',
        '|---|---:|---:|---:|---:|---:|---:|']
    for project in projects:
        name = project['project']['id']
        values = [report['summary'][name][condition][stage]['median_seconds'] * 1000
            for stage in ('first', 'reuse', 'edit') for condition in ('fresh', 'saved')]
        lines.append('| ' + name + ' | ' + ' | '.join(f'{value:.1f}' for value in values) + ' |')
    lines += ['', '## Complete three-question session', '',
        'The session includes its source copy, all three requests, cleanup and the measured edit through `fr file replace`. '
        'Shared source download and extraction remain separate. '
        'These totals expose whether an unchanged reuse repays the initial saved-work overhead.', '',
        '| Project | Fresh seconds | Saved seconds | Saved change from fresh |', '|---|---:|---:|---:|']
    for project in projects:
        totals = {condition: statistics.median(row['total_seconds'] for row in project['sessions']
            if row['condition'] == condition) for condition in ('fresh', 'saved')}
        lines.append(f"| {project['project']['id']} | {totals['fresh']:.3f} | {totals['saved']:.3f} | "
            f"{(totals['saved'] / totals['fresh'] - 1) * 100:+.1f}% |")
    lines += ['', 'Positive percentages mean saved work took longer. '
        'These scripts already know which questions to ask. They establish no saving on a complete agent task.', '',
        '## Repeating the unchanged question', '',
        'This table separates waiting for `fr` calls from the rest of each request. '
        'The remainder includes Python startup, reading saved objects and writing diagnostics. '
        'Call time includes SDK reply parsing. It is not a separate native CPU measurement.', '',
        '| Project | Approach | fr commands | Call milliseconds | Other request milliseconds |',
        '|---|---|---:|---:|---:|']
    for project in projects:
        for condition in ('fresh', 'saved'):
            rows = [row for session in project['sessions'] if session['condition'] == condition
                for row in session['stages'] if row['stage'] == 'reuse']
            count = statistics.median(row['native_calls'] for row in rows)
            calls = statistics.median(row['native_seconds'] for row in rows)
            other = statistics.median(row['seconds'] - row['native_seconds'] for row in rows)
            lines.append(f"| {project['project']['id']} | {condition} | {count:g} | {calls * 1000:.1f} | {other * 1000:.1f} |")
    lines += ['',
        '## CPU, memory and storage', '',
        'CPU totals include the source copy, cleanup, requests and edit. '
        'GNU time, the runner\'s process timer, rounds combined CPU use to hundredths of a second. '
        'The raw record also includes finer CPU readings from Python and its commands.', '',
        'Memory shows the largest sampled total for Python and its running commands. '
        'The table gives the median of three session peaks. Short peaks can escape sampling. '
        'Final storage includes the native cache, edit history and saved plan and answer files. '
        'Source files and retained measurement logs have separate sizes.', '',
        'Storage and growth sum file lengths. Filesystem block and directory overhead remain unmeasured.', '',
        'CPU time measures processor work. Elapsed time measures how long the request takes. '
        'KiB means 1,024 bytes. MiB means 1,048,576 bytes.', '',
        '| Project | Approach | CPU seconds | Peak sampled MiB | Final state KiB | Request disk growth KiB |',
        '|---|---|---:|---:|---:|---:|']
    for project in projects:
        for condition in ('fresh', 'saved'):
            sessions = [session for session in project['sessions'] if session['condition'] == condition]
            edits = {session['repetition']: json.loads(archive_read(directory,
                f"{project['project']['id']}/{session['repetition']}-{condition}/edit.json")) for session in sessions}
            cpu = statistics.median(session['copy_cpu_seconds'] + session['cleanup_cpu_seconds']
                + sum(row['cpu_seconds'] for row in session['stages'])
                + edits[session['repetition']]['cpu_seconds']
                for session in sessions)
            memory = statistics.median(max([row['sampled_aggregate_rss_bytes'] for row in session['stages']]
                + [edits[session['repetition']]['measurement']['sampled_aggregate_rss_bytes']]) for session in sessions)
            storage = statistics.median(sum(session['stages'][-1]['storage_after'].values()) for session in sessions)
            growth = statistics.median(sum(row['sampled_disk_growth_bytes'] for row in session['stages']) for session in sessions)
            lines.append(f"| {project['project']['id']} | {condition} | {cpu:.3f} | {memory / 1048576:.1f} | "
                f'{storage / 1024:.1f} | {growth / 1024:.1f} |')
    lines += ['', '## Text and preparation costs', '',
        'Every request includes the full worker program, its invocation and the question. '
        'Both approaches return the same answer bytes for the same project state. '
        'The raw record separates agent-visible text from intermediate `fr` replies. '
        'Those reply counts use compact JSON with sorted field names, excluding command-output whitespace differences.', '',
        'Both approaches use Python. The comparison cannot isolate its extra cost compared with calling `fr` directly.', '',
        '| Project | Source download and extraction seconds | Source KiB | Archive KiB |', '|---|---:|---:|---:|']
    for project in projects:
        lines.append(f"| {project['project']['id']} | {project['source_preparation_seconds']:.3f} | "
            f"{project['source_bytes'] / 1024:.1f} | {project['archive_bytes'] / 1024:.1f} |")
    lines += ['', '## Reproduction and limits', '',
        f'The optimized compilation took {build_seconds:.0f} seconds, outside the recurring request totals. '
        'The [collection steps](../tests/agent-eval/saved-work-cost/record.json) preserve checkout, toolchain setup and build timings separately.', '',
        f'The [original workflow]({run["html_url"]}) records build and environment setup outside request totals. '
        f'The [retained measurement](../{relative}/result.json) identifies the source, optimized binary, questions and raw archive.', '',
        'Operating-system caches remain uncontrolled. The order alternates, and each approach owns a separate native cache. '
        'Three repetitions provide a small project comparison. They cannot establish general speed or memory improvements.', '',
        'The next change should target measured repeated work and preserve answer agreement. '
        'Compare complete sessions again before recommending saved work as a faster approach.', '',
        'Earlier attempts remain in the first-evaluation record:', '']
    attempts = report['reserved']['previous_attempts']
    lines += [f"- [Run {row['run_id']}]({row['url']}): {row['conclusion']}; reserved implementation opened: {str(row['reserved_opened']).lower()}."
        for row in attempts] if attempts else ['No earlier workflow attempt preceded this evaluation.']
    (ROOT / 'docs/saved-work-cost-results.md').write_text('\n'.join(lines) + '\n')
    prose_spec = importlib.util.spec_from_file_location('saved_cost_prose', ROOT / 'tools/check-prose.py')
    prose = importlib.util.module_from_spec(prose_spec)
    prose_spec.loader.exec_module(prose)
    counts, budget = prose.gather(), prose.read_budget()
    cost.require(all(value <= budget[name] for name, value in counts.items()),
        'improve the writing; do not increase prose budgets: ' + str(counts))
    text = prose.BUDGET.read_text()
    for name, value in counts.items():
        text = re.sub(rf'^{re.escape(name)} \d+$', f'{name} {value}', text, flags=re.M)
    prose.BUDGET.write_text(text)


def archive_read(directory, name):
    with zipfile.ZipFile(directory / 'measurements.zip') as archive:
        return archive.read(name)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--directory', type=Path, required=True)
    parser.add_argument('--run-id', type=int, required=True)
    args = parser.parse_args()
    retain(args.directory, args.run_id)
