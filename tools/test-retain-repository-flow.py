#!/usr/bin/env python3
"""Rehash corrupt cohort receipts and verify the independent replay still refuses."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('retain_flow', ROOT / 'tools/frozen-repository-flow.py')
retain = importlib.util.module_from_spec(spec)
spec.loader.exec_module(retain)


class RetainedCohort(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        override = os.environ.get('FR_REPOSITORY_FLOW_COHORT')
        if override:
            if os.environ.get('GITHUB_ACTIONS') != 'true':
                raise ValueError('unretained reports may only be tested on GitHub')
            cls.report = Path(override)
        else:
            entries = json.loads((ROOT / 'tests/agent-eval/roadmap.json').read_text())['evidence']
            matches = [entry for entry in entries if entry['id'] == 'repository-flow']
            if not matches:
                raise unittest.SkipTest('cohort has not been retained yet')
            cls.report = ROOT / matches[0]['path']

    def changed(self, name, change):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name) / 'cohort'
        shutil.copytree(self.report.parent, root)
        file = root / name
        value = json.loads(file.read_bytes())
        change(value)
        file.write_text(json.dumps(value))
        manifest_path = root / 'manifest.json'
        manifest = json.loads(manifest_path.read_bytes())
        manifest['files'][name] = hashlib.sha256(file.read_bytes()).hexdigest()
        manifest_path.write_text(json.dumps(manifest))
        return root / 'result.json'

    def test_exact_cohort_replays_without_closing_repository_acceptance(self):
        result = retain.audit(self.report)
        self.assertEqual((result['repositories'], result['cases']), (3, 30))
        self.assertFalse(result['acceptance_demonstrated'])

    def test_rehashing_cannot_promote_the_summary(self):
        report = self.changed('result.json', lambda value: value['summary'].update(acceptance_demonstrated=True))
        with self.assertRaisesRegex(ValueError, 'accounting'):
            retain.audit(report)

    def test_runtime_events_are_independently_replayed(self):
        def change(value):
            value['cases'][0]['runtime'][0]['returned'] = 99
        report = self.changed('coverage/result.json', change)
        with self.assertRaisesRegex(ValueError, 'runtime outcome'):
            retain.audit(report)

    def test_incomplete_answer_cannot_be_relabelled_as_no_path(self):
        def change(value):
            row = next(row for row in value['cases'] if row['case'] == 'overwritten')
            row['assessment']['no_path'] = True
        report = self.changed('coverage/result.json', change)
        with self.assertRaisesRegex(ValueError, 'promoted'):
            retain.audit(report)

    def test_a_different_coverage_run_cannot_admit_the_holdout(self):
        report = self.changed('held-out/result.json', lambda value: value['coverage'].update(run_id=1))
        with self.assertRaisesRegex(ValueError, 'admitted'):
            retain.audit(report)

    def test_collector_source_changes_invalidate_the_cohort(self):
        def change(value):
            value['source_bindings']['tools/python-repository-flow.py'] = '0' * 64
        report = self.changed('result.json', change)
        with self.assertRaisesRegex(ValueError, 'source changed'):
            retain.audit(report)


if __name__ == '__main__':
    unittest.main()
