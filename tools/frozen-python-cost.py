"""Check completed Python cost studies with their unchanged source and SDK."""
import argparse
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
SOURCE_COMMIT = '8ca25c2d72484b7e66f49b279f8b5009c68f3d6e'
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


def require(condition, message):
    if not condition:
        raise ValueError(message)


def check_published_reports(source, published, paths):
    for name in paths:
        require((source / name).read_bytes() == (published / name).read_bytes(),
                'published historical report changed: ' + name)


def check_source(source):
    revision = subprocess.check_output(['git', '-C', str(source), 'rev-parse', 'HEAD'],
                                       text=True, timeout=30).strip()
    require(revision == SOURCE_COMMIT, 'historical source revision changed')
    status = subprocess.check_output(['git', '-C', str(source), 'status', '--porcelain',
                                     '--untracked-files=all'], text=True, timeout=30)
    require(not status, 'historical source checkout changed')
    require(source.resolve() != ROOT, 'historical checks need a separate source checkout')


def audit(source, published):
    check_source(source)
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
    check_source(source)
    print('All six original reports match; original controls and source-bound audits passed.')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', type=Path, required=True)
    args = parser.parse_args()
    audit(args.source.resolve(), ROOT)


if __name__ == '__main__':
    main()
