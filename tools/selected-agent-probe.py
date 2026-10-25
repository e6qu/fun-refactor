#!/usr/bin/env python3
"""Inspect the pinned M1 clients on a runner without credentials or inference."""
import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tarfile
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
CLIENTS = ROOT / 'tests/agent-eval/selected-agents/clients.json'
ARCHIVE_BYTES = 256 * 1024**2
OUTPUT_BYTES = 1024**2


def install(entry, destination):
    destination.mkdir(parents=True, exist_ok=False)
    archive = destination / 'client.tar.gz'
    with urllib.request.urlopen(entry['url'], timeout=30) as source, archive.open('xb') as output:
        size = 0
        while block := source.read(1024**2):
            size += len(block)
            if size > ARCHIVE_BYTES:
                raise ValueError('client archive exceeds 256 MiB')
            output.write(block)
    identity = entry['digest']
    with archive.open('rb') as source:
        checksum = hashlib.file_digest(source, identity['algorithm'])
    if identity['encoding'] == 'hex':
        actual = checksum.hexdigest()
    elif identity['encoding'] == 'base64':
        actual = base64.b64encode(checksum.digest()).decode()
    else:
        raise ValueError('unknown digest encoding')
    if actual != identity['value']:
        raise ValueError('client archive identity differs')
    with tarfile.open(archive) as source:
        members = source.getmembers()
        (destination / 'archive-members.json').write_text(json.dumps([
            {'name': item.name, 'size': item.size, 'type': item.type.decode(), 'link': item.linkname}
            for item in members], indent=2) + '\n')
        selected = [item for item in members if item.name == entry['member']]
        if len(members) > 64 or len(selected) != 1:
            raise ValueError('client archive members differ')
        if sum(item.size for item in members) != entry['unpacked_bytes']:
            raise ValueError('client unpacked footprint differs from its pinned inventory')
        member, = selected
        if not member.isfile() or not 0 < member.size <= entry['unpacked_bytes']:
            raise ValueError(f'client executable differs: {member.name}, type={member.type!r}, size={member.size}')
        binary = destination / 'client'
        with source.extractfile(member) as data, binary.open('xb') as output:
            while block := data.read(1024**2):
                output.write(block)
    binary.chmod(0o755)
    archive.unlink()
    return binary


def environment(root):
    home = root / 'home'
    home.mkdir()
    env = {'PATH': os.environ['PATH'], 'HOME': str(home), 'LANG': 'C.UTF-8',
           'CODEX_HOME': str(home / '.codex'), 'CI': 'true',
           'DISABLE_TELEMETRY': '1', 'RAYON_NUM_THREADS': '1', 'UV_THREADPOOL_SIZE': '1'}
    for key, folder in [('XDG_CONFIG_HOME', 'config'), ('XDG_DATA_HOME', 'data'),
                        ('XDG_CACHE_HOME', 'cache'), ('XDG_STATE_HOME', 'state')]:
        path = home / folder
        path.mkdir()
        env[key] = str(path)
    for name in ('AUTOUPDATE', 'MODELS_FETCH', 'DEFAULT_PLUGINS', 'LSP_DOWNLOAD'):
        env['OPENCODE_DISABLE_' + name] = '1'
    return env


def inspect(name, entry, root):
    binary = install(entry, root)
    env = environment(root)
    arguments = [['--version'], ['--help']]
    arguments += ([['app-server', '--help'], ['exec', '--help']] if name == 'codex' else
                  [['serve', '--help'], ['api', '--help'], ['run', '--help'], ['models', '--help']])
    results = []
    for index, args in enumerate(arguments):
        result = subprocess.run([str(binary), *args], cwd=root, env=env, capture_output=True, timeout=10)
        (root / f'{index}.stdout').write_bytes(result.stdout)
        (root / f'{index}.stderr').write_bytes(result.stderr)
        if result.returncode != 0 or len(result.stdout) + len(result.stderr) > OUTPUT_BYTES:
            raise ValueError(f'{name} {args}: invalid metadata response; inspect retained output')
        if index == 0 and re.search(r'(?<![\d.])' + re.escape(entry['version']) + r'(?![\d.])', result.stdout.decode()) is None:
            raise ValueError(f'{name} reports a different release')
        results.append({'arguments': args, 'returncode': result.returncode,
                        'stdout': f'{index}.stdout', 'stderr': f'{index}.stderr'})
    with binary.open('rb') as source:
        executable_sha256 = hashlib.file_digest(source, 'sha256').hexdigest()
    return {'release': entry, 'binary_sha256': executable_sha256, 'commands': results,
            'credentials_supplied': False, 'inference_requests': 0}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    if os.environ.get('GITHUB_ACTIONS') != 'true':
        raise SystemExit('Run client probes on GitHub under the workstation rules.')
    args.output.mkdir(parents=True, exist_ok=False)
    clients = json.loads(CLIENTS.read_text())
    report = {}
    try:
        for name, entry in clients.items():
            report[name] = inspect(name, entry, args.output / name)
    finally:
        (args.output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps({'clients': list(report), 'live_model_calls': 0}))
