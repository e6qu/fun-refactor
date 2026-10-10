#!/usr/bin/env python3
"""Reject missing, reordered or unearned source-repair delivery evidence."""
import copy
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('repair', ROOT / 'tools/source-repair-context.py')
repair = importlib.util.module_from_spec(spec)
spec.loader.exec_module(repair)
REPORT = ROOT / 'tests/agent-eval/results/2026-10-10-selection-source-repair/result.json'


class SourceRepairEvidence(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        override = os.environ.get('FR_SOURCE_REPAIR_REPORT')
        if override:
            if os.environ.get('GITHUB_ACTIONS') != 'true':
                raise ValueError('unretained reports may only be tested on GitHub')
            cls.report = json.loads(Path(override).read_text())
        else:
            manifest = json.loads((REPORT.parent / 'manifest.json').read_text())
            if manifest['files'] != {'result.json': hashlib.sha256(REPORT.read_bytes()).hexdigest()}:
                raise ValueError('retained source repair digest changed')
            cls.report = json.loads(REPORT.read_text())
            if (cls.report['source_commit'] != manifest['collection']['source_commit']
                    or cls.report['binary_sha256'] != manifest['binary_sha256']):
                raise ValueError('source repair collection identity changed')

    def changed(self, fault=None):
        value = copy.deepcopy(self.report)
        row = next(r for r in value['runs'] if r['arm'] == 'repair-applied' and r['fault'] == fault)
        return value, row

    def reject(self, value):
        with self.assertRaises(ValueError):
            repair.audit(value)

    def test_complete_matrix(self):
        result = repair.audit(self.report)
        self.assertEqual(result['cells'], 40)
        self.assertEqual(result['successful_deliveries'], 8)

    def test_missing_or_duplicate_cell(self):
        for replacement in ([], [self.report['runs'][0]]):
            value = copy.deepcopy(self.report)
            value['runs'] = value['runs'][:-1] + replacement
            self.reject(value)

    def test_initial_failure_stays_in_total_costs(self):
        value, row = self.changed()
        row['total_metrics'] = row['metrics']
        self.reject(value)

    def test_initial_program_stays_in_caller_bytes(self):
        value, row = self.changed()
        row['total_caller_bytes'] = row['caller_bytes']
        self.reject(value)

    def test_incorrect_source_cannot_be_replaced_by_expected_source(self):
        value, row = self.changed()
        row['failed_source'] = row['source']
        self.reject(value)

    def test_repeated_wrong_repair_cannot_deliver(self):
        value, row = self.changed('wrong-repair')
        row['patch'] = 'unearned delivery'
        self.reject(value)

    def test_stale_review_preserves_later_source(self):
        value, row = self.changed('source')
        row['source'] = row['failed_source']
        self.reject(value)

    def test_later_edit_preserved_before_reversal(self):
        value, row = self.changed('later-edit')
        row['source'] = row['failed_source']
        self.reject(value)

    def test_reversal_order_cannot_change(self):
        value, row = self.changed()
        row['transitions'].reverse()
        self.reject(value)

    def test_original_revision_must_be_restored(self):
        value, row = self.changed()
        row['restored']['source_revision'] = 'incorrect original revision'
        self.reject(value)

    def test_final_receipt_must_match_corrected_source(self):
        value, row = self.changed()
        row['history']['records'][0]['check_evidence'][0]['source_revision'] = 'stale'
        for event in row['events']:
            if event['request']['arguments'] == ['history', 'show', str(row['result']['transaction'])]:
                event['response']['report'] = copy.deepcopy(row['history'])
        row['metrics'] = repair.routes.metrics(row['events'])
        row['total_metrics'] = repair.routes.metrics(row['initial_events'] + row['events'])
        self.reject(value)

    def test_patch_order_cannot_change(self):
        value, row = self.changed()
        row['parts'].reverse()
        self.reject(value)

    def test_patch_bytes_cannot_change(self):
        value, row = self.changed()
        row['patch'] += '\n'
        self.reject(value)

    def test_receiver_must_apply_and_run_behavior(self):
        for key in ('check', 'apply', 'behavior'):
            value, row = self.changed()
            row['receiver'][key]['exit_code'] = 1
            self.reject(value)

    def test_actual_conflicting_apply_must_preserve_source(self):
        value, row = self.changed()
        row['receiver']['conflict_after'] = row['receiver']['source']
        self.reject(value)

    def test_incomplete_or_reordered_patches_must_refuse(self):
        for index in range(2):
            value, row = self.changed()
            row['receiver']['incomplete_or_reordered'][index]['apply']['exit_code'] = 0
            self.reject(value)


if __name__ == '__main__':
    unittest.main()
