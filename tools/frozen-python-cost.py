"""Check completed Python cost studies with their unchanged source and SDK."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
SOURCE_COMMIT = '8ca25c2d72484b7e66f49b279f8b5009c68f3d6e'
SNAPSHOT_SHA = ''
DIRECTORY = ROOT / 'tests/agent-eval/results/2026-10-19-python-cost-source'
OPTIMIZED = 'tests/agent-eval/results/2026-10-19-python-optimized-cost'
PROCESS = 'tests/agent-eval/results/2026-10-19-python-process-cost/result.json'
TOGETHER = 'tests/agent-eval/results/2026-10-19-python-together-cost/result.json'
REPORTS = tuple(f'{OPTIMIZED}/{name}/result.json' for name in (
    'cli-disabled', 'cli-enabled', 'profile-disabled', 'profile-enabled')) + (PROCESS, TOGETHER)
CHECKS = (
    ('tools/test-python-startup-cost.py',),
    ('tools/python-startup-cost.py', '--audit-set', OPTIMIZED),
    ('tools/test-python-process-cost.py',),
    ('tools/python-process-cost.py', '--audit', PROCESS),
    ('tools/test-python-together-cost.py',),
    ('tools/python-together-cost.py', '--audit', TOGETHER),
)
PATHS = ('src', 'grammars', 'crates', 'queries', 'build.rs', 'Cargo.toml', 'Cargo.lock',
         'native-source-inputs.txt', 'tools', 'sdk/python/src', 'sdk/python/pyproject.toml',
         '.github/workflows', 'tests/agent-eval/python-startup-cost',
         'tests/agent-eval/results/2026-10-19-python-together-cost/failed-audit.zip', *REPORTS)
spec = importlib.util.spec_from_file_location('frozen_utility', ROOT / 'tools/frozen-repository-flow.py')
frozen = importlib.util.module_from_spec(spec)
spec.loader.exec_module(frozen)


def require(condition, message):
    if not condition:
        raise ValueError(message)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def manifest(identity):
    return {'schema': 'fr-python-cost-history', 'source_commit': SOURCE_COMMIT,
            'snapshot': {'file': 'source.tar.gz', 'sha256': identity}, 'reports': list(REPORTS)}


def checked_snapshot(path, metadata, identity):
    require(len(identity) == 64 and all(char in '0123456789abcdef' for char in identity),
            'historical source snapshot is not pinned')
    require(path.stat().st_size <= 16 * 1024**2, 'historical source archive exceeds admission')
    data = path.read_bytes()
    require(digest(data) == identity, 'historical source archive changed')
    require(json.loads(metadata.read_bytes()) == manifest(identity), 'historical source manifest changed')
    return data


def check_published_reports(source, published, paths):
    for name in paths:
        require((source / name).read_bytes() == (published / name).read_bytes(),
                'published historical report changed: ' + name)


def audit(snapshot, metadata, identity, published):
    data = checked_snapshot(snapshot, metadata, identity)
    with tempfile.TemporaryDirectory(prefix='fr-python-cost-history-') as directory:
        source = Path(directory)
        frozen.unpack(data, source)
        check_published_reports(source, published, REPORTS)
        for check in CHECKS:
            script, *arguments = check
            if arguments:
                arguments[-1] = str(published / arguments[-1])
            result = subprocess.run([sys.executable, '-B', str(source / script), *arguments], cwd=source,
                                    env={**os.environ, 'PYTHONPATH': '', 'PYTHONDONTWRITEBYTECODE': '1'},
                                    capture_output=True, text=True, timeout=120)
            require(result.returncode == 0, script + '\n' + result.stdout[-4096:] + result.stderr[-8192:])
            print(script + ': ' + (result.stdout.strip() or result.stderr.strip()), flush=True)
    print('All six original reports match; original controls and source-bound audits passed.')


def retain():
    require(os.environ.get('GITHUB_ACTIONS') == 'true'
            and os.environ.get('GITHUB_REF', '').startswith('refs/heads/')
            and os.environ['GITHUB_REF'] != 'refs/heads/main', 'retain on a GitHub working branch')
    require(not DIRECTORY.exists() and SNAPSHOT_SHA == '', 'preserve the existing historical snapshot')
    data = subprocess.check_output(['git', 'archive', '--format=tar.gz', SOURCE_COMMIT, '--', *PATHS],
                                   cwd=ROOT, timeout=60)
    require(len(data) <= 16 * 1024**2, 'historical source archive exceeds admission')
    identity = digest(data)
    DIRECTORY.mkdir(parents=True)
    snapshot, metadata = DIRECTORY / 'source.tar.gz', DIRECTORY / 'manifest.json'
    snapshot.write_bytes(data)
    metadata.write_text(json.dumps(manifest(identity), indent=2) + '\n')
    audit(snapshot, metadata, identity, ROOT)
    path = Path(__file__)
    text = path.read_text()
    require(text.count("\nSNAPSHOT_SHA = ''\n") == 1, 'source snapshot identity already pinned')
    path.write_text(text.replace("\nSNAPSHOT_SHA = ''\n", f"\nSNAPSHOT_SHA = '{identity}'\n"))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument('--retain', action='store_true')
    mode.add_argument('--audit', action='store_true')
    args = parser.parse_args()
    if args.retain:
        retain()
    else:
        audit(DIRECTORY / 'source.tar.gz', DIRECTORY / 'manifest.json', SNAPSHOT_SHA, ROOT)


if __name__ == '__main__':
    main()
