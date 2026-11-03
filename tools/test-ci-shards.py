"""Exercise shard coverage refusal paths without compilers or external runtimes."""

import importlib.util
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("ci_shards", Path(__file__).with_name("ci-shards.py"))
assert spec and spec.loader
shards = importlib.util.module_from_spec(spec)
spec.loader.exec_module(shards)


class ShardTests(unittest.TestCase):
    def test_input_selection_follows_only_assigned_sources_and_their_current_reports(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            files = {
                'tests/one.rs': '"tools/needed.py"; retained::report("flow-facts")',
                'tests/two.rs': '"tests/agent-eval/results/unused-history"',
                'tools/needed.py': 'import utility\nFIXTURE = "tests/agent-eval/fixture"\nREGISTRY = "tests/agent-eval/registry.json"',
                'tools/utility.py': 'RECORD = "tests/agent-eval/results/needed-history"',
                'tools/unrelated.py': 'RECORD = "tests/agent-eval/results/unused-history"',
                'sdk/python/tests/test_one.py': 'subprocess.run(["python3", "tools/test-python-contract.py"])',
                'tests/agent-eval/fixture/record.json': '{"directory":"tests/agent-eval/results/fixture-history"}',
                'tests/agent-eval/registry.json': '{"manifest":"tests/agent-eval/results/registry-history/manifest.json"}',
                'tests/agent-eval/unrelated-registry.json': '{"manifest":"tests/agent-eval/results/unused-history/manifest.json"}',
                'tests/agent-eval/roadmap.json': '{"evidence":[{"id":"flow-facts","path":"tests/agent-eval/results/current-flow/result.json"},{"id":"python-contract","path":"tests/agent-eval/results/current-contract/result.json"}]}',
                'tests/agent-eval/current-scalar-reports.json': '{"reports":{"flow-facts":{"path":"tests/agent-eval/results/current-flow/result.json"}}}',
            }
            for name, content in files.items():
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(content)
            metadata = {'packages': [{'manifest_path': str(root / 'Cargo.toml'), 'targets': [
                {'kind': ['test'], 'name': name, 'src_path': str(root / ('tests/' + name + '.rs'))}
                for name in ('one', 'two')]}]}
            with patch.object(shards, 'ROOT', root), \
                 patch.object(shards, 'native_inventory', return_value={}), \
                 patch.object(shards, 'native_selection', return_value=['test:one']), \
                 patch.object(shards.subprocess, 'check_output', return_value=json.dumps(metadata)):
                plan = shards.checkout_plan('native', 0)
                self.assertEqual(shards.result_inputs(
                    ['needed-history', 'fixture-history', 'registry-history', 'current-flow', 'unused-history'], plan['sources']),
                    ['current-flow', 'fixture-history', 'needed-history', 'registry-history'])
                sdk = shards.checkout_plan('sdk', 0)
                self.assertEqual(shards.result_inputs(
                    ['needed-history', 'fixture-history', 'registry-history', 'unused-history', 'current-contract'], sdk['sources']),
                    ['current-contract'])
                self.assertFalse(sdk['catalog'])
                self.assertFalse(sdk['guides'])

    def test_sparse_checkout_restores_catalog_and_joined_inputs_without_unreferenced_archives(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            def git(*args, **kwargs):
                return subprocess.run(['git', *args], cwd=root, check=True, capture_output=True, **kwargs)
            artifact = 'declared artifact'
            record = json.dumps({'schema': 'fr-checkout-fixture', 'files': {
                'artifact.bin': hashlib.sha256(artifact.encode()).hexdigest()},
                'source_bindings': {'tests/agent-eval/results/binding/receipt.json': 'checked by the auditor'}})
            cohort = json.dumps({'schema': 'fr-unknown-target-cohort-1', 'trials': [{
                'path': 'tests/agent-eval/results/cohort-trial'}]})
            files = {
                'tests/agent-eval/roadmap.json': json.dumps({'evidence': [{
                    'id': 'catalog-only', 'path': 'tests/agent-eval/results/catalog-only/result.json',
                    'sha256': hashlib.sha256(record.encode()).hexdigest()}, {
                    'id': 'cohort', 'path': 'tests/agent-eval/unknown-target/cohort.json',
                    'sha256': hashlib.sha256(cohort.encode()).hexdigest()}]}),
                'tests/agent-eval/unknown-target/cohort.json': cohort,
                'tests/agent-eval/results/cohort-trial/manifest.json': 'declared trial',
                'tests/agent-eval/results/cohort-trial/unreferenced.bin': 'unused trial payload',
                'tests/example.rs': 'root.join("tests/agent-eval/results").join("2026-09-17-joined")',
                'tests/agent-eval/results/current/result.json': 'current record',
                'tests/agent-eval/results/2026-09-17-joined/result.json': 'joined record',
                'tests/agent-eval/results/unreferenced/result.json': 'unused old record',
                'tests/agent-eval/results/catalog-only/result.json': record,
                'tests/agent-eval/results/catalog-only/artifact.bin': artifact,
                'tests/agent-eval/results/catalog-only/unreferenced.bin': 'unused payload',
                'tests/agent-eval/results/binding/receipt.json': 'declared receipt',
                'tests/agent-eval/results/binding/unreferenced.bin': 'unused payload',
                'tests/agent-eval/results/documented/README.md': 'linked guide',
                'tests/agent-eval/results/documented/unreferenced.bin': 'unused payload',
            }
            for name, content in files.items():
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(content)
            git('init', '-q')
            git('add', '.')
            git('-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid',
                'commit', '-qm', 'Record checkout fixtures')
            git('sparse-checkout', 'set', '--no-cone', '--stdin',
                input='/*\n!/tests/agent-eval/results/*\n', text=True)
            self.assertFalse((root / 'tests/agent-eval/results/current/result.json').exists())
            docs = {'guides': lambda: [], 'evidence_links': lambda pages: [
                Path('tests/agent-eval/results/documented/README.md')]}
            with patch.object(shards, 'ROOT', root), patch.object(shards.runpy, 'run_path', return_value=docs):
                shards.checkout_inputs({'sources': [files['tests/example.rs'],
                    'tests/agent-eval/results/current/result.json'], 'guides': True, 'catalog': True})
            for name in ('current', '2026-09-17-joined'):
                path = 'tests/agent-eval/results/' + name + '/result.json'
                self.assertEqual((root / path).read_text(), files[path])
            self.assertFalse((root / 'tests/agent-eval/results/unreferenced').exists())
            for name in ('catalog-only/artifact.bin', 'binding/receipt.json', 'documented/README.md',
                         'cohort-trial/manifest.json'):
                path = 'tests/agent-eval/results/' + name
                self.assertEqual((root / path).read_text(), files[path])
            for directory in ('catalog-only', 'binding', 'documented', 'cohort-trial'):
                self.assertFalse((root / 'tests/agent-eval/results' / directory / 'unreferenced.bin').exists())

    def test_two_noncone_checkouts_preserve_shared_task_inputs(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            def git(*args, **kwargs):
                return subprocess.run(['git', *args], cwd=root, check=True, capture_output=True, **kwargs)
            files = {'tests/agent-eval/project-edits/comparison/patch.diff': 'approved patch',
                     'tests/agent-eval/selected-agents/tasks.json': 'required shared tasks',
                     'tests/agent-eval/results/unneeded/data': 'unused archive'}
            for name, value in files.items():
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(value)
            git('init', '-q')
            git('add', '.')
            git('-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid',
                'commit', '-qm', 'Record frozen checkout inputs')
            git('sparse-checkout', 'set', '--no-cone', '--stdin',
                input='/tests/agent-eval/project-edits/comparison/\n', text=True)
            task = root / 'tests/agent-eval/selected-agents/tasks.json'
            self.assertFalse(task.exists())
            sparse = root / git('rev-parse', '--git-path', 'info/sparse-checkout', text=True).stdout.strip()
            with sparse.open('a') as stream:
                stream.write('\n/*\n!/tests/agent-eval/results/\n')
            git('checkout', '--force', 'HEAD')
            self.assertEqual(task.read_text(), 'required shared tasks')
            self.assertFalse((root / 'tests/agent-eval/results/unneeded').exists())

    def test_historical_audit_uses_original_checker_from_a_partial_clone_and_cleans_up(self):
        spec = importlib.util.spec_from_file_location('historical_index_audit',
            Path(__file__).with_name('audit-historical-index-resolution.py'))
        audit = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(audit)
        with tempfile.TemporaryDirectory() as temporary:
            origin = Path(temporary) / 'origin'
            origin.mkdir()
            def git(cwd, *args, **kwargs):
                return subprocess.run(['git', *args], cwd=cwd, check=True, capture_output=True, **kwargs)
            checker = ('import hashlib,json,sys\nfrom pathlib import Path\n'
                       'report=json.loads(Path(sys.argv[-1]).read_text())\n'
                       'for name, expected in report["bindings"].items():\n'
                       ' assert hashlib.sha256(Path(name).read_bytes()).hexdigest()==expected\n')
            files = {'tools/index-resolution-acceptance.py': checker, 'fixture.py': 'original inputs',
                     'tests/agent-eval/index-resolution/baseline.json': '{}'}
            report_path = 'tests/agent-eval/results/original/result.json'
            report = json.dumps({'bindings': {name: hashlib.sha256(value.encode()).hexdigest()
                                             for name, value in files.items()}})
            files[report_path] = report
            files['tests/agent-eval/roadmap.json'] = json.dumps({'evidence': [{
                'id': 'index-resolution', 'path': report_path,
                'sha256': hashlib.sha256(report.encode()).hexdigest()}]})
            for name, value in files.items():
                path = origin / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(value)
            git(origin, 'init', '-q')
            git(origin, 'config', 'uploadpack.allowFilter', 'true')
            git(origin, 'add', '.')
            git(origin, '-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid',
                'commit', '-qm', 'Record original audit inputs')
            revision = git(origin, 'rev-parse', 'HEAD', text=True).stdout.strip()
            (origin / 'fixture.py').write_text('changed current inputs')
            (origin / 'current.txt').write_text('current checkout stays untouched')
            git(origin, 'add', '.')
            git(origin, '-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid',
                'commit', '-qm', 'Change current audit inputs')
            checkout = Path(temporary) / 'checkout'
            git(Path(temporary), 'clone', '--quiet', '--filter=blob:none', '--no-checkout', origin.as_uri(), str(checkout))
            git(checkout, 'config', 'protocol.file.allow', 'always')
            git(checkout, 'sparse-checkout', 'set', '--no-cone', '--stdin', input='/current.txt\n', text=True)
            git(checkout, 'checkout', '--quiet')
            self.assertFalse((checkout / 'fixture.py').exists())
            before = git(checkout, 'worktree', 'list', '--porcelain', text=True).stdout
            with patch.object(audit, 'ROOT', checkout), patch.object(audit, 'REVISION', revision):
                audit.audit(revision)
            self.assertFalse((checkout / 'fixture.py').exists())
            self.assertEqual((checkout / 'current.txt').read_text(), 'current checkout stays untouched')
            self.assertEqual(git(checkout, 'worktree', 'list', '--porcelain', text=True).stdout, before)

    def test_checkout_includes_paths_and_joined_names_without_historical_prefix_matches(self):
        names = ['2026-09-07', '2026-09-07-context', '2026-09-17-completion-acceptance',
                 'current-flow', 'unused-history']
        sources = ['"tests/agent-eval/results/2026-09-07-context/result.json"',
                   'root.join("tests/agent-eval/results").join("2026-09-17-completion-acceptance")',
                   '{"path":"tests/agent-eval/results/current-flow/result.json"}']
        self.assertEqual(shards.result_inputs(names, sources),
                         ['2026-09-07-context', '2026-09-17-completion-acceptance', 'current-flow'])
        self.assertEqual(shards.result_inputs(list(reversed(names)), sources),
                         shards.result_inputs(names, sources))
        for name in ['../escape', 'nested/name', '-option', 'bad\nname']:
            with self.assertRaises(ValueError):
                shards.result_inputs([name], sources)

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

    def test_sdk_collection_balances_slow_cases_and_keeps_execution_order(self):
        inventory = [f"tests/sample.py::test_case[{index}]" for index in range(40)]
        weights = {nodeid: 100 for nodeid in inventory[:4]}
        config = SimpleNamespace(hook=SimpleNamespace(pytest_deselected=lambda **kwargs: None))
        assignments = []
        with tempfile.TemporaryDirectory() as temp, patch.object(shards, "ROOT", Path(temp)):
            tools = Path(temp) / "tools"
            tools.mkdir()
            (tools / "ci-test-durations.json").write_text(json.dumps({"sdk_seconds": weights}))
            for index in range(shards.COUNTS["sdk"]):
                plugin = shards.SdkShard(index, Path("unused"))
                items = [SimpleNamespace(nodeid=nodeid) for nodeid in inventory]
                plugin.pytest_collection_modifyitems(None, config, items)
                self.assertEqual(len(set(plugin.selected) & weights.keys()), 1)
                self.assertEqual(plugin.selected, [nodeid for nodeid in inventory if nodeid in plugin.selected])
                shuffled = [SimpleNamespace(nodeid=nodeid) for nodeid in reversed(inventory)]
                reverse = shards.SdkShard(index, Path("unused"))
                reverse.pytest_collection_modifyitems(None, config, shuffled)
                self.assertEqual(set(plugin.selected), set(reverse.selected))
                assignments.extend(plugin.selected)
        self.assertEqual(sorted(assignments), sorted(inventory))

    def test_recorded_sdk_weights_use_pytest_node_ids(self):
        data = json.loads((shards.ROOT / "tools/ci-test-durations.json").read_text())
        self.assertIsInstance(data["sdk_source_run"], int)
        self.assertGreater(data["sdk_source_run"], 0)
        self.assertTrue(data["sdk_seconds"])
        for nodeid, seconds in data["sdk_seconds"].items():
            self.assertTrue(nodeid.startswith("tests/") and "::test_" in nodeid)
            self.assertTrue((shards.ROOT / "sdk/python" / nodeid.split("::")[0]).is_file())
            self.assertGreater(seconds, 0)

    def test_failed_native_tests_do_not_publish_a_passing_inventory(self):
        inventory = {f"test:case{index}": ["--test", f"case{index}"] for index in range(12)}
        failure = subprocess.CalledProcessError(101, ["cargo", "test"])
        with tempfile.TemporaryDirectory() as temp, \
                patch.object(shards, "native_inventory", return_value=inventory), \
                patch.object(Path, "read_text", return_value=json.dumps({"native_isolated_targets": [], "seconds": {}})), \
                patch.object(shards.subprocess, "run", side_effect=failure) as run, \
                patch.object(shards, "record") as record:
            with self.assertRaises(subprocess.CalledProcessError):
                shards.run_native(0, Path(temp))
            command = run.call_args.args[0]
            self.assertIn("--no-fail-fast", command[:command.index("--")])
            self.assertTrue(run.call_args.kwargs["check"])
            record.assert_not_called()

    def test_heavy_proof_target_runs_alone_and_every_other_target_still_runs_once(self):
        inventory = {name: ['--test', name.removeprefix('test:')] for name in
                     ['test:formal_kernel', *[f'test:case{i}' for i in range(40)]]}
        seen = []
        for index in range(shards.COUNTS['native']):
            with patch.object(shards, 'native_inventory', return_value=inventory), \
                    patch.object(shards.subprocess, 'run'), patch.object(shards, 'record') as record:
                shards.run_native(index, Path('unused'))
            selected = record.call_args.args[-1]
            if index == 0:
                self.assertEqual(selected, ['test:formal_kernel'])
            else:
                self.assertNotIn('test:formal_kernel', selected)
            seen.extend(selected)
        self.assertEqual(sorted(seen), sorted(inventory))

    def test_missing_isolated_target_fails_instead_of_omitting_it(self):
        with patch.object(shards, 'native_inventory', return_value={'test:other': ['--test', 'other']}), \
                self.assertRaisesRegex(ValueError, 'isolated native targets'):
            shards.run_native(0, Path('unused'))

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
