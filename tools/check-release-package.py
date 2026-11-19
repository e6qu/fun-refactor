"""Verify the exact archive that will be uploaded, using an extracted consumer."""
import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import shutil
import subprocess
import sys
import tarfile
import tempfile
from product_bundle import provenance

ROOT = Path(__file__).resolve().parents[1]


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def unpack(archive, checksum, destination):
    expected = checksum.read_text().strip().split()
    if len(expected) != 2 or expected[1] != archive.name or expected[0] != digest(archive):
        raise ValueError('archive and published checksum differ')
    prefix = archive.name.removesuffix('.tar.gz')
    with tarfile.open(archive) as stream:
        members = stream.getmembers()
        if not members or len(members) > 4096 or sum(item.size for item in members) > 256 * 1024**2:
            raise ValueError('release archive exceeds its member or expanded-size limit')
        seen = set()
        for member in members:
            name = PurePosixPath(member.name)
            if (name.is_absolute() or '..' in name.parts or not name.parts or name.parts[0] != prefix or
                    name in seen or not (member.isfile() or member.isdir())):
                raise ValueError('release archive has an unexpected, duplicate or nonregular path')
            seen.add(name)
        stream.extractall(destination, filter='data')
    return destination / prefix


def check(archive, checksum, version, kind, web_root=None):
    with tempfile.TemporaryDirectory(prefix='fr-release-consumer-') as temporary:
        consumer = Path(temporary)
        package = unpack(archive, checksum, consumer)
        for name in ('LICENSE', 'README.md'):
            if not (package / name).is_file():
                raise ValueError('release package lacks ' + name)
        environment = {key: value for key, value in os.environ.items() if key not in {'PYTHONPATH', 'PYTHONHOME', 'VIRTUAL_ENV'}}
        environment['PYTHONDONTWRITEBYTECODE'] = '1'
        if kind == 'native':
            for name in ('fr', 'CLI.md', 'skills/fr/SKILL.md'):
                if not (package / name).is_file():
                    raise ValueError('native package lacks ' + name)
            completed = subprocess.run([sys.executable, '-I', str(ROOT / 'tools/package-consumer.py'),
                '--fr', str(package / 'fr'), '--version', version], cwd=consumer, env=environment,
                capture_output=True, text=True, timeout=180, check=True)
            checks = json.loads(completed.stdout)
        else:
            if not all((package / name).is_file() for name in ('fun_refactor.js', 'fun_refactor_bg.wasm')):
                raise ValueError('browser package lacks its JavaScript or WebAssembly module')
            web = consumer / 'web'
            shutil.copytree(web_root or ROOT / 'web', web, ignore=shutil.ignore_patterns('node_modules', 'dist', 'wasm'))
            shutil.copytree(package, web / 'src/wasm')
            program = """import {readFileSync} from 'node:fs';
import {pathToFileURL} from 'node:url';
const [modulePath, wasmPath, expected] = process.argv.slice(1);
const engine = await import(pathToFileURL(modulePath));
await engine.default({module_or_path: readFileSync(wasmPath)});
if (engine.version() !== expected) throw Error('packaged WASM version differs from release');
"""
            subprocess.run(['node', '--input-type=module', '-e', program,
                            str(web / 'src/wasm/fun_refactor.js'), str(web / 'src/wasm/fun_refactor_bg.wasm'), version],
                           cwd=consumer, env=environment, capture_output=True, text=True, timeout=30, check=True)
            suites = ['api', 'review', 'patch', 'session']
            for name in suites:
                subprocess.run(['node', str(web / 'test' / (name + '.mjs'))], cwd=consumer, env=environment,
                               capture_output=True, text=True, timeout=120, check=True)
            checks = {'passed': True, 'checks': suites}
        return {'schema': 'fr-release-package-check-1', 'kind': kind, 'version': version,
                'archive': archive.name, 'archive_sha256': digest(archive), 'consumer': checks,
                'scope': 'Executed extracted archive bytes in a fresh consumer; no architecture-only fallback.'}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--archive', required=True, type=Path)
    parser.add_argument('--checksum', required=True, type=Path)
    parser.add_argument('--version', required=True)
    parser.add_argument('--kind', choices=('native', 'wasm'), required=True)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--web-root', type=Path, help='Browser harness from the selected product source')
    parser.add_argument('--source-commit')
    parser.add_argument('--checker-commit')
    args = parser.parse_args()
    identity = provenance(args.source_commit, args.checker_commit)
    try:
        result = check(args.archive.resolve(), args.checksum.resolve(), args.version, args.kind,
                       args.web_root.resolve() if args.web_root else None)
    except subprocess.CalledProcessError as error:
        print(error.stdout or '', file=sys.stderr)
        print(error.stderr or '', file=sys.stderr)
        raise
    result['provenance'] = identity
    args.output.write_text(json.dumps(result, indent=2, sort_keys=True) + '\n')
    print(json.dumps(result, sort_keys=True))
