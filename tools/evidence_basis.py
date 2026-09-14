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


if __name__ == "__main__":
    self_test()
