#!/usr/bin/env python3
"""Build the SDK and verify it from an isolated consumer environment."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile


def run(arguments: list[str], *, cwd: Path | None = None) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        arguments,
        cwd=cwd,
        check=True,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--fr", required=True, type=Path)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    binary = args.fr.resolve()
    if not binary.is_file():
        raise SystemExit(f"fr binary is absent: {binary}")

    with tempfile.TemporaryDirectory(prefix="fr-sdk-consumer-") as directory:
        consumer = Path(directory)
        source = consumer / "source"
        shutil.copytree(
            root / "sdk/python",
            source,
            ignore=shutil.ignore_patterns(
                ".venv", ".pytest_cache", "__pycache__", "*.egg-info"
            ),
        )
        dist = consumer / "dist"
        run([
            sys.executable, "-m", "build", "--no-isolation", "--outdir", str(dist),
            str(source),
        ], cwd=consumer)
        wheels = sorted(dist.glob("fun_refactor_ir-*.whl"))
        sdists = sorted(dist.glob("fun_refactor_ir-*.tar.gz"))
        if len(wheels) != 1 or len(sdists) != 1:
            raise SystemExit("SDK build did not produce exactly one wheel and one source archive")

        environment = consumer / "venv"
        run([sys.executable, "-m", "venv", str(environment)])
        python = environment / ("Scripts/python.exe" if sys.platform == "win32" else "bin/python")
        run([
            str(python), "-m", "pip", "install", "--no-index", "--no-deps",
            str(wheels[0]),
        ], cwd=consumer)
        program = (
            "import json,sys\n"
            "from importlib.metadata import version\n"
            "from fr_ir.runtime import FrClient\n"
            "report=FrClient(sys.argv[1], executable=sys.argv[2]).compatibility()\n"
            "print(json.dumps({'distribution_version':version('fun-refactor-ir'),"
            "'native_version':report.version,'schema':report.schema}))\n"
        )
        checked = run([str(python), "-c", program, str(consumer), str(binary)], cwd=consumer)
        report = json.loads(checked.stdout)
        if (report.get("schema") != "fr-sdk-compatibility-1"
                or report.get("distribution_version") != report.get("native_version")):
            raise SystemExit("installed SDK and native binary are incompatible")
        print(json.dumps({
            "schema": "fr-sdk-package-check-1",
            "passed": True,
            "version": report["native_version"],
            "wheel": wheels[0].name,
            "sdist": sdists[0].name,
            "consumer_root": "isolated-temporary-directory",
        }, sort_keys=True))


if __name__ == "__main__":
    main()
