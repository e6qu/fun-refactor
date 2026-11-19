#!/usr/bin/env python3
"""Check the distributable wheel and rebuild its source archive in clean consumers."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import tempfile
from product_bundle import provenance

ROOT = Path(__file__).resolve().parents[1]


def run(arguments, cwd):
    environment = {key: value for key, value in os.environ.items()
                   if key not in {'PYTHONPATH', 'PYTHONHOME', 'VIRTUAL_ENV'}}
    return subprocess.run(list(map(str, arguments)), cwd=cwd, env=environment,
                          check=True, text=True, capture_output=True, timeout=180)


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def distributions(dist):
    wheels = sorted(dist.glob('*.whl'))
    sdists = sorted(dist.glob('*.tar.gz'))
    if len(wheels) != 1 or len(sdists) != 1:
        raise ValueError('SDK package requires exactly one wheel and one source archive')
    if not all(path.name.startswith('fun_refactor_ir-') for path in [*wheels, *sdists]):
        raise ValueError('unexpected SDK distribution name')
    return wheels[0], sdists[0]


def installed_consumer(wheel, binary, version, consumer):
    consumer.mkdir()
    environment = consumer / 'venv'
    run([sys.executable, '-I', '-m', 'venv', environment], consumer)
    python = environment / ('Scripts/python.exe' if sys.platform == 'win32' else 'bin/python')
    run([python, '-I', '-m', 'pip', 'install', '--no-index', '--no-deps', wheel], consumer)
    result = run([python, '-I', ROOT / 'tools/package-consumer.py',
                  '--fr', binary, '--version', version, '--sdk'], consumer)
    return json.loads(result.stdout)


def check(binary, dist, version):
    with tempfile.TemporaryDirectory(prefix='fr-sdk-consumer-') as temporary:
        consumer = Path(temporary)
        if dist is None:
            source = consumer / 'source'
            shutil.copytree(ROOT / 'sdk/python', source,
                           ignore=shutil.ignore_patterns('.venv', '.pytest_cache', '__pycache__', '*.egg-info', 'build', 'dist'))
            dist = consumer / 'dist'
            run([sys.executable, '-I', '-m', 'build', '--no-isolation', '--outdir', dist, source], consumer)
        wheel, sdist = distributions(dist)
        if version is None:
            version = run([binary, '--version'], consumer).stdout.strip().removeprefix('fr ')
        wheel_report = installed_consumer(wheel, binary, version, consumer / 'wheel-consumer')
        unpacked = consumer / 'unpacked'
        unpacked.mkdir()
        with tarfile.open(sdist) as archive:
            members = archive.getmembers()
            if (not members or len(members) > 4096 or sum(item.size for item in members) > 64 * 1024**2
                    or any(not (item.isfile() or item.isdir()) for item in members)):
                raise ValueError('unexpected SDK source archive members')
            archive.extractall(unpacked, filter='data')
        sources = list(unpacked.iterdir())
        if len(sources) != 1 or not (sources[0] / 'pyproject.toml').is_file():
            raise ValueError('SDK source archive has no unique build root')
        rebuilt = consumer / 'rebuilt'
        run([sys.executable, '-I', '-m', 'build', '--wheel', '--no-isolation', '--outdir', rebuilt, sources[0]], consumer)
        rebuilt_wheels = list(rebuilt.glob('*.whl'))
        if len(rebuilt_wheels) != 1:
            raise ValueError('SDK source archive did not rebuild one wheel')
        source_report = installed_consumer(rebuilt_wheels[0], binary, version, consumer / 'source-consumer')
        return {'schema': 'fr-sdk-package-check-2', 'passed': True, 'version': version,
                'wheel': {'name': wheel.name, 'sha256': digest(wheel), 'consumer': wheel_report},
                'sdist': {'name': sdist.name, 'sha256': digest(sdist), 'consumer': source_report},
                'consumer_root': 'isolated-temporary-directories'}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--fr', required=True, type=Path)
    parser.add_argument('--dist', type=Path, help='Check these already-built distributions without rebuilding them')
    parser.add_argument('--version', help='Require this release version in both installed consumers')
    parser.add_argument('--output', type=Path)
    parser.add_argument('--source-commit')
    parser.add_argument('--checker-commit')
    args = parser.parse_args()
    identity = provenance(args.source_commit, args.checker_commit)
    try:
        result = check(args.fr.resolve(), args.dist.resolve() if args.dist else None, args.version)
    except subprocess.CalledProcessError as error:
        print(error.stdout or '', file=sys.stderr)
        print(error.stderr or '', file=sys.stderr)
        raise
    result['provenance'] = identity
    payload = json.dumps(result, indent=2, sort_keys=True) + '\n'
    if args.output:
        args.output.write_text(payload)
    print(payload, end='')
