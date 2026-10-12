#!/usr/bin/env python3
"""Replay the historical utility baseline against its retained source snapshot."""
import argparse
import atexit
import hashlib
import io
import json
import os
from pathlib import Path, PurePosixPath
import re
import subprocess
import sys
import tarfile
import tempfile

ROOT = Path(__file__).resolve().parents[1]
BASELINE = ROOT / 'tests/agent-eval/results/2026-10-12-utility-flow-repository-flow/result.json'
BASELINE_SHA = '191dd505876d18fe50a179951c32cdca1cf3e086a65a0113c6ac6bcb2c58fa00'
SOURCE_COMMIT = '80a0e47fe617c4fb6192a5cb200cb128c70f5a79'
AUDITOR_COMMIT = '1a940c5f3df0f71bc43f5e879d48946d3b5d14dc'
SNAPSHOT = ROOT / 'tests/agent-eval/results/2026-10-12-frozen-utility-source/source.tar.gz'
SNAPSHOT_SHA = '5013ad723e153b50d079c2412b9bea881fe40fc142b75c38b26f280ce6932b81'
_temporary = None


def require(condition, message):
    if not condition:
        raise ValueError(message)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def baseline():
    data = BASELINE.read_bytes()
    require(sha(data) == BASELINE_SHA, 'canonical historical baseline changed')
    return json.loads(data)


def unpack(data, root):
    require(len(data) <= 16 * 1024 * 1024, 'frozen source archive exceeds admission')
    seen, expanded = set(), 0
    with tarfile.open(fileobj=io.BytesIO(data), mode='r:gz') as archive:
        for member in archive:
            path = PurePosixPath(member.name)
            expanded += member.size
            require(path.parts and path.as_posix() == member.name.rstrip('/')
                    and not path.is_absolute() and '..' not in path.parts and member.name not in seen
                    and (member.isfile() or member.isdir()) and len(seen) < 8192
                    and expanded <= 64 * 1024 * 1024, 'unsafe frozen source member')
            seen.add(member.name)
            target = root / path
            if member.isdir():
                target.mkdir(parents=True, exist_ok=True)
            else:
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(archive.extractfile(member).read())


def source_root():
    global _temporary
    if _temporary is None:
        data = SNAPSHOT.read_bytes()
        require(SNAPSHOT_SHA and sha(data) == SNAPSHOT_SHA, 'frozen source archive digest changed')
        _temporary = tempfile.TemporaryDirectory(prefix='fr-frozen-utility-source-')
        atexit.register(_temporary.cleanup)
        unpack(data, Path(_temporary.name))
    return Path(_temporary.name)


def audit(path):
    value = json.loads(path.read_bytes())
    require(value['source_bindings'] == baseline()['source_bindings'], 'cohort source changed')
    require(all(item['source_commit'] == SOURCE_COMMIT for item in value['collections'].values()),
            'historical collection revision changed')
    root = source_root()
    result = subprocess.run([sys.executable, '-B', str(root / 'tools/retain-repository-flow.py'),
                             '--audit', str(path.resolve())], cwd=root, capture_output=True, text=True, timeout=120)
    require(result.returncode == 0, result.stderr[-8192:])
    return json.loads(result.stdout)


def retain():
    require(os.environ.get('GITHUB_ACTIONS') == 'true', 'retain frozen source on GitHub runners')
    require(os.environ.get('GITHUB_REF', '').startswith('refs/heads/')
            and os.environ['GITHUB_REF'] != 'refs/heads/main', 'retain into a working branch')
    canonical = baseline()
    paths = subprocess.check_output(['git', 'ls-tree', '-r', '--name-only', AUDITOR_COMMIT, '--',
                                     'tools', 'sdk/python/src', 'tests/agent-eval/python-repository-flow'],
                                    cwd=ROOT, text=True).splitlines()
    paths = sorted(set(paths) | set(canonical['source_bindings']))
    require(not SNAPSHOT.parent.exists(), 'preserve prior source snapshots')
    data = subprocess.check_output(['git', 'archive', '--format=tar.gz', AUDITOR_COMMIT, '--', *paths], cwd=ROOT)
    with tempfile.TemporaryDirectory() as directory:
        unpack(data, Path(directory))
    SNAPSHOT.parent.mkdir()
    SNAPSHOT.write_bytes(data)
    manifest = {'schema': 'fr-frozen-utility-source-1', 'source_commit': SOURCE_COMMIT,
                'auditor_commit': AUDITOR_COMMIT,
                'baseline_sha256': BASELINE_SHA, 'files': {'source.tar.gz': sha(data)}}
    SNAPSHOT.with_name('manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    path = Path(__file__)
    text = path.read_text()
    text, count = re.subn(r"^SNAPSHOT_SHA = ''$", f"SNAPSHOT_SHA = '{sha(data)}'", text, flags=re.MULTILINE)
    require(count == 1, 'frozen source identity already pinned')
    path.write_text(text)
    result = subprocess.run([sys.executable, '-B', str(path), '--audit', str(BASELINE)],
                            cwd=ROOT, capture_output=True, text=True, timeout=120)
    require(result.returncode == 0, result.stderr[-8192:])
    print(result.stdout, end='')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--retain', action='store_true')
    parser.add_argument('--audit', type=Path)
    args = parser.parse_args()
    if args.retain:
        retain()
    else:
        require(args.audit, 'choose retention or an offline audit')
        print(json.dumps(audit(args.audit), sort_keys=True))


if __name__ == '__main__':
    main()
