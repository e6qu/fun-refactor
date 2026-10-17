#!/usr/bin/env python3
"""Retain audited scalar, compiler, index and repository artifacts on runners."""
import argparse
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path, PurePosixPath
import re
import subprocess
import zipfile

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('retain', ROOT / 'tools/retain-python-contract.py')
retain = importlib.util.module_from_spec(spec)
spec.loader.exec_module(retain)
require = retain.contract.require
FLOW = {
    'scalar': ['flow-fixed-point', 'recursive-flow', 'flow-facts'],
    'modules': ['imported-flow', 'package-flow', 'package-reexports', 'module-aliases'],
    'expressions': ['expression-control', 'call-binding', 'literal-defaults', 'namespace-packages', 'scalar-assignments'],
    'cache': ['flow-cache'],
}
SCRIPTS = {'flow-fixed-point': 'flow-acceptance', 'compiler-profile': 'compiler-profile',
           'index-resolution': 'index-resolution-acceptance', 'python-repositories': 'python-repository-acceptance',
           'fixed-arity-probes': 'python-fixed-arity-probes'}
SCRIPTS.update({group: group + '-acceptance' for groups in FLOW.values() for group in groups
                if group != 'flow-fixed-point'})
ANALYZER_PATHS = ('src', 'grammars', 'crates', 'queries', 'sdk/python/src',
                  'build.rs', 'Cargo.toml', 'Cargo.lock')


def verify_sources(root, revisions):
    require(revisions and all(isinstance(revision, str) and re.fullmatch(r'[0-9a-f]{40}', revision)
                              for revision in revisions), 'need recorded collection commits')
    dirty = subprocess.check_output(['git', 'status', '--porcelain', '--untracked-files=all',
                                     '--', *ANALYZER_PATHS], cwd=root, text=True, timeout=5)
    require(not dirty, 'current analyzer source must be clean')
    def trees(revision):
        return subprocess.check_output(['git', 'rev-parse', *[
            f'{revision}:{path}' for path in ANALYZER_PATHS]], cwd=root, text=True,
            stderr=subprocess.STDOUT, timeout=5).splitlines()
    expected = trees('HEAD')
    for revision in sorted(revisions):
        require(trees(revision) == expected, 'collection analyzer source differs from current code')


def audit_arguments(group, path):
    flag = '--verify' if group == 'index-resolution' else '--audit'
    return [f'tools/{SCRIPTS[group]}.py', flag, str(path)]


def files(archive, expected_digest):
    require('sha256:' + hashlib.sha256(archive).hexdigest() == expected_digest, 'artifact digest changed')
    with zipfile.ZipFile(io.BytesIO(archive)) as zipped:
        names = zipped.namelist()
        require(len(names) == len(set(names)) and len(names) <= 8192
                and sum(item.file_size for item in zipped.infolist()) <= 64 * 1024 * 1024,
                'artifact inventory exceeds admission')
        for name in names:
            path = PurePosixPath(name)
            require(path.parts and path.as_posix() == name
                    and not path.is_absolute() and '..' not in path.parts and '\\' not in name
                    and not name.endswith('/'), 'unsafe artifact path')
        return {name: zipped.read(name) for name in names}


def collect(run_id, artifact_name, producer, repository):
    run = retain.api(f'repos/{repository}/actions/runs/{run_id}')
    require(run['status'] == 'completed' and run['conclusion'] == 'success', 'wait for successful collection')
    artifacts = retain.api(f'repos/{repository}/actions/runs/{run_id}/artifacts?per_page=100')['artifacts']
    matching = [item for item in artifacts if item['name'] == artifact_name and not item['expired']]
    require(len(matching) == 1 and matching[0]['size_in_bytes'] <= 16 * 1024 * 1024,
            'need one bounded current artifact')
    artifact = matching[0]
    archive = retain.download_artifact(repository, artifact['id'])
    jobs = retain.api(f'repos/{repository}/actions/runs/{run_id}/jobs?filter=all&per_page=100')['jobs']
    identity = retain.collection(run, artifact, {'source_commit': run['head_sha']}, repository, jobs,
                                 artifact_name, producer)
    return files(archive, artifact['digest']), identity


def keep(group, data, identity, prefix):
    relative = Path('tests/agent-eval/results') / (prefix + '-' + group)
    destination = ROOT / relative
    require(not destination.exists(), 'preserve previous reports; choose a new directory')
    value = json.loads(data['result.json'])
    if group == 'python-repositories':
        require(value['runner']['revision'] == identity['source_commit'], 'repository collection source changed')
        require(set(data) == {'result.json', *value['artifacts']}, 'repository artifact inventory changed')
    elif group == 'fixed-arity-probes':
        require(value['source_commit'] == identity['source_commit']
                and set(data) == {'result.json', *[name + '.tar.gz' for name in value['snapshots']]},
                'upstream probe source or inventory changed')
    elif group != 'index-resolution':
        require(value.get('source_commit', value.get('repository_revision')) == identity['source_commit'],
                'report collection source changed')
    destination.mkdir()
    for name, content in data.items():
        file = destination / name
        file.parent.mkdir(parents=True, exist_ok=True)
        file.write_bytes(content)
    argument = destination if group == 'python-repositories' else destination / 'result.json'
    result = subprocess.run(['python3', '-B', *audit_arguments(group, argument)], cwd=ROOT,
                            capture_output=True, text=True, timeout=60)
    require(result.returncode == 0, f'{group} audit failed: {result.stderr[-4096:]}')
    print(f'Audited {group} from {identity["source_commit"]}', flush=True)
    return relative, {'collection': identity, 'files': {name: hashlib.sha256(content).hexdigest()
                                                       for name, content in data.items()}}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--runs', required=True, help='JSON object with eight hosted collection run IDs')
    parser.add_argument('--prefix', required=True)
    args = parser.parse_args()
    require(os.environ.get('GITHUB_ACTIONS') == 'true', 'retain evidence on GitHub runners')
    require(os.environ.get('GITHUB_REF', '').startswith('refs/heads/')
            and os.environ['GITHUB_REF'] != 'refs/heads/main', 'retain into a working branch')
    require(re.fullmatch(r'[a-zA-Z0-9][a-zA-Z0-9_-]{0,79}', args.prefix), 'supply a plain dated prefix')
    runs = json.loads(args.runs)
    require(set(runs) == {'flow', 'compiler', 'index', 'repositories', 'python', 'routes', 'repair', 'probes'}
            and all(type(run) is int and run > 0 for run in runs.values()), 'declare all required collections')
    repository = os.environ['GITHUB_REPOSITORY']
    collected = []
    for shard, groups in FLOW.items():
        data, identity = collect(runs['flow'], 'flow-contract-' + shard, 'scalar-evidence (' + shard + ')', repository)
        expected = {f'refinement-evidence/{args.prefix}-{group}/result.json' for group in groups}
        require({name for name in data if name.startswith('refinement-evidence/')} == expected
                and all(name in expected or name.startswith('refinement-refresh-logs/') for name in data),
                'scalar report inventory changed')
        for group in groups:
            collected.append((group, {'result.json': data[f'refinement-evidence/{args.prefix}-{group}/result.json']}, identity))
    for key, group, artifact, producer in [
        ('compiler', 'compiler-profile', 'compiler-profile', 'compiler-profile'),
        ('index', 'index-resolution', 'index-resolution-evidence', 'Revalidate exploration pages and index resolution'),
        ('repositories', 'python-repositories', 'roadmap-evidence', 'repositories'),
        ('probes', 'fixed-arity-probes', 'fixed-arity-probes', 'fixed-arity-probes'),
    ]:
        data, identity = collect(runs[key], artifact, producer, repository)
        if key == 'index':
            report = f'refinement-evidence/{args.prefix}-{group}/result.json'
            require({name for name in data if name.startswith('refinement-evidence/')} == {report}
                    and all(name == report or name.startswith('refinement-refresh-logs/') for name in data),
                    'index report inventory changed')
            data = {'result.json': data[report]}
        elif key == 'compiler':
            require(set(data) == {'result.json'}, 'compiler report inventory changed')
        collected.append((group, data, identity))
    source_commits = {identity['source_commit'] for _, _, identity in collected}
    for key in ('python', 'routes', 'repair'):
        run = retain.api(f'repos/{repository}/actions/runs/{runs[key]}')
        require(run['status'] == 'completed' and run['conclusion'] == 'success',
                'wait for the complete successful collection family')
        source_commits.add(run['head_sha'])
    for revision in sorted(source_commits):
        require(isinstance(revision, str) and re.fullmatch(r'[0-9a-f]{40}', revision),
                'need recorded collection commits')
        retain.command(['git', 'fetch', '--no-tags', '--depth=1', 'origin', revision], capture_output=True)
    verify_sources(ROOT, source_commits)
    receipts = {}
    catalog_path = ROOT / 'tests/agent-eval/roadmap.json'
    catalog = json.loads(catalog_path.read_text())
    for group, data, identity in collected:
        relative, receipt = keep(group, data, identity, args.prefix)
        receipts[group] = receipt
        for entry in catalog['evidence']:
            if Path(entry['path']).parent.name.endswith('-' + group):
                entry.update(path=str(relative / 'result.json'), sha256=receipt['files']['result.json'])
                argument = relative if group == 'python-repositories' else relative / 'result.json'
                entry['audit'] = 'python3 ' + ' '.join(audit_arguments(group, argument))
    catalog_path.write_text(json.dumps(catalog, indent=2, ensure_ascii=False) + '\n')
    directory = ROOT / 'tests/agent-eval/results' / (args.prefix + '-scalar-collection')
    require(not directory.exists(), 'preserve previous collection receipts')
    directory.mkdir()
    (directory / 'manifest.json').write_text(json.dumps({'schema': 'fr-scalar-collection',
                                                        'groups': receipts}, indent=2, sort_keys=True) + '\n')
    tests = ROOT / 'tools/test-compiler-profile.py'
    text, count = re.subn(r"^REPORT = ROOT / '[^']+'$",
                          f"REPORT = ROOT / 'tests/agent-eval/results/{args.prefix}-compiler-profile/result.json'",
                          tests.read_text(), flags=re.MULTILINE)
    require(count == 1, 'expected one active compiler profile test reference')
    tests.write_text(text)
    subprocess.run(['python3', '-B', 'tools/test-compiler-profile.py'], cwd=ROOT, check=True, timeout=120)
    retain.command(['python3', '-B', 'tools/roadmap-status.py', '--write'])
    retain.command(['python3', '-B', 'tools/roadmap-status.py', '--check'])
    subprocess.run(['python3', '-B', 'tools/retain-python-contract.py', '--run', str(runs['python']),
                    '--prefix', args.prefix], cwd=ROOT, check=True)
    subprocess.run(['python3', '-B', 'tools/retain-workflow-evidence.py', '--routes-run', str(runs['routes']),
                    '--repair-run', str(runs['repair']), '--prefix', args.prefix], cwd=ROOT, check=True)
    print(json.dumps({'retained_groups': sorted(receipts)}, sort_keys=True))


if __name__ == '__main__':
    main()
