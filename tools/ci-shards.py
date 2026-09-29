"""Partition the complete Cargo/pytest inventories and verify their collected evidence."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
COUNTS = {"native": 6, "sdk": 4}
SDK_WRAPPER = "python_sdk_tests_pass_with_the_declared_test_extra"


def partition(items, count, weights):
    if count < 1 or len(items) != len(set(items)):
        raise ValueError("invalid shard inventory")
    shards = [[] for _ in range(count)]
    loads = [0.0] * count
    for item in sorted(items, key=lambda item: (-weights.get(item, 10), item)):
        index = min(range(count), key=lambda index: (loads[index], index))
        shards[index].append(item)
        loads[index] += weights.get(item, 10)
    return [sorted(shard) for shard in shards]


def record(directory, kind, index, inventory, selected):
    directory.mkdir(parents=True, exist_ok=True)
    (directory / f"{kind}-{index}.json").write_text(json.dumps({
        "kind": kind, "index": index, "count": COUNTS[kind],
        "inventory": sorted(inventory), "selected": sorted(selected),
    }, indent=2) + "\n")


def native_inventory():
    metadata = json.loads(subprocess.check_output([
        "cargo", "metadata", "--no-deps", "--format-version", "1", "--locked",
    ]))
    package = next(p for p in metadata["packages"]
                   if Path(p["manifest_path"]) == ROOT / "Cargo.toml")
    result = {}
    for target in package["targets"]:
        kinds = target["kind"]
        kind = "lib" if "lib" in kinds or "rlib" in kinds else kinds[0]
        if kind == "custom-build":
            continue
        if kind not in {"lib", "bin", "test", "example", "bench"}:
            raise ValueError(f"unsupported test target: {target}")
        result[f"{kind}:{target['name']}"] = (
            ["--lib"] if kind == "lib" else [f"--{kind}", target["name"]]
        )
    if not result:
        raise ValueError("Cargo discovered no targets")
    return result


def run_native(index, directory):
    inventory = native_inventory()
    weights = json.loads((ROOT / "tools/ci-test-durations.json").read_text())["seconds"]
    selected = partition(list(inventory), COUNTS["native"], weights)[index]
    if not selected:
        raise ValueError("empty native shard")
    command = ["cargo", "test", "--locked"]
    for target in selected:
        command.extend(inventory[target])
    command.extend(["--", "--test-threads", "1", "--skip", SDK_WRAPPER])
    print("Native targets:", selected, flush=True)
    subprocess.run(command, check=True)
    record(directory, "native", index, list(inventory), selected)


class SdkShard:
    def __init__(self, index, directory):
        self.index = index
        self.directory = directory
        self.inventory = []
        self.selected = []

    def pytest_collection_modifyitems(self, session, config, items):
        self.inventory = [item.nodeid for item in items]
        if len(self.inventory) != len(set(self.inventory)):
            raise ValueError("duplicate pytest node IDs")
        selected, deselected = [], []
        for item in items:
            bucket = int.from_bytes(hashlib.sha256(item.nodeid.encode()).digest(), "big")
            (selected if bucket % COUNTS["sdk"] == self.index else deselected).append(item)
        config.hook.pytest_deselected(items=deselected)
        items[:] = selected
        self.selected = [item.nodeid for item in selected]

    def pytest_sessionfinish(self, session, exitstatus):
        if exitstatus == 0 and self.selected:
            record(self.directory, "sdk", self.index, self.inventory, self.selected)


def verify(directory):
    for kind, count in COUNTS.items():
        reports = [json.loads((directory / f"{kind}-{index}.json").read_text())
                   for index in range(count)]
        inventory = reports[0]["inventory"]
        selected = []
        for index, report in enumerate(reports):
            if (report["kind"] != kind or report["index"] != index
                    or report["count"] != count or report["inventory"] != inventory
                    or not report["selected"]):
                raise ValueError(f"inconsistent {kind} shard {index}")
            selected.extend(report["selected"])
        if len(inventory) != len(set(inventory)) or sorted(selected) != inventory:
            raise ValueError(f"{kind} shards omit or duplicate tests")
        print(f"{kind}: {len(inventory)} inventory entries covered exactly once")
    logs = [directory / f"{kind}-{index}.log"
            for kind, count in COUNTS.items() for index in range(count)]
    combined = directory / "capability.log"
    combined.write_bytes(b"".join(path.read_bytes() for path in logs))
    matrices = [json.loads((directory / f"{kind}-{index}-matrix.json").read_text())
                for kind, count in COUNTS.items() for index in range(count)]
    if any(matrix != matrices[0] for matrix in matrices):
        raise ValueError("shards disagree on advertised capabilities")
    subprocess.run([sys.executable, "tools/capability-report.py",
                    str(directory / "native-0-matrix.json"), str(combined)], check=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("kind", choices=[*COUNTS, "verify"])
    parser.add_argument("index", type=int, nargs="?", default=0)
    parser.add_argument("--output", type=Path, default=Path("target/ci-results"))
    args = parser.parse_args()
    os.chdir(ROOT)
    directory = args.output.resolve()
    if args.kind == "verify":
        verify(directory)
        return
    if not 0 <= args.index < COUNTS[args.kind]:
        parser.error("shard index out of range")
    directory.mkdir(parents=True, exist_ok=True)
    log = directory / f"{args.kind}-{args.index}.log"
    log.write_text("")
    os.environ["FR_CAPABILITY_LOG"] = str(log)
    os.environ["PYTHONPATH"] = os.pathsep.join(filter(None, [
        str(ROOT / "sdk/python/src"), os.environ.get("PYTHONPATH", ""),
    ]))
    os.environ["LEAN_NUM_THREADS"] = "1"
    os.environ["FR_LEAN_JOBS"] = "1"
    os.environ["ZIG_GLOBAL_CACHE_DIR"] = str(ROOT / "target/zig-cache")
    os.environ["GOCACHE"] = str(ROOT / "target/go-cache")
    if args.kind == "native":
        run_native(args.index, directory)
    else:
        subprocess.run(["cargo", "build", "--locked", "--bin", "fr"], check=True)
        sys.path.insert(0, str(ROOT / "sdk/python/src"))
        import pytest
        status = pytest.main(["sdk/python/tests", "-v", "--durations=20"],
                             plugins=[SdkShard(args.index, directory)])
        if status:
            raise SystemExit(status)
    matrix = subprocess.check_output(["target/debug/fr", "capabilities", "--json"])
    (directory / f"{args.kind}-{args.index}-matrix.json").write_bytes(matrix)


if __name__ == "__main__":
    main()
