"""Mutation checks for honest evidence classification and finite milestone gates."""
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'tools'))
spec = importlib.util.spec_from_file_location('roadmap_status', ROOT / 'tools/roadmap-status.py')
status = importlib.util.module_from_spec(spec)
spec.loader.exec_module(status)


class RoadmapStatusTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        (self.root / 'input.py').write_text('def f(): return 1\n')
        self.value = {'schema': 'fr-package-flow-acceptance-1', 'complete': True,
                      'source_bindings': {'input.py': status.file_digest(self.root / 'input.py')}}
        (self.root / 'result.json').write_text(json.dumps(self.value))
        (self.root / 'PLAN.md').write_text('\n'.join(f'### {m}. Title\n- [x] Implementation\n' for m in 'ABCD'))
        entry = {'id': 'result', 'path': 'result.json', 'kind': 'deterministic', 'audit': 'independent evaluator',
                 'scope': 'finite fixture', 'sha256': status.digest(self.root / 'result.json')}
        self.catalog = {'schema': 'fr-roadmap-catalog-1', 'profile': 'agent-analysis-v2', 'baseline': 'pinned',
            'engineering': {m: {'done': 1, 'total': 1} for m in 'ABCD'}, 'evidence': [entry], 'boundaries': [],
            'obligations': [{'id': f'{m}.outcome', 'kind': 'deterministic', 'acceptance': 'behavior passes',
                'next': 'retain evidence', 'requires': [{'evidence': 'result',
                    'assertions': [{'pointer': '/complete', 'equals': True}]}]} for m in 'ABCD']}

    def compute(self):
        return status.compute(self.root, self.catalog)

    def rewrite(self):
        (self.root / 'result.json').write_text(json.dumps(self.value))
        self.catalog['evidence'][0]['sha256'] = status.digest(self.root / 'result.json')

    def test_all_clauses_required_even_when_engineering_is_done(self):
        self.assertEqual(self.compute()['milestones_complete'], 4)
        self.catalog['obligations'].append({'id': 'A.additional', 'kind': 'source-proof',
            'acceptance': 'checked correspondence', 'next': 'prove it', 'requires': []})
        report = self.compute()
        self.assertEqual(report['milestones_complete'], 3)
        self.assertEqual(report['milestones']['A']['remaining'], ['A.additional'])

    def test_demonstrated_gates_do_not_hide_unfinished_engineering(self):
        path = self.root / 'PLAN.md'
        path.write_text(path.read_text().replace('[x]', '[ ]', 1))
        self.catalog['engineering']['A']['done'] = 0
        report = self.compute()
        self.assertEqual(report['milestones']['A']['remaining'], [])
        self.assertEqual(report['milestones']['A']['engineering_remaining'], 1)
        self.assertEqual(report['milestones']['A']['state'], 'open')
        self.assertEqual(report['milestones_complete'], 3)

    def test_missing_report_opens_all_dependents(self):
        (self.root / 'result.json').unlink()
        self.assertEqual(self.compute()['milestones_complete'], 0)

    def test_tampered_report_is_an_error(self):
        (self.root / 'result.json').write_text('{}')
        with self.assertRaisesRegex(ValueError, 'tampered evidence'):
            self.compute()

    def test_source_drift_and_deleted_inputs_are_stale(self):
        (self.root / 'input.py').write_text('def f(): return 2\n')
        self.assertEqual(self.compute()['milestones_complete'], 0)
        (self.root / 'input.py').unlink()
        self.assertEqual(self.compute()['evidence'][0]['changed_sources'], ['input.py'])

    def test_unbound_historical_report_does_not_close_current_gate(self):
        self.value.pop('source_bindings')
        self.rewrite()
        self.assertEqual(self.compute()['evidence'][0]['freshness'], 'historical')
        self.assertEqual(self.compute()['milestones_complete'], 0)

    def test_failure_cannot_be_relabelled_by_rehashing(self):
        self.value['complete'] = False
        self.rewrite()
        self.assertEqual(self.compute()['milestones_complete'], 0)

    def test_missing_assertion_and_boolean_integer_confusion(self):
        for value in (1, None):
            self.value['complete'] = value
            self.rewrite()
            self.assertEqual(self.compute()['milestones_complete'], 0)
        self.value.pop('complete')
        self.rewrite()
        self.assertEqual(self.compute()['milestones_complete'], 0)

    def test_kind_separation_rejects_live_or_source_proof_claim(self):
        for kind in ('live-agent', 'source-proof'):
            self.catalog['evidence'][0]['kind'] = kind
            with self.assertRaisesRegex(ValueError, 'kind/schema mismatch'):
                self.compute()
        self.catalog['evidence'][0]['kind'] = 'deterministic'
        self.catalog['obligations'][0]['kind'] = 'source-proof'
        with self.assertRaisesRegex(ValueError, 'wrong evidence kind'):
            self.compute()

    def test_duplicate_ids_and_unknown_references(self):
        self.catalog['obligations'].append(copy.deepcopy(self.catalog['obligations'][0]))
        with self.assertRaisesRegex(ValueError, 'duplicate obligation'):
            self.compute()
        self.catalog['obligations'].pop()
        self.catalog['evidence'].append(copy.deepcopy(self.catalog['evidence'][0]))
        with self.assertRaisesRegex(ValueError, 'duplicate evidence'):
            self.compute()
        self.catalog['evidence'].pop()
        self.catalog['obligations'][0]['requires'][0]['evidence'] = 'absent'
        with self.assertRaisesRegex(ValueError, 'unknown evidence'):
            self.compute()

    def test_empty_milestone_or_unasserted_requirement_refuses(self):
        self.catalog['obligations'].pop()
        with self.assertRaisesRegex(ValueError, 'empty milestone'):
            self.compute()
        self.catalog['obligations'][0]['requires'][0]['assertions'] = []
        with self.assertRaisesRegex(ValueError, 'explicit evidence assertions'):
            self.compute()

    def test_manifest_artifacts_are_not_trusted_from_top_level_hash(self):
        self.value['artifacts'] = {'receipt.json': '0' * 64}
        self.rewrite()
        with self.assertRaisesRegex(ValueError, 'tampered artifact'):
            self.compute()

    def test_path_escape_refuses(self):
        self.value['source_bindings'] = {'../outside': '0' * 64}
        self.rewrite()
        with self.assertRaisesRegex(ValueError, 'unsafe path'):
            self.compute()

    def test_release_versions_follow_the_cache_evaluator_normalization(self):
        path = self.root / 'Cargo.toml'
        def contents(version, remote):
            return (f'[package]\nname="example"\nversion="{version}"\n[dependencies]\n'
                    f'child={{path="child",version="{version}"}}\nremote="{remote}"\n')
        path.write_text(contents('1.0.0', '2.0.0'))
        expected = status.binding_digest(path, 'fr-flow-cache-acceptance-1')
        path.write_text(contents('1.1.0', '2.0.0'))
        self.assertEqual(expected, status.binding_digest(path, 'fr-flow-cache-acceptance-1'))
        path.write_text(contents('1.1.0', '3.0.0'))
        self.assertNotEqual(expected, status.binding_digest(path, 'fr-flow-cache-acceptance-1'))

    def test_plan_checkbox_changes_require_catalog_review(self):
        path = self.root / 'PLAN.md'
        path.write_text(path.read_text().replace('[x]', '[ ]', 1))
        with self.assertRaisesRegex(ValueError, 'engineering counts drifted'):
            self.compute()


class DogfoodReceiptTests(unittest.TestCase):
    def test_retained_edits_bind_review_save_and_application(self):
        root = ROOT / 'tests/agent-eval/results/2026-09-30-roadmap-dogfood'
        manifest = json.loads((root / 'manifest.json').read_text())
        actual = {p.name: status.digest(p) for p in root.iterdir() if p.name != 'manifest.json'}
        self.assertEqual(manifest['files'], actual)
        saved = sorted(root.glob('*-saved.json'))
        self.assertGreaterEqual(len(saved), 8)
        for path in saved:
            name = path.name.removesuffix('-saved.json')
            review = json.loads((root / f'{name}-preview.json').read_text())
            retained = json.loads(path.read_text())
            applied = json.loads((root / f'{name}-applied.json').read_text())
            self.assertEqual(review['plan_context_basis'], retained['plan_context_basis'])
            self.assertTrue(retained['saved'])
            self.assertEqual(retained['transaction'], applied['transaction'])
            self.assertEqual(retained['transaction_context_basis'], applied['context_basis'])
            self.assertTrue(applied['applied'])


if __name__ == '__main__':
    unittest.main()
