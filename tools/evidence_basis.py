#!/usr/bin/env python3
"""Stable file identities for retained evaluation evidence."""

import hashlib
import json
from pathlib import Path
import tomllib


def digest(data):
    return hashlib.sha256(data).hexdigest()


def cargo_lock_basis(data):
    lock = tomllib.loads(data.decode())
    local = {
        (package["name"], str(package["version"]))
        for package in lock.get("package", [])
        if "source" not in package
    }
    packages = []
    for package in lock.get("package", []):
        normalized = dict(package)
        if "source" not in normalized:
            normalized["version"] = "<workspace>"
        if "dependencies" in normalized:
            dependencies = []
            for dependency in normalized["dependencies"]:
                replacement = dependency
                for name, version in local:
                    if dependency == f"{name} {version}":
                        replacement = f"{name} <workspace>"
                        break
                dependencies.append(replacement)
            normalized["dependencies"] = dependencies
        packages.append(normalized)
    lock["package"] = packages
    return json.dumps(lock, sort_keys=True, separators=(",", ":")).encode()


def file_digest(path):
    path = Path(path)
    data = path.read_bytes()
    return digest(cargo_lock_basis(data) if path.name == "Cargo.lock" else data)


def self_test():
    def lock(local_version="0.1.0", remote_version="1.0.0", checksum="first"):
        return f'''version = 4

[[package]]
name = "workspace-root"
version = "{local_version}"
dependencies = ["workspace-child {local_version}", "remote"]

[[package]]
name = "workspace-child"
version = "{local_version}"

[[package]]
name = "remote"
version = "{remote_version}"
source = "registry+https://example.invalid/index"
checksum = "{checksum}"
'''.encode()

    baseline = digest(cargo_lock_basis(lock()))
    assert baseline == digest(cargo_lock_basis(lock(local_version="0.2.0")))
    assert baseline != digest(cargo_lock_basis(lock(remote_version="1.0.1")))
    assert baseline != digest(cargo_lock_basis(lock(checksum="second")))



def _native_policy(root):
    roots, ignored = {}, set()
    for line in (root / 'native-source-inputs.txt').read_text().splitlines():
        if not line or line.strip() != line:
            raise ValueError('Native source policy needs one literal path per line.')
        name = line[1:] if line.startswith('!') else line
        directory = name.endswith('/')
        name = name[:-1] if directory else name
        if (not name or '\\' in name or Path(name).is_absolute()
                or any(part in {'', '.', '..'} for part in name.split('/'))):
            raise ValueError('Native source roots must be normalized relative paths.')
        if line.startswith('!'):
            if not directory or '/' in name or name in ignored:
                raise ValueError('Ignored directories need unique single names.')
            ignored.add(name)
        else:
            if name in roots:
                raise ValueError('Native source roots must be unique.')
            roots[name] = directory
    if roots.get('native-source-inputs.txt') is not False:
        raise ValueError('Native source policy must include itself.')
    return roots, ignored


def native_source_roots(root):
    return tuple(_native_policy(Path(root))[0])


def native_source_paths(root):
    import stat
    root = Path(root)
    roots, ignored = _native_policy(root)
    files = set()
    def collect(relative):
        path = root / relative
        mode = path.lstat().st_mode
        if stat.S_ISLNK(mode):
            raise ValueError(f'Native source input is a symlink: {relative}')
        if stat.S_ISREG(mode):
            files.add(relative.as_posix())
        elif stat.S_ISDIR(mode):
            for child in path.iterdir():
                if stat.S_ISDIR(child.lstat().st_mode) and child.name in ignored:
                    continue
                collect(relative / child.name)
        else:
            raise ValueError(f'Native source input is not a regular file: {relative}')
    for name, directory in roots.items():
        path = root / name
        mode = path.lstat().st_mode
        if stat.S_ISLNK(mode) or stat.S_ISDIR(mode) != directory:
            raise ValueError(f'Native source root has the wrong kind: {name}')
        collect(Path(name))
    return tuple(sorted(files))


def cargo_manifest_basis(data):
    manifest = tomllib.loads(data.decode())
    version = manifest['package']['version']
    manifest['package']['version'] = '<workspace>'
    for container in [manifest, manifest.get('workspace', {}), *manifest.get('target', {}).values()]:
        for kind in ('dependencies', 'dev-dependencies', 'build-dependencies'):
            for dependency in container.get(kind, {}).values():
                if isinstance(dependency, dict) and 'path' in dependency and dependency.get('version') == version:
                    dependency['version'] = '<workspace>'
    return json.dumps(manifest, sort_keys=True, ensure_ascii=False, separators=(',', ':'), allow_nan=False).encode()


def native_source_bindings(root):
    root = Path(root)
    manifest = tomllib.loads((root / 'Cargo.toml').read_text())
    members = {'Cargo.toml' if member == '.' else member + '/Cargo.toml'
               for member in manifest['workspace']['members']}
    return {path: digest(cargo_manifest_basis((root / path).read_bytes()))
            if path in members else file_digest(root / path) for path in native_source_paths(root)}


def native_bindings(root, declared):
    return {**{path: file_digest(Path(root) / path) for path in declared}, **native_source_bindings(root)}


def checker_bindings(root, declared):
    root = Path(root)
    client = root / 'sdk/python/src'
    metadata = root / 'sdk/python/pyproject.toml'
    if (client.is_symlink() or not client.is_dir()
            or metadata.is_symlink() or not metadata.is_file()):
        raise ValueError('Checker evidence needs a complete regular Python client source tree.')
    entries = tuple(client.rglob('*'))
    paths = sorted(path for path in entries if path.suffix == '.py' and path.is_file())
    if not paths or any(path.is_symlink() for path in entries):
        raise ValueError('Checker evidence needs a complete regular Python client source tree.')
    names = sorted({*declared, 'sdk/python/pyproject.toml', *(path.relative_to(root).as_posix() for path in paths)})
    return {**{path: file_digest(root / path) for path in names}, **native_source_bindings(root)}


if __name__ == "__main__":
    self_test()
