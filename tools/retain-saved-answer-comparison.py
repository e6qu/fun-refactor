"""Publish authenticated batch-storage costs without replacing earlier measurements."""
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import zipfile

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('comparison', ROOT / 'tools/saved-answer-comparison.py')
comparison = importlib.util.module_from_spec(spec)
spec.loader.exec_module(comparison)
cost = comparison.cost
DIRECTORY = ROOT / 'tests/agent-eval/batched-answer-storage'
BEFORE_RUN = 39561749625
BEFORE_SOURCE = 'f9abf411e5a7d59b480ab60dde80824715d287d1'


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
    lines = ['# Save an answer with one database transaction', '',
        'A user asks where `glob_with` appears and which code a change could affect.',
        'The Python helper library, called the SDK, can save that answer and the inputs it depends on.',
        'A content fingerprint identifies each distinct piece of the answer by its bytes.',
        'Previously, saving the glob answer wrote 1,387 files and waited for each write to reach disk.',
        'The local store now puts the pieces in one SQLite database inside the chosen directory.',
        'SQLite is a database library included with Python; it needs no separate server.',
        'A transaction commits the pieces together. If it fails, none of its changes become visible.',
        'Disk synchronization stays enabled. Conflicting bytes and corrupted records still fail verification.', '',
        '## What is compared', '',
        'The previous SDK is the code merged in PR #554. Both SDKs use the same optimized `fr` build on the same runner.',
        'Both ask identical questions on pinned glob, Boltons and JMESPath sources at the same absolute project path.',
        'glob matches file paths in Rust. Boltons provides Python utilities. JMESPath selects values from JSON documents.',
        'Each project and SDK has three sessions containing first use, unchanged reuse and one source edit.',
        'Each question starts a new Python process. Both approaches keep their own initially empty native analysis cache.',
        'The saved approach also keeps its answer and a task plan, which records the inputs needed for the answer to remain valid.',
        'The fresh approach asks `fr` again without saving a task plan or answer.',
        'After an edit, the saved approach checks those inputs, expires the old answer and repeats discovery.',
        'The edit uses `fr file replace` in one file. For example, glob replaces `glob_with` with `glob_with_cost_renamed`.',
        'It then asks about the old name. This checks whether an answer expires; it does not deliver a complete project rename.',
        'SDK order alternates by project. Saved and fresh order alternates by repetition within each SDK.',
        'Each SDK finishes its three repetitions on a project before the other SDK starts that project.',
        'All 108 request answers agree across SDKs, including missing, ambiguous and bounded results.', '',
        '## Complete session time', '',
        'Median seconds include Python startup, validation, discovery, saving, output, source copies, the edit and cleanup.',
        'Builds, dependency installation and initial downloads remain separate in raw reports and workflow steps.', '',
        '| Project | Previous saved | Current saved | Change | Previous fresh | Current fresh | Current saved overhead versus fresh |',
        '|---|---:|---:|---:|---:|---:|---:|']
    for name, versions in report['summary'].items():
        old = versions['previous']['saved']['median_session_seconds']
        new = versions['current']['saved']['median_session_seconds']
        old_fresh = versions['previous']['fresh']['median_session_seconds']
        fresh = versions['current']['fresh']['median_session_seconds']
        lines.append(f'| {name} | {old:.3f} | {new:.3f} | {(new / old - 1) * 100:+.1f}% | {old_fresh:.3f} | {fresh:.3f} | {(new / fresh - 1) * 100:+.1f}% |')
    lines += ['', 'Negative change means less time than the previous saved approach.',
        'Positive overhead means saving costs more than asking again with the current SDK.',
        'Percentages compare the median complete session times above.', '',
        '## CPU, memory and saved bytes', '',
        'CPU counts processor work in Python and its `fr` child processes. Elapsed time also includes waiting.',
        'Request CPU sums the three questions, sampled before the final report and answer writes.',
        'Raw GNU time values include those writes and round CPU to 0.01 seconds. Copy, edit and cleanup CPU remain separate.',
        'Memory is the largest sampled sum of resident memory across the worker and its children.',
        'This measure, RSS, counts memory present in RAM. Sampling can miss short peaks, and shared pages can count twice.',
        'Saved bytes include the database or record files and the task receipt, including the earlier answer retained after editing.',
        'Filesystem block allocation and directory entries add unmeasured overhead.',
        'The database uses more stored bytes in these trials because it also keeps pages and an index.',
        'The index maps each content fingerprint to its stored piece.',
        'The memory samples include the database library. These results establish no memory saving.',
        'One KiB is 1,024 bytes. One MiB is 1,048,576 bytes.', '',
        '| Project / SDK | Unchanged reuse seconds | Request CPU seconds | Maximum memory MiB | Final saved KiB |',
        '|---|---:|---:|---:|---:|']
    for name, versions in report['summary'].items():
        for version in ('previous', 'current'):
            value = versions[version]['saved']
            lines.append(f"| {name} / {version} | {value['median_reuse_seconds']:.3f} | {value['median_worker_cpu_seconds']:.3f} | {value['maximum_sampled_aggregate_rss_bytes'] / 1048576:.1f} | {value['median_final_saved_bytes'] / 1024:.1f} |")
    lines += ['', '## What the checks establish', '',
        'Separate profiles locate storage work. Their timings include profiler overhead and do not enter these tables.',
        'Checks cover reopening, concurrent writers, conflicting bytes, corrupted records and a writer killed before commit.',
        'Each SDK audits its own retained state in a separate process beside its declared source.',
        'The application has one current local format and no older-format reader or fallback.',
        'Record contents, fingerprints, verification and request resource limits stay the same.',
        'HTTP stores publish pieces in order; the remote service owns transaction support and durability.', '',
        'Both approaches use Python. This experiment does not isolate Python overhead against direct command-line use.',
        'Three repetitions and uncontrolled operating-system file caches limit the precision of timing estimates.',
        'These scripted requests establish no general agent, model-token or billing savings.',
        'All projects are familiar. The original first JMESPath result remains unchanged and separately audited.',
        'The earlier hashing comparison remains unchanged and is audited beside the code merged in PR #554.', '',
        '[Measurements and raw archives](../tests/agent-eval/batched-answer-storage/record.json) | '
        '[Authenticated profiles and workflow steps](../tests/agent-eval/batched-answer-storage/publication.json) | '
        '[Earlier hashing comparison](saved-answer-improvement.md)', '']
    return '\n'.join(lines)


def retain_measurement(run_id):
    run = cost.api(f'actions/runs/{run_id}')
    authenticate(run_id, run['head_sha'])
    rows = cost.api(f'actions/runs/{run_id}/artifacts?per_page=100')['artifacts']
    artifact, = [row for row in rows if row['name'] == 'saved-answer-comparison']
    data = cost.artifact_bytes(artifact)
    directory = ROOT / f'tests/agent-eval/results/2026-10-21-batched-answers-{run_id}'
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
    cost.require(not DIRECTORY.exists(), 'preserve the completed storage publication')
    DIRECTORY.mkdir(parents=True)
    pointer = {'directory': str(directory.relative_to(ROOT)),
               'sha256': cost.file_digest(directory / 'result.json'), 'run_id': run_id}
    cost.save(DIRECTORY / 'record.json', pointer)
    cost.save(DIRECTORY / 'collection.json', {'run': run, 'artifact': artifact})
    subprocess.run(['git', 'add', '--', str(directory.relative_to(ROOT))], cwd=ROOT, check=True)
    return pointer, report


def main():
    cost.require(os.environ.get('GITHUB_ACTIONS') == 'true', 'publish on GitHub runners')
    cost.require(os.environ['GITHUB_REF'].startswith('refs/heads/')
                 and os.environ['GITHUB_REF'] != 'refs/heads/main', 'publish on the work branch')
    run_id = os.environ['COLLECTION_RUN']
    cost.require(run_id.isdigit() and int(run_id) > 0, 'declare the completed collection run')
    pointer, report = retain_measurement(int(run_id))
    run, jobs = authenticate(pointer['run_id'], report['source_commit'])
    names = {job['name'] for job in jobs if job['conclusion'] == 'success'}
    cost.require({'compare', 'profile', 'Original saved answer comparison'} <= names, 'incomplete comparison gate')
    before = retain_profile(BEFORE_RUN, BEFORE_SOURCE, 'profile-before')
    after = retain_profile(pointer['run_id'], report['source_commit'], 'profile-after')
    subprocess.run(['git', 'fetch', '--no-tags', '--depth=1', 'origin', BEFORE_SOURCE], check=True)
    subprocess.run(['git', 'diff', '--exit-code', comparison.BASELINE,
                    BEFORE_SOURCE, '--', 'sdk/python/src', 'tools/profile-saved-answers.py'], check=True)
    cost.save(DIRECTORY / 'publication.json', {'collection_run': run, 'jobs': jobs,
        'comparison_sha256': pointer['sha256'], 'before': before, 'after': after})
    catalog_path = ROOT / 'tests/agent-eval/roadmap.json'
    catalog = json.loads(catalog_path.read_text())
    entry, = [row for row in catalog['evidence'] if row['id'] == 'current-saved-work-cost']
    cost.require(entry['path'] == 'tests/agent-eval/results/2026-10-21-saved-answer-39561749625/result.json',
                 'review the previous current cost record before replacing its selection')
    entry.update(path=pointer['directory'] + '/result.json', sha256=pointer['sha256'],
                 audit='python3 -B tools/saved-answer-comparison.py --audit ' + pointer['directory'],
                 scope='Matched complete sessions before and after batch storage on three familiar projects. '
                       'Includes fresh baselines, full answer agreement and unchanged first-project qualification.')
    cost.save(catalog_path, catalog)
    (ROOT / 'docs/batched-answer-storage.md').write_text(markdown(report))


if __name__ == '__main__':
    main()
