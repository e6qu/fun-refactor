"""Check the original unread package result against its original committed source."""
import argparse
import json
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
REVISION = '06294a686ef7ebe29987787bd2ef29b441ac0147'
FROZEN = 'd04c9301bb703eeaaddd06ecffc0cacd52f9fbd9'


def audit(revision):
    assert revision == REVISION
    for commit in (revision, FROZEN):
        subprocess.run(['git', 'fetch', '--no-tags', '--depth=1', 'origin', commit], cwd=ROOT,
            check=True, capture_output=True, timeout=60)
    with tempfile.TemporaryDirectory(prefix='fr-original-package-audit-') as temporary:
        source = Path(temporary) / 'original'
        subprocess.run(['git', 'worktree', 'add', '--detach', str(source), revision],
            cwd=ROOT, check=True, capture_output=True, timeout=60)
        try:
            subprocess.run(['git', 'sparse-checkout', 'disable'], cwd=source,
                check=True, capture_output=True, timeout=60)
            catalog = json.loads((source / 'tests/agent-eval/roadmap.json').read_text())
            entry, = [row for row in catalog['evidence'] if row['id'] == 'original-package-questions']
            subprocess.run(['python3', '-B', 'tools/test-package-milestone-audit.py', entry['path']],
                cwd=source, check=True, timeout=120)
        finally:
            subprocess.run(['git', 'worktree', 'remove', '--force', str(source)], cwd=ROOT,
                check=True, capture_output=True, timeout=60)
    print('Original unread package result checked against source ' + revision)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source-commit', required=True)
    audit(parser.parse_args().source_commit)
