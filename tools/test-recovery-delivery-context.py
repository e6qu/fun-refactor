#!/usr/bin/env python3
"""Audit retained recovery delivery and reject incomplete or corrupted evidence."""
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('recovery', ROOT / 'tools/recovery-delivery-context.py')
recovery = importlib.util.module_from_spec(spec)
spec.loader.exec_module(recovery)
REPORT = ROOT / 'tests/agent-eval/results/2026-10-10-recovery-delivery/recovery-delivery-context.json'


class RecoveryEvidence(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        manifest = json.loads((REPORT.parent / 'manifest.json').read_text())
        for name, digest in manifest['files'].items():
            path = (REPORT.parent / name).resolve()
            if not path.is_relative_to(REPORT.parent.resolve()) or hashlib.sha256(path.read_bytes()).hexdigest() != digest:
                raise ValueError('artifact digest changed')
        cls.report = json.loads(REPORT.read_text())
        if (cls.report['source_commit'] != manifest['collection']['source_commit']
                or cls.report['binary_sha256'] != manifest['binary_sha256']):
            raise ValueError('collection identity changed')

    def changed(self, fault=None):
        value = copy.deepcopy(self.report)
        row = next(r for r in value['runs'] if r['arm'] == 'resume' and r['fault'] == fault)
        return value, row

    def test_complete_matrix_and_receiver_delivery(self):
        result = recovery.audit(self.report)
        self.assertEqual(result['cells'], 32)
        self.assertEqual(result['successful_deliveries'], 8)

    def test_missing_and_duplicate_cells_refuse(self):
        for replacement in ([], [self.report['runs'][0]]):
            value = copy.deepcopy(self.report)
            value['runs'] = value['runs'][:-1] + replacement
            with self.assertRaisesRegex(ValueError, 'missing or duplicate'):
                recovery.audit(value)

    def test_initial_failed_work_cannot_disappear_from_costs(self):
        value, row = self.changed()
        row['total_metrics'] = row['metrics']
        with self.assertRaisesRegex(ValueError, 'accounting'):
            recovery.audit(value)

    def test_complete_caller_size_is_recomputed(self):
        value, row = self.changed()
        row['caller_bytes'] -= 1
        with self.assertRaisesRegex(ValueError, 'accounting'):
            recovery.audit(value)

    def test_initial_applied_failure_cannot_claim_original_source(self):
        value, row = self.changed()
        row['failed_source'] = row['receiver']['before']
        with self.assertRaisesRegex(ValueError, 'initial applied failure'):
            recovery.audit(value)

    def test_repeated_check_failure_cannot_deliver(self):
        value, row = self.changed('check')
        row['patch'] = 'unearned patch'
        with self.assertRaisesRegex(ValueError, 'failed recovery'):
            recovery.audit(value)

    def test_stale_review_must_preserve_later_source(self):
        value, row = self.changed('source')
        row['source'] = row['failed_source']
        with self.assertRaisesRegex(ValueError, 'failed recovery'):
            recovery.audit(value)

    def test_patch_bytes_must_match_delivery_receipt(self):
        value, row = self.changed()
        row['patch'] += '\n'
        with self.assertRaisesRegex(ValueError, 'patch differs'):
            recovery.audit(value)

    def test_receiver_must_apply_and_check_the_delivered_source(self):
        for field in ('apply', 'behavior'):
            value, row = self.changed()
            row['receiver'][field]['exit_code'] = 1
            with self.assertRaisesRegex(ValueError, 'receiver replay'):
                recovery.audit(value)

    def test_required_check_receipts_cannot_change_with_their_response(self):
        value, row = self.changed()
        row['history']['records'][0]['check_evidence'][0]['checks'] = ['weaker']
        row['events'][-1]['response']['report'] = copy.deepcopy(row['history'])
        row['metrics'] = recovery.routes.metrics(row['events'])
        row['total_metrics'] = recovery.routes.metrics(row['initial_events'] + row['events'])
        with self.assertRaisesRegex(ValueError, 'required check receipts'):
            recovery.audit(value)

    def test_resume_must_use_the_reviewed_transaction(self):
        value, row = self.changed()
        row['events'][0]['request']['arguments'][2] = '999'
        row['metrics'] = recovery.routes.metrics(row['events'])
        row['total_metrics'] = recovery.routes.metrics(row['initial_events'] + row['events'])
        with self.assertRaisesRegex(ValueError, 'reviewed transaction'):
            recovery.audit(value)

    def test_receiver_conflicts_must_be_preserved(self):
        value, row = self.changed()
        row['receiver']['conflict_after'] = row['receiver']['source']
        with self.assertRaisesRegex(ValueError, 'receiver replay'):
            recovery.audit(value)


if __name__ == '__main__':
    unittest.main()
