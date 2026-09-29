"""Exercise shard coverage refusal paths without compilers or external runtimes."""

import importlib.util
import json
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("ci_shards", Path(__file__).with_name("ci-shards.py"))
assert spec and spec.loader
shards = importlib.util.module_from_spec(spec)
spec.loader.exec_module(shards)


class ShardTests(unittest.TestCase):
    def test_weighted_partition_is_complete_stable_and_balanced(self):
        items = ["heavy", "medium", "new", "small", "other"]
        weights = {"heavy": 40, "medium": 25, "small": 5, "other": 5}
        result = shards.partition(items, 2, weights)
        self.assertEqual(result, shards.partition(list(reversed(items)), 2, weights))
        self.assertEqual(sorted(sum(result, [])), sorted(items))
        self.assertEqual(sorted(sum(weights.get(item, 10) for item in group)
                                for group in result), [40, 45])
        with self.assertRaises(ValueError):
            shards.partition(["same", "same"], 2, {})

    def test_cargo_inventory_includes_new_targets_and_examples(self):
        kinds = ["rlib", "bin", "test", "example", "bench"]
        package = {"manifest_path": str(shards.ROOT / "Cargo.toml"), "targets": [
            {"kind": [kind], "name": kind} for kind in kinds
        ]}
        with patch.object(shards.subprocess, "check_output", return_value=json.dumps({"packages": [package]})):
            inventory = shards.native_inventory()
        self.assertEqual(len(inventory), 5)
        self.assertEqual(inventory["lib:rlib"], ["--lib"])
        self.assertEqual(inventory["example:example"], ["--example", "example"])

    def test_sdk_collection_covers_parameterized_ids_exactly_once(self):
        inventory = [f"test_sample.py::test_case[{i}]" for i in range(100)]
        assigned = []
        config = SimpleNamespace(hook=SimpleNamespace(pytest_deselected=lambda **kwargs: None))
        for index in range(shards.COUNTS["sdk"]):
            plugin = shards.SdkShard(index, Path("unused"))
            items = [SimpleNamespace(nodeid=nodeid) for nodeid in inventory]
            plugin.pytest_collection_modifyitems(None, config, items)
            self.assertEqual(plugin.inventory, inventory)
            self.assertTrue(plugin.selected)
            assigned.extend(plugin.selected)
        self.assertEqual(sorted(assigned), sorted(inventory))

    def evidence(self, directory):
        for kind, count in shards.COUNTS.items():
            inventory = [str(index) for index in range(count)]
            for index in range(count):
                shards.record(directory, kind, index, inventory, [str(index)])
                (directory / f"{kind}-{index}.log").write_text("rename\tRust\n")
                (directory / f"{kind}-{index}-matrix.json").write_text("[]")

    def test_aggregate_requires_every_test_once_and_matching_inventories(self):
        for corruption in ["none", "missing", "duplicate", "different", "wrong-index", "missing-log", "matrix"]:
            with self.subTest(corruption=corruption), tempfile.TemporaryDirectory() as temp:
                directory = Path(temp)
                self.evidence(directory)
                path = directory / "native-0.json"
                report = json.loads(path.read_text())
                if corruption == "missing":
                    report["selected"] = []
                elif corruption == "duplicate":
                    report["selected"] = ["0", "1"]
                elif corruption == "different":
                    report["inventory"].append("new-test")
                elif corruption == "wrong-index":
                    report["index"] = 5
                elif corruption == "missing-log":
                    (directory / "sdk-1.log").unlink()
                elif corruption == "matrix":
                    (directory / "sdk-1-matrix.json").write_text('[{"changed": true}]')
                path.write_text(json.dumps(report))
                with patch.object(shards.subprocess, "run") as run:
                    if corruption == "none":
                        shards.verify(directory)
                        run.assert_called_once()
                        self.assertTrue(run.call_args.kwargs["check"])
                    else:
                        with self.assertRaises((ValueError, FileNotFoundError)):
                            shards.verify(directory)
                        run.assert_not_called()


if __name__ == "__main__":
    unittest.main()
