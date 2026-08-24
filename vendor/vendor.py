#!/usr/bin/env python3
"""Vendor upstream tree-sitter query files with provenance.

Run from the repository root:  python3 vendor/vendor.py

Re-running is idempotent: it rewrites every vendored file and regenerates
MANIFEST.toml, so a diff shows exactly what changed upstream.
"""
import hashlib
import glob
import os
import shutil
import subprocess
import sys
from datetime import date

# Grammar crate -> (our language name, upstream repository).
# The repository is recorded because a crate is a mirror of one. The version is the pin:
# the crate version Cargo.lock resolves, or the upstream tag `grammars/<lang>` names for
# a grammar this repository compiles itself.
GRAMMARS = {
    "tree-sitter-rust": ("rust", "https://github.com/tree-sitter/tree-sitter-rust"),
    "tree-sitter-go": ("go", "https://github.com/tree-sitter/tree-sitter-go"),
    "tree-sitter-zig": ("zig", "https://github.com/tree-sitter-grammars/tree-sitter-zig"),
    "tree-sitter-typescript": ("typescript", "https://github.com/tree-sitter/tree-sitter-typescript"),
    "tree-sitter-python": ("python", "https://github.com/tree-sitter/tree-sitter-python"),
    "tree-sitter-bash": ("bash", "https://github.com/tree-sitter/tree-sitter-bash"),
    "tree-sitter-hcl": ("hcl", "https://github.com/tree-sitter-grammars/tree-sitter-hcl"),
    "tree-sitter-yaml": ("yaml", "https://github.com/tree-sitter-grammars/tree-sitter-yaml"),
    "tree-sitter-html": ("html", "https://github.com/tree-sitter/tree-sitter-html"),
    "tree-sitter-css": ("css", "https://github.com/tree-sitter/tree-sitter-css"),
    "tree-sitter-scss": ("scss", "https://github.com/tree-sitter-grammars/tree-sitter-scss"),
    "tree-sitter-sass": ("sass", "https://github.com/bajrangCoder/tree-sitter-sass"),
    "tree-sitter-xml": ("xml", "https://github.com/tree-sitter-grammars/tree-sitter-xml"),
    "tree-sitter-java": ("java", "https://github.com/tree-sitter/tree-sitter-java"),
    "tree-sitter-md-025": ("markdown", "https://github.com/tree-sitter-grammars/tree-sitter-markdown"),
}

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
VENDOR = os.path.join(ROOT, "vendor")
QUERIES = os.path.join(VENDOR, "tree-sitter-queries")


def crate_dir(name):
    """The extracted crate source Cargo compiled, so the pin is what we build."""
    matches = sorted(glob.glob(os.path.expanduser(
        f"~/.cargo/registry/src/*/{name}-*/")))
    return matches[-1] if matches else None


def vendored_dir(language):
    """The grammar this repository compiles itself, when there is one.

    Five languages are built from `grammars/`, so their upstream release is here and
    not in the registry. The directory carries the same `queries/` the release does.
    """
    directory = os.path.join(ROOT, "grammars", language)
    if os.path.isdir(os.path.join(directory, "queries")):
        return directory + os.sep
    return None


def vendored_tag(directory):
    """What `PROVENANCE.toml` pins: an upstream tag, or a commit where there is no tag."""
    provenance = os.path.join(directory, "PROVENANCE.toml")
    for key in ("tag =", "commit ="):
        for line in open(provenance, encoding="utf-8"):
            if line.strip().startswith(key):
                return line.split("=", 1)[1].strip().strip('"')
    raise SystemExit(f"{provenance} names neither a tag nor a commit")


def sha256(path):
    with open(path, "rb") as handle:
        return hashlib.sha256(handle.read()).hexdigest()


def license_of(directory):
    """The SPDX id the crate declares, which is what a licence audit must use."""
    manifest = os.path.join(directory, "Cargo.toml")
    if os.path.exists(manifest):
        for line in open(manifest, encoding="utf-8", errors="replace"):
            if line.strip().startswith("license"):
                return line.split("=", 1)[1].strip().strip('"')
    return None


def license_file(directory):
    for entry in sorted(os.listdir(directory)):
        if entry.lower().startswith(("license", "licence", "copying")):
            full = os.path.join(directory, entry)
            if os.path.isfile(full):
                return full
    return None


def main():
    if os.path.isdir(QUERIES):
        shutil.rmtree(QUERIES)
    os.makedirs(QUERIES)

    entries, missing = [], []
    for crate, (language, repository) in sorted(GRAMMARS.items()):
        directory = vendored_dir(language) or crate_dir(crate)
        if not directory:
            missing.append(crate)
            continue
        local = vendored_dir(language)
        version = (
            vendored_tag(local)
            if local
            else os.path.basename(directory.rstrip("/")).rsplit("-", 1)[-1]
        )

        sources = sorted(glob.glob(os.path.join(directory, "queries", "**", "*.scm"),
                                   recursive=True))
        if not sources:
            # Recorded and not skipped: "this grammar ships no queries" is a fact
            # a reader needs, and its absence would otherwise look like an oversight.
            entries.append({
                "language": language,
                "crate": crate,
                "version": version,
                "repository": repository,
                "license": license_of(directory) or "unknown",
                "license_file": None,
                "files": [],
            })
            continue

        target = os.path.join(QUERIES, language)
        os.makedirs(target, exist_ok=True)
        files = []
        for source in sources:
            relative = os.path.relpath(source, os.path.join(directory, "queries"))
            destination = os.path.join(target, relative)
            os.makedirs(os.path.dirname(destination), exist_ok=True)
            shutil.copyfile(source, destination)
            files.append((relative, sha256(destination)))

        found = license_file(directory)
        if found:
            shutil.copyfile(found, os.path.join(target, "LICENSE"))

        entries.append({
            "language": language,
            "crate": crate,
            "version": version,
            "repository": repository,
            "license": license_of(directory) or "unknown",
            "license_file": "LICENSE" if found else None,
            "files": files,
        })

    lines = [
        "# Provenance for every vendored artifact.",
        "#",
        "# Generated by vendor/vendor.py — do not edit by hand. Re-run it to refresh,",
        "# and the diff shows exactly what changed upstream.",
        "#",
        "# `version` is the crate version Cargo resolved, or the upstream tag that",
        "# `grammars/<lang>/PROVENANCE.toml` pins. Either way it is what the build",
        "# compiles. The repository is recorded because a grammar crate is a mirror of",
        "# one, and that is where the upstream history lives.",
        "",
        f'retrieved = "{date.today().isoformat()}"',
        f'tree_sitter = "{TREE_SITTER_VERSION}"',
        "",
    ]
    for entry in entries:
        lines += [
            "[[artifact]]",
            f'language = "{entry["language"]}"',
            f'crate = "{entry["crate"]}"',
            f'version = "{entry["version"]}"',
            f'repository = "{entry["repository"]}"',
            f'license = "{entry["license"]}"',
        ]
        if entry["license_file"]:
            lines.append(
                f'license_file = "tree-sitter-queries/{entry["language"]}/LICENSE"')
        if entry["files"]:
            lines.append("files = [")
            for relative, digest in entry["files"]:
                lines.append(f'  {{ path = "{relative}", sha256 = "{digest}" }},')
            lines.append("]")
        else:
            lines.append("files = []")
            lines.append('note = "this grammar ships no query files upstream"')
        lines.append("")

    with open(os.path.join(VENDOR, "MANIFEST.toml"), "w", encoding="utf-8") as handle:
        handle.write("\n".join(lines))

    print(f"vendored {len(entries)} grammar query sets")
    for entry in entries:
        print(f"  {entry['language']:<11} {entry['crate']}-{entry['version']:<8} "
              f"{entry['license']:<12} {len(entry['files'])} file(s)")
    if missing:
        print(f"\nnot found in the cargo registry (build the project first): "
              f"{', '.join(missing)}", file=sys.stderr)
        return 1
    return 0


TREE_SITTER_VERSION = "0.26"

if __name__ == "__main__":
    sys.exit(main())
