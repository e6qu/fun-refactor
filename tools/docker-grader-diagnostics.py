#!/usr/bin/env python3
"""Retain bounded host diagnostics when an isolated grader fails on GitHub."""
import argparse
import json
import os
from pathlib import Path
import shutil

from agent_eval.isolated_grade import DOCKER, invoke


COMMANDS = (
    (*DOCKER, 'version'),
    (*DOCKER, 'info'),
    ('sudo', '-n', 'journalctl', '-u', 'docker', '-u', 'containerd',
     '--since', '-5 minutes', '--lines', '160', '--no-pager'),
    ('ps', '-eo', 'stat,pid,ppid,comm,wchan:32'),
    ('free', '-b'),
)


def collect(output, *, execute=invoke):
    output.parent.mkdir(parents=True, exist_ok=True)
    disk = shutil.disk_usage(output.parent)
    report = {'load_average': os.getloadavg(), 'disk_bytes': dict(zip(('total', 'used', 'free'), disk)),
              'commands': []}
    for command in COMMANDS:
        process, stdout, stderr = execute(list(command), b'', output.parent, 5, 65536)
        report['commands'].append({'command': command, 'process': process,
            'stdout': stdout.decode('utf-8', errors='replace'),
            'stderr': stderr.decode('utf-8', errors='replace')})
        output.write_text(json.dumps(report, indent=2, sort_keys=True) + '\n')
    return report


if __name__ == '__main__':
    if os.environ.get('GITHUB_ACTIONS') != 'true':
        raise ValueError('collect Docker host diagnostics on GitHub runners')
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    collect(args.output)
