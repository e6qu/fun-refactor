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


def representative(directory, binary, client):
    """Require complete streamed-workload evidence before launching a local client."""
    manifest = load(directory / "provenance.json")
    expected_names = {f"streaming-{platform}-{case}" for platform in ("macos-15", "ubuntu-latest") for case in range(4)}
    require(manifest["conclusion"] == "success" and len(manifest["artifacts"]) == 8
            and {a["name"] for a in manifest["artifacts"]} == expected_names, "complete streaming matrix required")
    checker = runpy.run_path(str(ROOT / "tools/check-change-streaming.py"))
    identities = {"binary_sha256": source_reviews.identity(binary), "opencode_sha256": source_reviews.identity(client)}
    for artifact in manifest["artifacts"]:
        archive_path = directory / (artifact["name"] + ".zip")
        require(archive_path.stat().st_size <= 16 * 1024**2
                and source_reviews.identity(archive_path) == artifact["sha256"], "streaming archive differs")
        with tempfile.TemporaryDirectory() as temporary, zipfile.ZipFile(archive_path) as archive:
            root = Path(temporary).resolve()
            require(sum(i.file_size for i in archive.infolist()) <= 64 * 1024**2
                    and all((root / n).resolve().is_relative_to(root) for n in archive.namelist()), "invalid streaming archive")
            archive.extractall(root)
            saved = load(root / "result.json")
            require(saved["admitted"] and len(saved["attempts"]) == 4, "streaming admission failed; stop local client work")
            actual = checker["compare"]([checker["report"](root / str(i)) for i in range(4)])
            require(actual == saved and actual["admitted"], "streaming replay differs")
            case = int(artifact["name"].rsplit("-", 1)[1])
            for index, result in enumerate(actual["attempts"]):
                require(result["case"] == checker["control"].CASES[case], "streaming case differs")
                plan = load(root / str(index) / "plan.json")["plan"]
                require(result["runtime"] == changes.implementation()
                        and plan["client_environment"] == changes.CLIENT_ENVIRONMENTS[-1], "streaming runtime differs")
                if artifact["name"].startswith("streaming-macos-15-"):
                    require(all(result[key] == value for key, value in identities.items()), "streaming executable differs")
    return source_reviews.identity(directory / "provenance.json")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    parser.add_argument("--hosted", type=Path, required=True)
    parser.add_argument("--streaming", type=Path, required=True)
    parser.add_argument("--fr", type=Path, required=True)
    parser.add_argument("--opencode", type=Path, required=True)
    parser.add_argument("--confirm-local-control", action="store_true")
    args = parser.parse_args()
    require(args.confirm_local_control, "run one control through the workstation resource guard")
    require(not args.output.exists(), "control already exists; no retries")
    binary, client = args.fr.resolve(), args.opencode.resolve()
    streaming_sha = representative(args.streaming.resolve(), binary, client)
    checker = hosted(args.hosted.resolve(), binary, client)
    result = checker["single"](args.output.resolve(), 1, binary, client)
    (args.output / "result.json").write_bytes(encode(result))
    admitted = result["headroom_admitted"] and result["record"]["status"] == "completed"
    (args.output / "admission.json").write_bytes(encode({"admitted": admitted,
        "streaming_manifest_sha256": streaming_sha,
        "hosted_manifest_sha256": source_reviews.identity(args.hosted / "provenance.json"),
        "result_sha256": hashlib.sha256(encode(result)).hexdigest(),
        "scope": "One scripted fr preview/apply control under the local guard; live calls remain individually bounded."}))
    require(admitted, "workstation admission failed; stop local client work")
    print(encode({"admitted": admitted, "process": result["process"]}).decode())


if __name__ == "__main__":
    main()
