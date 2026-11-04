"""Check the original speed comparison against its original committed checker."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
REVISION = '06294a686ef7ebe29987787bd2ef29b441ac0147'


def audit(revision):
    assert revision == REVISION
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
    assert all(not Path(name).is_absolute() and '..' not in Path(name).parts
               and not any(char in name for char in '\\!*?[]\n\r') for name in paths)
    with tempfile.TemporaryDirectory(prefix='fr-historical-index-evidence-') as temporary:
        original = Path(temporary) / 'source'
        subprocess.run(['git', 'worktree', 'add', '--no-checkout', '--detach', str(original), revision],
            cwd=ROOT, check=True, capture_output=True, timeout=60)
        try:
            subprocess.run(['git', 'sparse-checkout', 'set', '--no-cone', '--stdin'], cwd=original,
                input=''.join('/' + name + '\n' for name in sorted(paths)), text=True, check=True, timeout=60)
            subprocess.run(['git', 'checkout', '--force', '--detach', revision],
                cwd=original, check=True, capture_output=True, timeout=60)
            subprocess.run(['python3', '-B', 'tools/index-resolution-acceptance.py', '--verify', entry['path']],
                cwd=original, check=True, timeout=60)
        finally:
            subprocess.run(['git', 'worktree', 'remove', '--force', str(original)],
                cwd=ROOT, check=True, capture_output=True, timeout=60)
    print('Original indexing comparison checked against source ' + revision)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source-commit', required=True)
    audit(parser.parse_args().source_commit)
