"""Partition the complete Cargo/pytest inventories and verify their collected evidence."""

import argparse
import ast
import hashlib
import json
import os
from pathlib import Path
import re
import runpy
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
COUNTS = {"native": 8, "sdk": 4}
SDK_WRAPPER = "python_sdk_tests_pass_with_the_declared_test_extra"


def result_inputs(names, sources):
    tokens = set(re.findall(r'[A-Za-z0-9][A-Za-z0-9_.-]*', '\n'.join(sources)))
    selected = []
    for name in names:
        if not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9_.-]*', name):
            raise ValueError('invalid retained result directory')
        if name in tokens:
            selected.append(name)
    return sorted(selected)


def checkout_plan(kind, index):
    selected = set()
    if kind == 'sdk':
        paths = set((ROOT / 'sdk/python/tests').rglob('*.py'))
        catalog = json.loads((ROOT / 'tests/agent-eval/roadmap.json').read_text())
        contract, = [row for row in catalog['evidence'] if row['id'] == 'python-contract']
        sources = [path.read_text() for path in sorted(paths)]
        sources.append(json.dumps(contract))
        return {'sources': sources, 'guides': False, 'catalog': False}
    else:
        selected = set(native_selection(native_inventory(), index))
        metadata = json.loads(subprocess.check_output([
            'cargo', 'metadata', '--no-deps', '--format-version', '1', '--locked',
        ], cwd=ROOT))
        package = next(row for row in metadata['packages']
                       if Path(row['manifest_path']) == ROOT / 'Cargo.toml')
        paths = set()
        for target in package['targets']:
            kinds = target['kind']
            target_kind = 'lib' if 'lib' in kinds or 'rlib' in kinds else kinds[0]
            if target_kind + ':' + target['name'] in selected:
                source = Path(target['src_path']).resolve()
                if not source.is_relative_to(ROOT.resolve()):
                    raise ValueError('test source is outside the checkout')
                paths.add(source)
                if target_kind == 'lib':
                    paths.update((ROOT / 'src').rglob('*.rs'))
    if not paths:
        raise ValueError('assigned tests have no source files')
    sources = [path.read_text() for path in sorted(paths)]
    guides = 'test:docs_links' in selected
    catalog_audit = 'test:roadmap_outcomes' in selected
    candidates = set((ROOT / 'tools').glob('*.py')) | set((ROOT / 'tools/agent_eval').glob('*.py'))
    tokens = set(re.findall(r'[A-Za-z0-9][A-Za-z0-9_.-]*', '\n'.join(sources)))
    pending = {path for path in candidates if path.name in tokens or path.stem in tokens}
    visited = set()
    while pending:
        path = pending.pop()
        if path in visited:
            continue
        visited.add(path)
        text = path.read_text()
        if path.name == 'roadmap-status.py':
            catalog_audit = True
        else:
            sources.append(text)
        if path.name == 'review-docs.py':
            guides = True
        dependencies = set()
        for node in ast.walk(ast.parse(text)):
            if isinstance(node, ast.Import):
                dependencies.update(alias.name.rsplit('.', 1)[-1] for alias in node.names)
            elif isinstance(node, ast.ImportFrom):
                dependencies.add((node.module or '').rsplit('.', 1)[-1])
                if node.module in {'agent_eval', None}:
                    dependencies.update(alias.name for alias in node.names)
            elif isinstance(node, ast.Call):
                dependencies.update(Path(child.value).name for child in ast.walk(node)
                                    if isinstance(child, ast.Constant) and isinstance(child.value, str)
                                    and (child.value.endswith('.py')
                                         or any(candidate.stem == child.value for candidate in candidates)))
        pending.update(candidate for candidate in candidates - visited
                       if candidate.name in dependencies or candidate.stem in dependencies)
    data = set((ROOT / 'tests/agent-eval').glob('*.json'))
    data.update((ROOT / 'tests/agent-eval').glob('*/*.json'))
    data -= {ROOT / 'tests/agent-eval/roadmap.json', ROOT / 'tests/agent-eval/current-scalar-reports.json'}
    while True:
        text = '\n'.join(sources)
        tokens = set(re.findall(r'[A-Za-z0-9][A-Za-z0-9_.-]*', text))
        required = {path for path in data
                    if path.relative_to(ROOT).as_posix() in text
                    or (path.parent != ROOT / 'tests/agent-eval' and path.parent.name in tokens)
                    or (path.parent == ROOT / 'tests/agent-eval' and path.name in tokens)}
        if not required:
            break
        sources.extend(path.read_text() for path in sorted(required))
        data -= required
    tokens = set(re.findall(r'[A-Za-z0-9][A-Za-z0-9_.-]*', '\n'.join(sources)))
    catalog = json.loads((ROOT / 'tests/agent-eval/roadmap.json').read_text())
    scalar = json.loads((ROOT / 'tests/agent-eval/current-scalar-reports.json').read_text())
    sources.extend(json.dumps(row) for row in catalog['evidence'] if row['id'] in tokens)
    sources.extend(json.dumps(row) for group, row in scalar['reports'].items() if group in tokens)
    if 'compiler-profile' in tokens and 'compiler-profile' in scalar['reports']:
        directory = Path(scalar['reports']['compiler-profile']['path']).parent
        family = directory.with_name(directory.name.removesuffix('-compiler-profile') + '-scalar-collection')
        sources.append(family.as_posix())
    print(f'Checkout inputs for {kind}-{index}: {len(paths)} test source files.', flush=True)
    if selected:
        print('Assigned native targets:', sorted(selected), flush=True)
    return {'sources': sources, 'guides': guides, 'catalog': catalog_audit}


def result_pattern(name):
    path = Path(name)
    if (path.as_posix() != name or path.parts[:3] != ('tests', 'agent-eval', 'results')
            or len(path.parts) < 4 or '..' in path.parts
            or any(char in name for char in '\\!*?[]\n\r')):
        raise ValueError('invalid retained input path: ' + name)
    return '/' + name


def checkout_inputs(plan):
    entries = subprocess.check_output([
        'git', 'ls-tree', '--format=%(objecttype) %(path)',
        'HEAD:tests/agent-eval/results',
    ], text=True, cwd=ROOT)
    names = [line.removeprefix('tree ') for line in entries.splitlines() if line.startswith('tree ')]
    selected = result_inputs(names, plan['sources'])
    patterns = {'/tests/agent-eval/results/' + name + '/' for name in selected}
    if plan['guides']:
        docs = runpy.run_path(str(ROOT / 'tools/review-docs.py'))
        patterns.update(result_pattern(path.as_posix()) for path in docs['evidence_links'](docs['guides']()))
    if plan['catalog']:
        catalog = json.loads((ROOT / 'tests/agent-eval/roadmap.json').read_text())
        for row in catalog['evidence']:
            name = row['path']
            path = Path(name)
            if (path.as_posix() != name or path.is_absolute() or '..' in path.parts
                    or not (ROOT / path).resolve().is_relative_to(ROOT.resolve())):
                raise ValueError('invalid catalog input path: ' + name)
            if name.startswith('tests/agent-eval/results/'):
                patterns.add(result_pattern(name))
    print(f'Fetching {len(selected)}/{len(names)} retained directories.', flush=True)
    if patterns:
        subprocess.run(['git', 'sparse-checkout', 'add', '--stdin'],
                       input=''.join(path + '\n' for path in sorted(patterns)),
                       text=True, check=True, timeout=60, cwd=ROOT)
    if plan['catalog']:
        artifacts = set()
        for row in catalog['evidence']:
            path = ROOT / row['path']
            raw = path.read_bytes()
            if hashlib.sha256(raw).hexdigest() != row['sha256']:
                raise ValueError('catalog input identity differs: ' + row['path'])
            value = json.loads(raw)
            files = value['artifacts'] if 'artifacts' in value else value.get('files', {})
            for name in files:
                artifacts.add(result_pattern((path.parent / name).relative_to(ROOT).as_posix()))
            if value['schema'] == 'fr-original-package-questions':
                artifacts.add(result_pattern(path.with_name('receipt.json').relative_to(ROOT).as_posix()))
            if value['schema'] == 'fr-unknown-target-cohort-1':
                for trial in value['trials']:
                    artifacts.add(result_pattern(trial['path'] + '/manifest.json'))
            binding_field = 'bindings' if value['schema'] == 'fr-index-resolution-result-1' else 'source_bindings'
            bindings = value.get(binding_field, {})
            artifacts.update(result_pattern(name) for name in bindings
                             if name.startswith('tests/agent-eval/results/'))
        if artifacts:
            subprocess.run(['git', 'sparse-checkout', 'add', '--stdin'],
                input=''.join(path + '\n' for path in sorted(artifacts)),
                text=True, check=True, timeout=60, cwd=ROOT)
    files = subprocess.check_output(['git', 'ls-files', '-t', 'tests/agent-eval/results'],
                                    text=True, cwd=ROOT).splitlines()
    included = sum(not path.startswith('S ') for path in files)
    print(f'Materialized retained inputs: {included}/{len(files)} result files.',
          flush=True)


def partition(items, count, weights, *, default_weight=10):
    if count < 1 or len(items) != len(set(items)):
        raise ValueError("invalid shard inventory")
    shards = [[] for _ in range(count)]
    loads = [0.0] * count
    for item in sorted(items, key=lambda item: (-weights.get(item, default_weight), item)):
        index = min(range(count), key=lambda index: (loads[index], index))
        shards[index].append(item)
        loads[index] += weights.get(item, default_weight)
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


def native_selection(inventory, index):
    timing = json.loads((ROOT / "tools/ci-test-durations.json").read_text())
    isolated = timing["native_isolated_targets"]
    if (not isinstance(isolated, list) or len(isolated) != len(set(isolated))
            or not set(isolated) <= inventory.keys() or len(isolated) >= COUNTS["native"]):
        raise ValueError("isolated native targets must exist and leave room for the remaining inventory")
    assignments = [[target] for target in isolated] + partition(
        sorted(inventory.keys() - set(isolated)), COUNTS["native"] - len(isolated), timing["seconds"])
    selected = assignments[index]
    if not selected:
        raise ValueError("empty native shard")
    return selected


def run_native(index, directory):
    inventory = native_inventory()
    selected = native_selection(inventory, index)
    command = ["cargo", "test", "--locked", "--no-fail-fast"]
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
        weights = json.loads((ROOT / "tools/ci-test-durations.json").read_text())["sdk_seconds"]
        assigned = set(partition(self.inventory, COUNTS["sdk"], weights, default_weight=1)[self.index])
        selected, deselected = [], []
        for item in items:
            (selected if item.nodeid in assigned else deselected).append(item)
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
    parser.add_argument("kind", choices=[*COUNTS, "verify", "checkout-native", "checkout-sdk"])
    parser.add_argument("index", type=int, nargs="?")
    parser.add_argument("--output", type=Path, default=Path("target/ci-results"))
    args = parser.parse_args()
    os.chdir(ROOT)
    if args.kind.startswith('checkout-'):
        kind = args.kind.removeprefix('checkout-')
        if args.index is None or not 0 <= args.index < COUNTS[kind]:
            parser.error('declare an explicit valid shard index')
        checkout_inputs(checkout_plan(kind, args.index))
        return
    directory = args.output.resolve()
    if args.kind == "verify":
        verify(directory)
        return
    if args.index is None or not 0 <= args.index < COUNTS[args.kind]:
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
