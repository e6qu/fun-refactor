"""Publish authenticated measurements and preserve the original project qualification."""
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import tempfile
import zipfile

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('comparison', ROOT / 'tools/saved-answer-comparison.py')
comparison = importlib.util.module_from_spec(spec)
spec.loader.exec_module(comparison)
cost = comparison.cost
DIRECTORY = ROOT / 'tests/agent-eval/saved-answer-improvement'


def authenticate(run_id, expected_head):
    run = cost.api(f'actions/runs/{run_id}')
    cost.require(run['status'] == 'completed' and run['conclusion'] == 'success'
                 and run['head_sha'] == expected_head and run['run_attempt'] == 1
                 and run['path'] == '.github/workflows/saved-answer-improvement.yml',
                 'measurement run did not pass with the declared source')
    jobs = cost.api(f'actions/runs/{run_id}/jobs?per_page=100')['jobs']
    return run, jobs


def retain_profile(run_id, expected_head, name):
    run, jobs = authenticate(run_id, expected_head)
    rows = cost.api(f'actions/runs/{run_id}/artifacts?per_page=100')['artifacts']
    artifact, = [row for row in rows if row['name'] == 'saved-answer-profile']
    data = cost.artifact_bytes(artifact)
    with zipfile.ZipFile(io.BytesIO(data)) as archive:
        entry, = archive.infolist()
        cost.require(entry.filename == 'saved-answer-profile.json' and entry.file_size <= 1048576,
                     'unexpected diagnostic profile')
        profile = archive.read(entry)
        cost.require(set(json.loads(profile)) == {'glob', 'boltons', 'jmespath'}, 'missing profiled answers')
    (DIRECTORY / (name + '.zip')).write_bytes(data)
    (DIRECTORY / (name + '.json')).write_bytes(profile)
    return {'run': run, 'jobs': jobs, 'artifact': artifact,
            'profile_sha256': hashlib.sha256(profile).hexdigest()}


def markdown(report):
    lines = ['# Spend less time reopening saved answers', '',
        'A user asks where `glob_with` appears and what other code its change could affect.',
        'Python programs use the helper library, called the SDK, to call `fr` and keep saved answers.',
        'On reopening that unchanged question, the previous helpers repeatedly hashed the same saved contents.',
        'A content hash identifies a piece by its bytes. Parent pieces record the hashes of their children.',
        'The new code checks each distinct piece once and builds parent hashes from those verified child hashes.',
        'It still rejects missing, corrupted or cyclic data and returns independently editable copies of repeated values.', '',
        'The comparison uses the previous SDK from merged PR #553 and the changed SDK with the same release build of `fr`.',
        'Both answer the same questions on identical pinned glob, Boltons and JMESPath sources.',
        'glob matches file paths in Rust. Boltons supplies Python utility functions. JMESPath selects values from JSON documents.',
        'Every question starts a new Python process.',
        'A cache saves earlier results. Each route starts with an empty `fr` analysis cache and keeps it between questions.',
        'The saved route also stores its task plan and answer, validates dependencies before reuse, and repeats discovery after an edit.',
        'The task plan records which inputs an answer depends on, so edits can expire the answer.',
        'The fresh route asks `fr` again and retains no task plan or answer.', '',
        'Each session contains first use, unchanged reuse and one edit, with three repetitions per project and SDK.',
        'SDK order alternates by project. Saved and fresh order alternates by repetition within each SDK.',
        'Each SDK finishes its three repetitions on a project before the other SDK starts that project.',
        'The edit replaces the selected name in one file through `fr file replace`; it checks invalidation rather than delivering a complete rename.',
        'For example, the glob trial replaces `glob_with` with `glob_with_cost_renamed`, then asks about the original name again.',
        'All 108 request answers agree between SDKs, and each saved answer agrees with its matching fresh check.', '',
        '## Complete session time', '',
        'Median seconds include Python startup, validation, discovery, persistence, output, checkout copies, the edit and cleanup.',
        'Preparation and environment setup remain separate in the raw reports and recorded workflow steps.', '',
        '| Project | Previous saved | Current saved | Change | Previous fresh | Current fresh | Current saved overhead versus fresh |',
        '|---|---:|---:|---:|---:|---:|---:|']
    for name, versions in report['summary'].items():
        old = versions['previous']['saved']['median_session_seconds']
        new = versions['current']['saved']['median_session_seconds']
        old_fresh = versions['previous']['fresh']['median_session_seconds']
        fresh = versions['current']['fresh']['median_session_seconds']
        lines.append(f'| {name} | {old:.3f} | {new:.3f} | {(new / old - 1) * 100:+.1f}% | {old_fresh:.3f} | {fresh:.3f} | {(new / fresh - 1) * 100:+.1f}% |')
    lines += ['', 'A negative change means less time. Positive overhead means saving still costs more than asking again.',
        'Percentages compare the median times in the table.', '',
        'Change compares current saved time with previous saved time. Overhead compares current saved time with current fresh time.', '',
        '## Unchanged requests, CPU, memory and storage', '',
        'CPU time counts processor work in Python and the `fr` programs it starts. Elapsed time also includes waiting.',
        'CPU below sums worker and child CPU across the three questions; it excludes copy, edit and cleanup CPU.',
        'The worker samples CPU before its final report and answer writes.',
        'The raw GNU time values include those writes and round CPU to 0.01 seconds.',
        'The raw reports retain GNU time CPU, edit CPU and separate preparation measurements.',
        'Memory is the largest sampled total of resident memory across the worker and its child processes.',
        'This measure, RSS, counts memory present in RAM. Sampling can miss short peaks.',
        'Shared pages can count in more than one process.',
        'Stored bytes count file contents; directory and filesystem block allocation add unmeasured overhead.', '',
        'One KiB is 1,024 bytes. One MiB is 1,048,576 bytes.', '',
        '| Project / SDK | Saved reuse seconds | Saved request CPU seconds | Maximum memory MiB | Final saved KiB |',
        '|---|---:|---:|---:|---:|']
    for name, versions in report['summary'].items():
        for version, routes in versions.items():
            value = routes['saved']
            lines.append(f"| {name} / {version} | {value['median_reuse_seconds']:.3f} | {value['median_worker_cpu_seconds']:.3f} | {value['maximum_sampled_aggregate_rss_bytes'] / 1048576:.1f} | {value['median_final_saved_bytes'] / 1024:.1f} |")
    lines += ['', '## What this establishes', '',
        'The dedicated profile locates repeated hashing and also exposes the cost of flushing many small files to disk.',
        'For example, the original glob answer contains 1,387 distinct stored pieces.',
        'Each new file write waits for its bytes to reach disk before finishing. That storage cost remains after the hashing fix.',
        'Profile timings include profiler overhead and do not enter the timing tables above.',
        'The change preserves object contents, content addresses, verification and the existing resource limits.',
        'It adds no alternate reader, fallback or compatibility layer.', '',
        'Both measured routes use Python, so this experiment does not isolate Python overhead against direct command-line use.',
        'Three repetitions and uncontrolled operating-system file caches limit the precision of the timing estimates.',
        'The requests preserve bounded, missing and ambiguous answers. They establish no general agent, model-token or billing savings.', '',
        'All projects in this follow-up are familiar. The original first JMESPath evaluation remains separate and unchanged.',
        'The recurring checks audit that original result beside its original code and audit these measurements beside the current code.', '',
        'The next cost improvement should address the remaining measured storage and process costs and retain complete session comparisons.',
        'Broader agent-cost trials, translation and proofs connected to source code remain separate work.', '',
        f"[Matched measurement and raw archives](../{DIRECTORY.relative_to(ROOT)}/record.json) | "
        '[Original costs](saved-work-cost-results.md) | '
        '[Authenticated profiles and workflow steps](../tests/agent-eval/saved-answer-improvement/publication.json)', '']
    return '\n'.join(lines)


def retain_cache():
    spec = importlib.util.spec_from_file_location('scalar', ROOT / 'tools/retain-scalar-evidence.py')
    scalar = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(scalar)
    data, identity = scalar.collect(39559089792, 'refinement-evidence', 'refresh', os.environ['GITHUB_REPOSITORY'])
    scalar.verify_sources(ROOT, {identity['source_commit']})
    paths = scalar.report_paths(data, ['flow-cache'])
    relative, receipt = scalar.keep('flow-cache', {'result.json': data[paths['flow-cache']]},
                                     identity, '2026-10-21-saved-answer')
    subprocess.run(['git', 'add', '--', str(relative)], cwd=ROOT, check=True)
    cost.save(DIRECTORY / 'cache-collection.json', receipt)
    return relative, receipt


def retain_measurement(run_id):
    run = cost.api(f'actions/runs/{run_id}')
    authenticate(run_id, run['head_sha'])
    rows = cost.api(f'actions/runs/{run_id}/artifacts?per_page=100')['artifacts']
    artifact, = [row for row in rows if row['name'] == 'saved-answer-comparison']
    data = cost.artifact_bytes(artifact)
    directory = ROOT / f'tests/agent-eval/results/2026-10-21-saved-answer-{run_id}'
    cost.require(not directory.exists(), 'preserve existing matched measurements')
    with zipfile.ZipFile(io.BytesIO(data)) as archive:
        entries = archive.infolist()
        cost.require({entry.filename for entry in entries} == {'result.json', 'previous.zip', 'current.zip'}
                     and len(entries) == 3 and sum(entry.file_size for entry in entries) <= 16777216,
                     'unexpected measurement artifact inventory')
        directory.mkdir()
        for entry in entries:
            (directory / entry.filename).write_bytes(archive.read(entry))
    report = comparison.audit(directory)
    cost.require(report['source_commit'] == run['head_sha'] and report['run_id'] == run_id,
                 'measurement source differs from its producer')
    pointer = {'directory': str(directory.relative_to(ROOT)),
               'sha256': cost.file_digest(directory / 'result.json'), 'run_id': run_id}
    DIRECTORY.mkdir(parents=True, exist_ok=True)
    cost.save(DIRECTORY / 'record.json', pointer)
    cost.save(DIRECTORY / 'collection.json', {'run': run, 'artifact': artifact})
    return pointer, report


def retain_failed_attempts():
    directory = DIRECTORY / 'failures'
    directory.mkdir()
    cases = (
        (39560287457, 'Commit the checked reports', b'(fetch first)',
         'Another writer updated the branch before the checked reports could be pushed.'),
        (39559712773, 'Compare both SDKs with the same optimized native executable',
         b'SDK versions disagree or omit requests',
         'The two trials used different absolute project paths, which fr includes in project identities. '
         'The harness removed temporary raw results before packaging them; no raw measurements survived. '
         'The corrected trial reuses one explicit project path and retains raw results on failure.'))
    for run_id, failed_step, expected_error, cause in cases:
        run = cost.api(f'actions/runs/{run_id}')
        jobs = cost.api(f'actions/runs/{run_id}/jobs?per_page=100')['jobs']
        cost.require(run['status'] == 'completed' and run['conclusion'] == 'failure'
                     and any(step['name'] == failed_step and step['conclusion'] == 'failure'
                             for job in jobs for step in job['steps']), 'failed attempt changed')
        artifacts = cost.api(f'actions/runs/{run_id}/artifacts?per_page=100')['artifacts']
        cost.save(directory / f'{run_id}.json', {'run': run, 'jobs': jobs, 'cause': cause,
                                                'artifacts': artifacts})
        with tempfile.TemporaryFile() as output:
            subprocess.run(['gh', 'run', 'view', str(run_id), '--log-failed'],
                           stdout=output, stderr=subprocess.PIPE, check=True, timeout=30)
            output.seek(0)
            data = output.read(2097153)
        cost.require(len(data) <= 2097152 and expected_error in data,
                     'failure log does not confirm the recorded cause')
        (directory / f'{run_id}.log').write_bytes(data)


def main():
    cost.require(os.environ.get('GITHUB_ACTIONS') == 'true', 'publish on GitHub runners')
    cost.require(os.environ['GITHUB_REF'].startswith('refs/heads/')
                 and os.environ['GITHUB_REF'] != 'refs/heads/main', 'publish on the work branch')
    run_id = os.environ['COLLECTION_RUN']
    cost.require(run_id.isdigit() and int(run_id) > 0, 'declare the completed collection run')
    pointer, report = retain_measurement(int(run_id))
    retain_failed_attempts()
    run, jobs = authenticate(pointer['run_id'], report['source_commit'])
    names = {job['name'] for job in jobs if job['conclusion'] == 'success'}
    cost.require({'compare', 'profile', 'Original saved answer comparison'} <= names, 'incomplete comparison gate')
    before = retain_profile(39558069368, '8a0ac262ac17332feffb22a0ed61e864d920895b', 'profile-before')
    after = retain_profile(pointer['run_id'], report['source_commit'], 'profile-after')
    subprocess.run(['git', 'diff', '--exit-code', comparison.BASELINE,
                    '8a0ac262ac17332feffb22a0ed61e864d920895b', '--', 'sdk/python/src'], check=True)
    cost.save(DIRECTORY / 'publication.json', {'collection_run': run, 'jobs': jobs,
        'comparison_sha256': pointer['sha256'], 'before': before, 'after': after})
    subprocess.run(['rustup', 'toolchain', 'install', '1.99.1', '--profile', 'minimal', '--component', 'rustfmt'],
                   cwd=ROOT, check=True)
    subprocess.run(['python3', '-B', 'tools/retain-ci-reports.py', '--runs', json.dumps({
        'sdk-contexts': 39559075965, 'structural-change': 39559082672}),
        '--prefix', '2026-10-21-saved-answer'], cwd=ROOT, check=True)
    cache_path, cache_receipt = retain_cache()
    subprocess.run(['git', 'add', '--', 'tests/python_sdk.rs', 'tests/structural_change.rs',
                    *[str(path.relative_to(ROOT)) for path in
                      sorted((ROOT / 'tests/agent-eval/results').glob('2026-10-21-saved-answer-*'))]],
                   cwd=ROOT, check=True)
    catalog_path = ROOT / 'tests/agent-eval/roadmap.json'
    catalog = json.loads(catalog_path.read_text())
    cache, = [row for row in catalog['evidence'] if row['id'] == 'cache-measurements']
    cache.update(path=str(cache_path / 'result.json'), sha256=cache_receipt['files']['result.json'],
                 audit='python3 -B tools/flow-cache-acceptance.py --audit ' + str(cache_path / 'result.json'))
    identifier = 'current-saved-work-cost'
    cost.require(not any(row['id'] == identifier for row in catalog['evidence']), 'preserve previous publication')
    catalog['evidence'].append({'id': identifier, 'kind': 'measurement',
        'path': pointer['directory'] + '/result.json', 'sha256': pointer['sha256'],
        'audit': 'python3 -B tools/saved-answer-comparison.py --audit ' + pointer['directory'],
        'scope': 'Matched previous and current SDK sessions on three familiar projects; original first JMESPath qualification remains separate. Full session costs, fresh baselines and answer agreement.'})
    gate, = [row for row in catalog['obligations'] if row['id'] == 'C.performance']
    cost.require(gate['requires'][0]['evidence'] == 'saved-work-cost', 'review existing cost requirement')
    gate['requires'] = [{'evidence': identifier, 'assertions': [
        {'pointer': '/passed', 'equals': True}, {'pointer': '/projects', 'equals': 3},
        {'pointer': '/repetitions', 'equals': 3}, {'pointer': '/answer_agreement', 'equals': True},
        {'pointer': '/all_projects_familiar', 'equals': True},
        {'pointer': '/new_unread_claim', 'equals': False},
        {'pointer': '/original_unread/qualified', 'equals': True},
        {'pointer': '/original_unread/first_evaluation', 'equals': True}]}]
    gate['next'] = 'Use the matched SDK comparison to address remaining storage and process costs. Keep checking complete sessions and answer agreement.'
    cost.save(catalog_path, catalog)
    (ROOT / 'docs/saved-answer-improvement.md').write_text(markdown(report))
    spec = importlib.util.spec_from_file_location('prose', ROOT / 'tools/check-prose.py')
    prose = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(prose)
    counts = prose.gather()
    budget = prose.read_budget()
    for name, count in counts.items():
        cost.require(count <= budget[name], 'writing budget increased: ' + name)
    text = prose.BUDGET.read_text()
    for name, count in counts.items():
        text = text.replace(f'{name} {budget[name]}\n', f'{name} {count}\n')
    prose.BUDGET.write_text(text)


if __name__ == '__main__':
    main()
