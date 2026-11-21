"""Check freshness without running collectors or rewriting evidence selectors."""
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('preflight', ROOT / 'tools/evidence-preflight.py')
preflight = importlib.util.module_from_spec(spec)
spec.loader.exec_module(preflight)


class Freshness(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.save('Cargo.toml', '[package]\nname = "example"\nversion = "1.0.0"\n[workspace]\nmembers = ["."]\n')
        self.save('native-source-inputs.txt', 'native-source-inputs.txt\nsrc/\nCargo.toml\n')
        self.save('src/lib.rs', 'pub fn example() {}\n')
        self.save('tools/shared-test.py', '# independently checked examples\n')

    def save(self, name, data):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(data)
        return path

    def report(self, name='result.json', extra=()):
        names = (*preflight.native_source_paths(self.root), *extra)
        bindings = {name: preflight.roadmap.binding_digest(self.root / name, None, self.root) for name in names}
        path = self.save(name, json.dumps({'source_bindings': bindings}))
        return {'path': name, 'selected_by': [name], 'sha256': hashlib.sha256(path.read_bytes()).hexdigest()}

    def inspect(self, selected):
        return preflight.inspect(preflight.Reader(self.root), selected,
                                 set(preflight.native_source_paths(self.root)), {})

    def test_changed_shared_test_lists_each_dependent_report(self):
        selected = [self.report(name, ('tools/shared-test.py',)) for name in ('compiler.json', 'contract.json')]
        self.save('tools/shared-test.py', '# changed examples\n')
        for item in selected:
            row = self.inspect(item)
            self.assertEqual(row['state'], 'stale')
            self.assertEqual(row['changed_sources'], ['tools/shared-test.py'])

    def test_added_native_file_is_stale_even_without_a_declared_hash(self):
        selected = self.report()
        self.save('src/new.rs', 'pub fn added() {}\n')
        self.assertEqual(self.inspect(selected)['changed_sources'], ['src/new.rs'])

    def test_removed_input_is_stale(self):
        selected = self.report()
        (self.root / 'src/lib.rs').unlink()
        self.assertEqual(self.inspect(selected)['changed_sources'], ['src/lib.rs'])

    def test_sparse_historical_receipt_uses_its_git_blob(self):
        name = 'tests/agent-eval/results/earlier/receipt.json'
        data = b'{"receipt": true}'
        self.save('result.json', json.dumps({'source_bindings': {name: hashlib.sha256(data).hexdigest()}}))
        reader = preflight.Reader(self.root)
        with patch.object(preflight.subprocess, 'check_output', side_effect=[str(len(data)).encode(), data]):
            row = preflight.inspect(reader, {'path': 'result.json', 'selected_by': ['current']}, set(), {})
        self.assertEqual(row['state'], 'current')

    def test_release_version_is_normalized(self):
        selected = self.report()
        path = self.root / 'Cargo.toml'
        path.write_text(path.read_text().replace('1.0.0', '1.0.1'))
        self.assertEqual(self.inspect(selected)['state'], 'current')

    def test_report_schemas_keep_their_own_python_version_normalization(self):
        name = 'sdk/python/src/fr_ir/_version.py'
        path = self.save(name, 'VERSION = "1.0.0"  # x-release-please-version\n')
        digests = {}
        reader = preflight.Reader(self.root)
        for index, schema in enumerate(('fr-python-contract-3', 'fr-python-repository-acceptance-2')):
            report = f'version-{index}.json'
            self.save(report, json.dumps({'schema': schema, 'source_bindings': {
                name: preflight.roadmap.binding_digest(path, schema, self.root)}}))
            row = preflight.inspect(reader, {'path': report, 'selected_by': [schema]}, set(), digests)
            self.assertEqual(row['state'], 'current')
        self.assertEqual(len(digests), 2)

    def test_storage_reports_bind_exact_python_release_bytes(self):
        name = 'sdk/python/src/fr_ir/_version.py'
        path = self.save(name, 'VERSION = "1.0.0"  # x-release-please-version\n')
        fingerprint = hashlib.sha256(path.read_bytes()).hexdigest()
        selected = []
        for schema in ('fr-saved-work-cost', 'fr-saved-answer-improvement'):
            report = schema + '.json'
            self.save(report, json.dumps({'schema': schema, 'source_bindings': {name: fingerprint}}))
            selected.append({'path': report, 'selected_by': [schema]})
            self.assertEqual(self.inspect(selected[-1])['state'], 'current')
        path.write_text('VERSION = "1.0.1"  # x-release-please-version\n')
        for row in selected:
            self.assertEqual(self.inspect(row)['state'], 'stale')
            self.assertEqual(self.inspect(row)['changed_sources'], [name])

    def test_digest_tampering_and_missing_reports_are_errors(self):
        selected = self.report()
        self.save('result.json', '{}')
        self.assertEqual(self.inspect(selected)['state'], 'error')
        (self.root / 'result.json').unlink()
        self.assertEqual(self.inspect(selected)['state'], 'error')

    def test_escaping_dependency_is_an_error(self):
        self.save('result.json', json.dumps({'bindings': {'../outside.py': '0' * 64}}))
        row = self.inspect({'path': 'result.json', 'selected_by': ['unsafe']})
        self.assertEqual(row['state'], 'error')
        self.assertIn('unsafe path', row['error'])

    def test_archived_binding_sources_are_checked_without_loading_archives(self):
        before = hashlib.sha256((self.root / 'src/lib.rs').read_bytes()).hexdigest()
        self.save('manifest.json', json.dumps({'files': {'binding-sources/src/lib.rs': before}}))
        selected = {'path': 'manifest.json', 'selected_by': ['archive']}
        self.assertEqual(self.inspect(selected)['state'], 'current')
        self.save('src/lib.rs', 'pub fn changed() {}')
        self.assertEqual(self.inspect(selected)['changed_sources'], ['src/lib.rs'])

    def test_unbound_report_does_not_claim_freshness(self):
        self.save('result.json', '{}')
        self.assertEqual(self.inspect({'path': 'result.json', 'selected_by': ['unbound']})['state'],
                         'not-source-bound')

    def test_historical_catalog_records_are_not_current_selectors(self):
        self.assertTrue({'repository-flow', 'project-changes', 'rust-live-cohort',
                         'saved-work-cost'}.isdisjoint(preflight.CURRENT_CATALOG))

    def test_current_storage_comparison_detects_changed_sdk_inputs(self):
        self.assertIn('current-answer-storage-cost', preflight.CURRENT_CATALOG)
        name = 'sdk/python/src/fr_ir/flow_completion.py'
        self.save(name, 'CLASSES = ("ValueError",)\n')
        selected = self.report(extra=(name,))
        selected['selected_by'] = ['current-answer-storage-cost']
        self.assertEqual(self.inspect(selected)['state'], 'current')
        self.save(name, 'CLASSES = ("IndexError", "ValueError")\n')
        row = self.inspect(selected)
        self.assertEqual(row['state'], 'stale')
        self.assertEqual(row['changed_sources'], [name])

    def test_report_reads_have_a_size_limit(self):
        path = self.root / 'oversized.json'
        with path.open('wb') as stream:
            stream.truncate(preflight.LIMIT + 1)
        with self.assertRaisesRegex(ValueError, 'read budget'):
            preflight.Reader(self.root).read('oversized.json')

    def test_reports_do_not_accumulate_in_the_read_budget(self):
        selected = [self.report(name) for name in ('first.json', 'second.json')]
        size = sum((self.root / row['path']).stat().st_size for row in selected)
        with patch.object(preflight, 'selections', return_value=selected), \
                patch.object(preflight, 'TOTAL_LIMIT', size - 1):
            self.assertTrue(preflight.compute(self.root)['passed'])


if __name__ == '__main__':
    unittest.main()
