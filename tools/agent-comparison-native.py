#!/usr/bin/env python3
"""Run three separate native-client/tool setups on GitHub without live models."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import urllib.request

from agent_eval.comparison_clients import HOSTS, SETUPS, require

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("comparison_boundary", ROOT / "tools/agent-comparison-rehearsal.py")
boundary = importlib.util.module_from_spec(spec)
spec.loader.exec_module(boundary)


def download(entry, destination):
    destination.mkdir()
    archive = destination / "archive.tar.gz"
    digest, size = hashlib.sha256(), 0
    with urllib.request.urlopen(entry["url"], timeout=30) as response, archive.open("wb") as output:
        while chunk := response.read(1024**2):
            size += len(chunk)
            require(size <= 512 * 1024**2, "pinned package exceeds 512 MiB")
            digest.update(chunk)
            output.write(chunk)
    require(digest.hexdigest() == entry["sha256"], "pinned package checksum mismatch")
    with tarfile.open(archive) as bundle:
        bundle.extractall(destination / "unpacked", filter="data")
    archive.unlink()
    return destination / "unpacked"


def run(host, output):
    require(os.environ.get("GITHUB_ACTIONS") == "true", "run native rehearsals on GitHub, not a workstation")
    output.mkdir(parents=True, exist_ok=False)
    summary = {"host": host, "live_model_calls": 0, "passed": False,
               "setups": {s: {"status": "unstarted"} for s in SETUPS},
               "scope": "Scripted configuration and real tool execution; not agent adoption, costs or completed study attempts."}
    try:
        client = json.loads((ROOT / "tests/agent-eval/agent-first-use/hosts.json").read_text())[host]
        native = download(client, output / "client-package") / client["executable"]
        require(native.is_file(), "pinned native executable is missing")
        native.chmod(0o755)
        for setup in SETUPS:
            summary["setups"][setup] = {"status": "running"}
            context = output / (setup + "-image")
            context.mkdir()
            (context / "client/bin").mkdir(parents=True)
            shutil.copyfile(native, context / "client/bin" / host)
            (context / "client/bin" / host).chmod(0o755)
            (context / "client/identity.json").write_text(json.dumps(client) + "\n")
            (context / "tool/bin").mkdir(parents=True)
            if setup != "files":
                relative = "tests/agent-eval/agent-first-use/codegraph.json" if setup == "codegraph" else "tests/agent-eval/agent-comparison/tools.json"
                entry = json.loads((ROOT / relative).read_text())
                if setup == "fr":
                    entry = entry["fr"]
                package = download(entry, output / (setup + "-package"))
                if setup == "codegraph":
                    shutil.copytree(package / "codegraph-linux-x64", context / "tool", dirs_exist_ok=True)
                else:
                    binary, = (p for p in package.rglob("fr") if p.is_file())
                    shutil.copyfile(binary, context / "tool/bin/fr")
                    (context / "tool/bin/fr").chmod(0o755)
            if setup == "fr":
                shutil.copytree(ROOT / "skills/fr", context / "skill")
            else:
                (context / "skill").mkdir()
            for module in ("comparison_clients.py", "comparison_native.py"):
                shutil.copyfile(ROOT / "tools/agent_eval" / module, context / module)
            shutil.copyfile(ROOT / "tests/agent-eval/agent-comparison/Native.Dockerfile", context / "Dockerfile")
            subprocess.run(["docker", "build", "--tag", "fr-native-" + host + "-" + setup, str(context)], check=True, timeout=120)
            image = subprocess.check_output(["docker", "image", "inspect", "--format", "{{.Id}}",
                                             "fr-native-" + host + "-" + setup], timeout=10).decode().strip()
            shutil.rmtree(context)
            report = boundary.rehearsal(image, setup, output / setup, False, host)
            require(report["passed"], "native rehearsal did not pass")
            summary["setups"][setup] = {"status": "passed", "image": image, "result": setup + "/result.json"}
        summary["passed"] = True
    except Exception as error:
        for row in summary["setups"].values():
            if row["status"] == "running":
                row.update(status="failed", error=type(error).__name__ + ": " + str(error)[:4096])
        summary["error"] = type(error).__name__ + ": " + str(error)[:4096]
        raise
    finally:
        (output / "summary.json").write_text(json.dumps(summary, indent=2, allow_nan=False) + "\n")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--host", choices=HOSTS, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    run(args.host, args.output.resolve())
