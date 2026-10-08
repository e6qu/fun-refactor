#!/usr/bin/env python3
"""Admit one guarded workstation control only after matching hosted controls pass."""
import argparse
import hashlib
from pathlib import Path
import runpy
import tempfile
import zipfile

from agent_eval import source_reviews, terminal_changes as changes
from agent_eval.study import encode, load, require

ROOT = Path(__file__).resolve().parents[1]


def hosted(directory, binary, client):
    manifest = load(directory / "provenance.json")
    artifacts = manifest["artifacts"]
    require({r["platform"] for r in artifacts} == {"macos-15", "ubuntu-latest"} and len(artifacts) == 2,
            "both hosted platforms required")
    checker = runpy.run_path(str(ROOT / "tools/check-terminal-changes.py"))
    for artifact in artifacts:
        name = artifact["archive"]
        require(Path(name).name == name, "invalid archive name")
        archive_path = directory / name
        require(archive_path.stat().st_size <= 1024**2
                and source_reviews.identity(archive_path) == artifact["sha256"], "hosted archive differs")
        with tempfile.TemporaryDirectory() as temporary, zipfile.ZipFile(archive_path) as archive:
            root = Path(temporary).resolve()
            require(sum(i.file_size for i in archive.infolist()) <= 8 * 1024**2
                    and all((root / n).resolve().is_relative_to(root) for n in archive.namelist()), "invalid control archive")
            archive.extractall(root)
            saved = load(root / "result.json")
            require(len(saved) == len(checker["CASES"]), "incomplete hosted controls")
            for index, expected in enumerate(saved):
                folder = root / str(index)
                actual = checker["report"](folder)
                require(actual == expected and actual["headroom_admitted"], "hosted admission failed")
                plan = load(folder / "plan.json")["plan"]
                require(plan["runtime"] == changes.implementation()
                        and plan["client_environment"] == changes.CLIENT_ENVIRONMENTS[-1], "hosted runtime differs")
                require(plan["provenance"]["script_sha256"] == source_reviews.identity(ROOT / "tools/check-terminal-changes.py"),
                        "hosted control implementation differs")
                if artifact["platform"] == "macos-15":
                    require(plan["binary_sha256"] == source_reviews.identity(binary)
                            and plan["opencode_sha256"] == source_reviews.identity(client), "hosted executable differs")
    return checker


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    parser.add_argument("--hosted", type=Path, required=True)
    parser.add_argument("--fr", type=Path, required=True)
    parser.add_argument("--opencode", type=Path, required=True)
    parser.add_argument("--confirm-local-control", action="store_true")
    args = parser.parse_args()
    require(args.confirm_local_control, "run one control through the workstation resource guard")
    require(not args.output.exists(), "control already exists; no retries")
    binary, client = args.fr.resolve(), args.opencode.resolve()
    checker = hosted(args.hosted.resolve(), binary, client)
    result = checker["single"](args.output.resolve(), 1, binary, client)
    (args.output / "result.json").write_bytes(encode(result))
    admitted = result["headroom_admitted"] and result["record"]["status"] == "completed"
    (args.output / "admission.json").write_bytes(encode({"admitted": admitted,
        "hosted_manifest_sha256": source_reviews.identity(args.hosted / "provenance.json"),
        "result_sha256": hashlib.sha256(encode(result)).hexdigest(),
        "scope": "One scripted fr preview/apply control under the local guard; live calls remain individually bounded."}))
    require(admitted, "workstation admission failed; stop local client work")
    print(encode({"admitted": admitted, "process": result["process"]}).decode())


if __name__ == "__main__":
    main()
