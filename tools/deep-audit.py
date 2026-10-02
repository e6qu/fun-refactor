"""Run bounded repository-audit shards and require complete test inventories."""

import argparse
import importlib.util
import json
import os
from pathlib import Path
import re
import subprocess

ROOT = Path(__file__).resolve().parents[1]
COUNTS = {"commands_agree": 6, "conformance": 1, "round_trip": 1,
          "self_translation": 1, "lean_kernels": 1}
# Seconds observed in the timed-out audit of 1e5a8a0. Unknown tests get a
# conservative weight; they remain in the discovered inventory and must run.
WEIGHTS = {
    "the_self_hosted_recipe_replays_without_changing_the_workspace": 630,
    "impact_covers_every_reference_it_could_rewrite": 470,
    "usages_reports_the_references_that_resolved_to_the_symbol": 250,
    "a_definition_group_holds_its_own_symbol": 145,
    "a_report_that_stops_early_says_how_many_it_left_out": 125,
    "every_span_duplicates_reports_is_a_region_of_its_file": 35,
    "usages_reports_the_name_where_it_appears_in_prose": 6,
    "a_comment_naming_a_symbol_is_reported_by_usages": 1,
}

spec = importlib.util.spec_from_file_location("ci_shards", ROOT / "tools/ci-shards.py")
assert spec and spec.loader
shards = importlib.util.module_from_spec(spec)
spec.loader.exec_module(shards)


def inventory(output):
    names = []
    for line in output.splitlines():
        if line.endswith(": test"):
            names.append(line[:-6])
        elif line.strip() and not re.fullmatch(r"\d+ tests?, 0 benchmarks?", line):
            raise ValueError("unexpected test inventory output: " + line)
    if not names or any(not name for name in names) or len(names) != len(set(names)):
        raise ValueError("empty or duplicate test inventory")
    return sorted(names)


def run(target, index, directory, revision):
    count = COUNTS[target]
    if not 0 <= index < count:
        raise ValueError("audit shard index out of range")
    command = ["cargo", "test", "--locked", "--features", "full-audit", "--test", target]
    discovered = inventory(subprocess.check_output(command + ["--", "--list"], text=True))
    weights = {name: WEIGHTS.get(name, 90) for name in discovered}
    selected = shards.partition(discovered, count, weights)[index]
    if not selected:
        raise ValueError("empty audit shard")
    filters = [arg for name in discovered if name not in selected for arg in ("--skip", name)]
    # libtest skip filters are substrings. Check their actual selection before
    # running, so a newly nested test cannot silently remove another test.
    actual = inventory(subprocess.check_output(command + ["--", "--list", *filters], text=True))
    if actual != selected:
        raise ValueError("test filters differ from the assigned inventory")
    print(f"{target}-{index}: {selected}", flush=True)
    subprocess.run(command + ["--", "--include-ignored", "--test-threads", "1", *filters], check=True)
    directory.mkdir(parents=True, exist_ok=True)
    (directory / f"{target}-{index}.json").write_text(json.dumps({
        "revision": revision, "target": target, "index": index, "count": count,
        "inventory": discovered, "selected": selected,
    }, indent=2) + "\n")


def verify(directory, revision):
    expected = {f"{target}-{index}.json" for target, count in COUNTS.items() for index in range(count)}
    if {path.name for path in directory.glob("*.json")} != expected:
        raise ValueError("missing or unexpected audit reports")
    for target, count in COUNTS.items():
        reports = [json.loads((directory / f"{target}-{index}.json").read_text()) for index in range(count)]
        discovered, selected = reports[0]["inventory"], []
        for index, report in enumerate(reports):
            if (report["revision"] != revision or report["target"] != target
                    or report["index"] != index or report["count"] != count
                    or report["inventory"] != discovered or not report["selected"]):
                raise ValueError("inconsistent audit report")
            selected.extend(report["selected"])
        if not discovered or sorted(set(discovered)) != discovered or sorted(selected) != discovered:
            raise ValueError("audit shards omit or duplicate tests")
        print(f"{target}: {len(discovered)} tests covered exactly once")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("target", choices=[*COUNTS, "verify"])
    parser.add_argument("index", type=int, nargs="?", default=0)
    parser.add_argument("--output", type=Path, default=Path("target/deep-audit"))
    args = parser.parse_args()
    os.chdir(ROOT)
    revision = subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip()
    if args.target == "verify":
        verify(args.output, revision)
    else:
        os.environ["LEAN_NUM_THREADS"] = "1"
        os.environ["FR_LEAN_JOBS"] = "1"
        os.environ["ZIG_GLOBAL_CACHE_DIR"] = str(ROOT / "target/zig-cache")
        os.environ["GOCACHE"] = str(ROOT / "target/go-cache")
        run(args.target, args.index, args.output, revision)


if __name__ == "__main__":
    main()
