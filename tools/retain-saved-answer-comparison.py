"""Publish authenticated hashing costs without replacing earlier measurements."""
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
DIRECTORY = ROOT / 'tests/agent-eval/distinct-answer-hashing'
BEFORE_RUN = 39612376706
BEFORE_SOURCE = '3e4649c2286b801cf2ab0f9cd65d51a8cfd0c3f8'


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
    lines = ['# Hash each distinct answer piece once', '',
        'A user asks where `glob_with` appears and which code a change could affect.',
        'The Python helper library, called the SDK, can save that answer and the inputs it depends on.',
        'A content fingerprint is a label computed from a saved piece\'s contents. Computing that label is called hashing.',
        'JSON is the text format used to save those pieces, including numbers, lists and named fields.',
        'For example, the same function name can appear in many parts of an answer.',
        'Previously, packing the glob answer visited 11,210 pieces and hashed every visit, although only 1,387 pieces were distinct.',
        'The SDK now remembers fingerprints within each operation. Equal pieces share a fingerprint without repeating that hash.',
        'This includes separately created lists and objects with equal contents, not just repeated references to one Python object.',
        'The SDK still visits every input entry. It skips the repeated fingerprint computation.',
        'The temporary lookup ends with the operation. Editing an input before the next call produces a new fingerprint.',
        'Stored pieces, their fingerprints and corruption checks remain identical.', '',
        '## What is compared', '',
        'The previous SDK is the code merged in PR #555, which already writes an answer\'s pieces together into one database.',
        'Both SDKs call the same compiled `fr` program, built with optimizations, on the same runner.',
        'Both ask identical questions on fixed versions of glob, Boltons and JMESPath at the same absolute project path.',
        'glob matches file paths in Rust. Boltons provides Python utilities. JMESPath selects values from JSON documents.',
        'Each project and SDK has three sessions containing first use, unchanged reuse and one source edit.',
        'Each question starts a new Python process.',
        'Both approaches let the compiled `fr` program keep its own saved analysis between questions, starting empty.',
        'The saved approach also keeps its answer and a task plan, which records the inputs needed for the answer to remain valid.',
        'The fresh approach asks `fr` again without saving a task plan or answer.',
        'After an edit, the saved approach checks those inputs, expires the old answer and repeats discovery.',
        'The edit uses `fr file replace` in one file. For example, glob replaces `glob_with` with `glob_with_cost_renamed`.',
        'It then asks about the old name. This checks whether an answer expires; it does not deliver a complete project rename.',
        'SDK order alternates by project. Saved and fresh order alternates by repetition within each SDK.',
        'Each SDK finishes its three repetitions on a project before the other SDK starts that project.',
        'All 108 request answers agree across SDKs, including missing names, ambiguous matches and answers that stop at their work limit.', '',
        '## Complete session time', '',
        'Each table uses three trials. The median is the middle value after sorting those three measurements.',
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
        'CPU time measures processor work in Python and any `fr` programs it starts. Elapsed time also includes waiting.',
        'Request CPU sums the three questions, sampled before the final report and answer writes.',
        'The GNU `time` command also measures those writes and rounds processor time to 0.01 seconds.',
        'Processor work for copying, editing and cleanup remains separate.',
        'Memory is the largest sampled sum of resident memory across Python and the programs it starts.',
        'This measure, RSS, counts memory present in RAM. Sampling can miss short peaks, and shared pages can count twice.',
        'Saved bytes include the database and saved task record, including the earlier answer retained after editing.',
        'Filesystem block allocation and directory entries add unmeasured overhead.',
        'Both SDKs use the same database format. This change is intended to reduce repeated computation, not stored bytes.',
        'The temporary lookup adds one entry per distinct piece while hashing. The memory samples include that working space.',
        'The memory samples include the database library. These results establish no memory saving.',
        'One KiB is 1,024 bytes. One MiB is 1,048,576 bytes.', '',
        '| Project / SDK | Unchanged reuse seconds | Request CPU seconds | Maximum memory MiB | Final saved KiB |',
        '|---|---:|---:|---:|---:|']
    for name, versions in report['summary'].items():
        for version in ('previous', 'current'):
            value = versions[version]['saved']
            lines.append(f"| {name} / {version} | {value['median_reuse_seconds']:.3f} | {value['median_worker_cpu_seconds']:.3f} | {value['maximum_sampled_aggregate_rss_bytes'] / 1048576:.1f} | {value['median_final_saved_bytes'] / 1024:.1f} |")
    lines += ['', '## What the checks establish', '',
        'Separate profiles locate hashing work. Their timings include profiler overhead and do not enter these tables.',
        'Tests compare fingerprints with an independent implementation and the compiled `fr` program.',
        'Tests also check one fingerprint computation per distinct piece and changed fingerprints after later edits.',
        'A list containing itself and numbers such as infinity cannot form valid JSON inputs; both fail visibly.',
        'Integers, decimal numbers, booleans and signed zero remain distinct.',
        'For example, `1`, `1.0` and `true` cannot share a stored piece. The fingerprints for `0.0` and `-0.0` also differ.',
        'Checks cover reopening, concurrent writers, conflicting bytes, corrupted records and a writer killed before commit.',
        'Python considers `True == 1` true. Both stores compare the saved JSON bytes, so these values cannot overwrite one another under the same fingerprint.',
        'Each SDK audits its own retained state in a separate process beside its declared source.',
        'The application has one current local format and no older-format reader or fallback.',
        'Record contents, fingerprints, verification and request resource limits stay the same.',
        '',
        'Both approaches use Python. This experiment does not isolate Python overhead against direct command-line use.',
        'Three repetitions and uncontrolled operating-system file caches limit the precision of timing estimates.',
        'These scripted requests establish no general agent, model-token or billing savings.',
        'All projects are familiar. The original first JMESPath result remains unchanged and separately audited.',
        'The earlier storage and hashing comparisons remain unchanged and are audited beside their measured code.', '',
        '[Measurements and raw archives](../tests/agent-eval/distinct-answer-hashing/record.json) | '
        '[Authenticated profiles and workflow steps](../tests/agent-eval/distinct-answer-hashing/publication.json) | '
        '[Previous storage comparison](batched-answer-storage.md) | '
        '[Earlier hashing comparison](saved-answer-improvement.md)', '']
    return '\n'.join(lines)


def retain_measurement(run_id):
    run = cost.api(f'actions/runs/{run_id}')
    authenticate(run_id, run['head_sha'])
    rows = cost.api(f'actions/runs/{run_id}/artifacts?per_page=100')['artifacts']
    artifact, = [row for row in rows if row['name'] == 'saved-answer-comparison']
    data = cost.artifact_bytes(artifact)
    directory = ROOT / f'tests/agent-eval/results/2026-10-21-distinct-answers-{run_id}'
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
    cost.require(not DIRECTORY.exists(), 'preserve the completed hashing publication')
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
    cost.require(entry['path'] == 'tests/agent-eval/results/2026-10-21-batched-answers-39612376706/result.json',
                 'review the previous current cost record before replacing its selection')
    entry.update(path=pointer['directory'] + '/result.json', sha256=pointer['sha256'],
                 audit='python3 -B tools/saved-answer-comparison.py --audit ' + pointer['directory'],
                 scope='Matched complete sessions before and after hashing distinct pieces once on three familiar projects. '
                       'Includes fresh baselines, full answer agreement and unchanged first-project qualification.')
    cost.save(catalog_path, catalog)
    (ROOT / 'docs/distinct-answer-hashing.md').write_text(markdown(report))


if __name__ == '__main__':
    main()
