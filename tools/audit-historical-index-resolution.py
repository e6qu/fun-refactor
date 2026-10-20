"""Check the original speed comparison against its original committed checker."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tarfile
import tempfile

ROOT = Path(__file__).resolve().parents[1]


def audit(revision):
    assert revision == '06294a686ef7ebe29987787bd2ef29b441ac0147'
    subprocess.run(['git', 'fetch', '--no-tags', '--depth=1', 'origin', revision],
        cwd=ROOT, check=True, timeout=60, capture_output=True)
    def recorded(path):
        return subprocess.check_output(['git', 'show', revision + ':' + path], cwd=ROOT, timeout=30)
    catalog = json.loads(recorded('tests/agent-eval/roadmap.json'))
    entry, = [row for row in catalog['evidence'] if row['id'] == 'index-resolution']
    report_bytes = recorded(entry['path'])
    assert hashlib.sha256(report_bytes).hexdigest() == entry['sha256']
    report = json.loads(report_bytes)
    paths = set(report['bindings']) | {entry['path'], 'tests/agent-eval/index-resolution/baseline.json'}
    assert all(not Path(name).is_absolute() and '..' not in Path(name).parts for name in paths)
    with tempfile.TemporaryDirectory(prefix='fr-historical-index-evidence-') as temporary:
        work = Path(temporary)
        archive = work / 'source.tar'
        with archive.open('wb') as output:
            subprocess.run(['git', 'archive', revision, '--', *sorted(paths)], cwd=ROOT,
                check=True, stdout=output, timeout=60)
        with tarfile.open(archive) as source:
            source.extractall(work / 'source', filter='data')
        original = work / 'source'
        subprocess.run(['python3', '-B', 'tools/index-resolution-acceptance.py', '--verify', entry['path']],
            cwd=original, check=True, timeout=60)
    print('Original indexing comparison checked against source ' + revision)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source-commit', required=True)
    audit(parser.parse_args().source_commit)
