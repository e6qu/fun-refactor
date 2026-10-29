"""Compare the previous and current SDK on complete, identical project sessions."""
import argparse
import importlib.util
import io
import json
import os
from pathlib import Path
import platform
import shutil
import statistics
import subprocess
import sys
import tarfile
import tempfile
import zipfile

ROOT = Path(__file__).resolve().parents[1]
BASELINE = 'b5d68215c9c428a00d03ca8f96bd8b3cfdd89b79'
spec = importlib.util.spec_from_file_location('cost', ROOT / 'tools/saved-work-cost.py')
cost = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cost)
EXTRA = ('tools/saved-answer-comparison.py', 'tools/profile-saved-answers.py',
         'tools/test-saved-answer-comparison.py', '.github/workflows/saved-answer-improvement.yml')


def bindings():
    return cost.bindings() | {name: cost.file_digest(ROOT / name) for name in EXTRA}


def baseline_tree(destination):
    data = subprocess.check_output(['git', 'archive', BASELINE, 'sdk/python/src',
                                    'tools/saved-work-worker.py'])
    with tarfile.open(fileobj=io.BytesIO(data)) as archive:
        for entry in archive.getmembers():
            path = Path(entry.name)
            cost.require(not path.is_absolute() and '..' not in path.parts
                         and (entry.isfile() or entry.isdir()), 'unsafe baseline source')
            target = destination / path
            if entry.isdir():
                target.mkdir(parents=True, exist_ok=True)
            else:
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(archive.extractfile(entry).read())
    expected = cost.file_digest(ROOT / 'tools/saved-work-worker.py')
    cost.require(cost.file_digest(destination / 'tools/saved-work-worker.py') == expected,
                 'comparison must use identical worker code')
    return cost.inventory(destination)


def archive_tree(directory, destination):
    cost.save(directory / 'inventory.json', cost.inventory(directory))
    with zipfile.ZipFile(destination, 'w', compression=zipfile.ZIP_DEFLATED) as archive:
        for path in sorted(directory.rglob('*')):
            if path.is_file():
                archive.write(path, str(path.relative_to(directory)))


def unpack(source, destination):
    with zipfile.ZipFile(source) as archive:
        entries, names, total = archive.infolist(), set(), 0
        for entry in entries:
            path = Path(entry.filename)
            total += entry.file_size
            cost.require(not path.is_absolute() and '..' not in path.parts and entry.filename not in names
                         and not entry.is_dir() and entry.file_size <= 2097152
                         and (entry.external_attr >> 16) & 0o170000 in (0, 0o100000)
                         and total <= 67108864 and len(names) < 8192, 'unsafe comparison archive')
            names.add(entry.filename)
            target = destination / path
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(archive.read(entry))
    inventory = json.loads((destination / 'inventory.json').read_text())
    cost.require(inventory == {name: digest for name, digest in cost.inventory(destination).items()
                              if name != 'inventory.json'}, 'raw comparison inventory differs')


def compare_answers(projects):
    old, new = projects['previous'], projects['current']
    cost.require(len(old) == len(new) == 3, 'missing project comparison')
    for before, after in zip(old, new):
        cost.require(before['project'] == after['project']
                     and before['workspace'] == after['workspace']
                     and before['source_files'] == after['source_files']
                     and before['changed_files'] == after['changed_files'], 'project inputs differ')
        answers = lambda project: {(session['repetition'], session['condition'], row['stage']): row['answer_digest']
            for session in project['sessions'] for row in session['stages']}
        cost.require(answers(before) == answers(after) and len(answers(before)) == 18,
                     'SDK versions disagree or omit requests')


def audit_project_sdk(directory, sdk):
    # Each archive is checked by its declared SDK, in a separate process.
    subprocess.run([sys.executable, '-B', str(Path(__file__).resolve()),
                    '--audit-project', str(directory), '--sdk', str(sdk)], check=True)


def summary(projects):
    result = {}
    for project_index, old in enumerate(projects['previous']):
        name = old['project']['id']
        result[name] = {}
        for version in ('previous', 'current'):
            project = projects[version][project_index]
            result[name][version] = {}
            for condition in ('saved', 'fresh'):
                sessions = [session for session in project['sessions'] if session['condition'] == condition]
                stages = [row for session in sessions for row in session['stages']]
                memory = [row['sampled_aggregate_rss_bytes'] for row in stages
                          if row['sampled_aggregate_rss_bytes'] is not None]
                result[name][version][condition] = {
                    'median_session_seconds': statistics.median(session['total_seconds'] for session in sessions),
                    'median_reuse_seconds': statistics.median(row['seconds'] for row in stages if row['stage'] == 'reuse'),
                    'median_worker_cpu_seconds': statistics.median(sum(row['worker_cpu_seconds']
                        for row in session['stages']) for session in sessions),
                    'maximum_sampled_aggregate_rss_bytes': max(memory) if memory else None,
                    'observed_memory_stages': len(memory),
                    'unobserved_memory_stages': len(stages) - len(memory),
                    'source_edits_without_memory_sample': sum(
                        session['source_edit_rss_observation'] == 'unobserved' for session in sessions),
                    'median_final_saved_bytes': statistics.median(session['stages'][-1]['storage_after']['saved_state']
                        for session in sessions)}
    return result


def original_reference():
    record = json.loads((ROOT / 'tests/agent-eval/saved-work-cost/record.json').read_text())
    path = ROOT / record['directory'] / 'result.json'
    cost.require(cost.file_digest(path) == record['sha256'], 'original measurement changed')
    original = json.loads(path.read_text())
    cost.require(original['passed'] is True and original['reserved']['first_evaluation'] is True,
                 'original reserved evaluation did not pass')
    return {'path': str(path.relative_to(ROOT)), 'sha256': record['sha256'],
            'source_commit': original['source_commit'], 'qualified': True,
            'first_evaluation': True, 'run_id': original['reserved']['run_id']}


def audit(directory):
    report = json.loads((directory / 'result.json').read_text())
    cost.require(report['schema'] == 'fr-saved-answer-improvement' and report['passed'] is True
                 and report['all_projects_familiar'] is True and report['new_unread_claim'] is False,
                 'incorrect completion or unfamiliar-project claim')
    cost.require(report['source_bindings'] == bindings() and report['tasks'] == json.loads(cost.TASKS.read_text()),
                 'comparison procedure changed')
    cost.require(report['baseline_source_commit'] == BASELINE and report['original_unread'] == original_reference(),
                 'comparison baseline or original qualification changed')
    cost.require(report['projects'] == 3 and report['repetitions'] == 3 and report['answer_agreement'] is True,
                 'missing comparisons')
    projects = {}
    with tempfile.TemporaryDirectory(prefix='fr-answer-audit-') as temporary:
        root = Path(temporary)
        baseline_sources = baseline_tree(root / 'baseline')
        cost.require(report['baseline_source_files'] == baseline_sources, 'baseline SDK differs')
        cost.require(report['files'] == {name: cost.file_digest(directory / name)
                     for name in ('previous.zip', 'current.zip')}, 'raw measurement changed')
        for version in ('previous', 'current'):
            tree = root / version
            unpack(directory / (version + '.zip'), tree)
            projects[version] = []
            for task in report['tasks']['projects']:
                value = json.loads((tree / task['id'] / 'result.json').read_text())
                sdk = root / 'baseline/sdk/python/src' if version == 'previous' else ROOT / 'sdk/python/src'
                audit_project_sdk(tree / task['id'], sdk)
                for session in value['sessions']:
                    for row in session['stages']:
                        process = json.loads((tree / task['id'] / row['path'] / 'process.json').read_text())
                        argv = process['argv']
                        cost.require(argv[argv.index('--project') + 1] == value['workspace'],
                                     'request used a different project path')
                projects[version].append(value)
        compare_answers(projects)
        cost.require(report['summary'] == summary(projects), 'summary differs from full session costs')
    return report


def collect(binary, output):
    cost.require(os.environ.get('GITHUB_ACTIONS') == 'true', 'collect on GitHub runners')
    output.mkdir(parents=True, exist_ok=False)
    tasks, before = json.loads(cost.TASKS.read_text()), bindings()
    projects = {'previous': [], 'current': []}
    with tempfile.TemporaryDirectory(prefix='fr-answer-comparison-') as temporary:
        root = Path(temporary)
        workspace = root / 'workspace'
        workspace.mkdir()
        baseline_sources = baseline_tree(root / 'baseline')
        for index, task in enumerate(tasks['projects']):
            versions = ('previous', 'current') if index % 2 == 0 else ('current', 'previous')
            values = {}
            for version in versions:
                cost.WORKER = (root / 'baseline' if version == 'previous' else ROOT) / 'tools/saved-work-worker.py'
                directory = output / 'raw' / version / task['id']
                value = cost.collect_project(task, directory, binary, tasks, workspace)
                sdk = root / 'baseline/sdk/python/src' if version == 'previous' else ROOT / 'sdk/python/src'
                audit_project_sdk(directory, sdk)
                values[version] = value
            for version in versions:
                projects[version].append(values[version])
        cost.WORKER = ROOT / 'tools/saved-work-worker.py'
        compare_answers(projects)
        for version in ('previous', 'current'):
            archive_tree(output / 'raw' / version, output / (version + '.zip'))
        shutil.rmtree(output / 'raw')
    cost.require(bindings() == before, 'collection changed its source')
    cost.save(output / 'result.json', {
        'schema': 'fr-saved-answer-improvement', 'passed': True,
        'source_commit': os.environ['GITHUB_SHA'], 'source_bindings': before,
        'run_id': int(os.environ['GITHUB_RUN_ID']), 'baseline_source_commit': BASELINE,
        'baseline_source_files': baseline_sources, 'original_unread': original_reference(),
        'all_projects_familiar': True, 'new_unread_claim': False,
        'projects': 3, 'repetitions': 3, 'answer_agreement': True,
        'tasks': tasks, 'binary_sha256': cost.file_digest(binary),
        'environment': {'python': platform.python_version(), 'platform': platform.platform(),
                        'rustc': subprocess.check_output(['rustc', '--version'], text=True).strip(),
                        'build': cost.BUILD},
        'summary': summary(projects),
        'files': {name: cost.file_digest(output / name) for name in ('previous.zip', 'current.zip')}})
    audit(output)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument('--collect', type=Path)
    mode.add_argument('--audit', type=Path)
    mode.add_argument('--audit-project', type=Path)
    parser.add_argument('--sdk', type=Path)
    parser.add_argument('--fr', type=Path)
    args = parser.parse_args()
    if args.audit_project:
        cost.require(args.sdk is not None, 'project audit requires an explicit SDK path')
        tasks = json.loads(cost.TASKS.read_text())
        sys.path.insert(0, str(args.sdk.resolve()))
        import fr_ir.context
        cost.require(Path(fr_ir.context.__file__).resolve().is_relative_to(args.sdk.resolve()),
                     'audit loaded a different SDK')
        cost.audit_project(json.loads((args.audit_project / 'result.json').read_text()),
                           args.audit_project, tasks)
    elif args.audit:
        print(json.dumps(audit(args.audit)['summary'], indent=2))
    else:
        cost.require(args.fr is not None, 'collection requires the explicit optimized native binary')
        collect(args.fr.resolve(), args.collect)
