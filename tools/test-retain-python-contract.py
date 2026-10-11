#!/usr/bin/env python3
"""Reject swapped, corrupted and unsafe hosted evidence artifacts."""
import copy
import hashlib
import importlib.util
import io
from pathlib import Path
import unittest
import zipfile

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('retain', ROOT / 'tools/retain-python-contract.py')
retain = importlib.util.module_from_spec(spec)
spec.loader.exec_module(retain)


class ArtifactIdentity(unittest.TestCase):
    def archive(self, names=('result.json',)):
        output = io.BytesIO()
        with zipfile.ZipFile(output, 'w') as zipped:
            for name in names:
                zipped.writestr(name, b'{"contract_complete":false}')
        data = output.getvalue()
        return data, 'sha256:' + hashlib.sha256(data).hexdigest()

    def identity(self):
        run = {'id': 7, 'run_attempt': 2, 'html_url': 'https://github.com/e6qu/fun-refactor/actions/runs/7',
               'head_sha': 'a' * 40, 'status': 'completed', 'conclusion': 'success',
               'path': '.github/workflows/refinement-evidence.yml',
               'repository': {'full_name': 'e6qu/fun-refactor'},
               'head_repository': {'full_name': 'e6qu/fun-refactor'}}
        artifact = {'id': 8, 'name': 'python-contract', 'expired': False, 'digest': 'sha256:' + 'b' * 64,
                    'created_at': '2026-10-11T08:01:00Z', 'workflow_run': {'id': 7, 'head_sha': 'a' * 40}}
        jobs = [{'id': 9, 'name': 'python-contract', 'run_id': 7, 'run_attempt': 2,
                 'status': 'completed', 'conclusion': 'success',
                 'started_at': '2026-10-11T08:00:00Z', 'completed_at': '2026-10-11T08:02:00Z'}]
        return run, artifact, {'source_commit': 'a' * 40}, jobs

    def test_exact_report_bytes_and_run_attempt_are_preserved(self):
        data, digest = self.archive()
        self.assertEqual(retain.checked_report(data, digest), b'{"contract_complete":false}')
        run, artifact, report, jobs = self.identity()
        identity = retain.collection(run, artifact, report, 'e6qu/fun-refactor', jobs)
        self.assertEqual((identity['run_id'], identity['attempt'], identity['artifact_id']), ('7', 2, 8))
        self.assertEqual(identity['artifact_digest'], artifact['digest'])

    def test_corrupt_archive_is_rejected_before_reading_report(self):
        data, digest = self.archive()
        with self.assertRaises(ValueError):
            retain.checked_report(data + b'changed', digest)

    def test_extra_or_escaping_members_are_rejected(self):
        for names in [('other.json',), ('../result.json',), ('result.json', 'extra.json')]:
            data, digest = self.archive(names)
            with self.assertRaises(ValueError):
                retain.checked_report(data, digest)

    def test_workflow_archive_requires_every_report_exactly_once(self):
        names = ['result.json', 'guide-body-context.json', 'recovery-delivery-context.json',
                 'agent-guide-context.json']
        data, digest = self.archive(tuple(reversed(names)))
        self.assertEqual(set(retain.checked_reports(data, digest, names)), set(names))
        for changed in (names[:-1], names + ['extra.json'], names + [names[0]]):
            data, digest = self.archive(changed)
            with self.assertRaises(ValueError):
                retain.checked_reports(data, digest, names)

    def test_workflow_producer_must_match_the_requested_group(self):
        run, artifact, report, jobs = self.identity()
        artifact['name'] = jobs[0]['name'] = 'workflow-routes'
        identity = retain.collection(run, artifact, report, 'e6qu/fun-refactor', jobs, 'workflow-routes')
        self.assertEqual(identity['producer_job_id'], 9)
        jobs[0]['name'] = 'source-repair'
        with self.assertRaises(ValueError):
            retain.collection(run, artifact, report, 'e6qu/fun-refactor', jobs, 'workflow-routes')

    def test_cohort_artifact_must_match_its_named_phase_producer(self):
        run, artifact, report, jobs = self.identity()
        artifact['name'] = 'repository-flow-coverage'
        jobs[0]['name'] = 'coverage'
        identity = retain.collection(run, artifact, report, 'e6qu/fun-refactor', jobs,
                                     'repository-flow-coverage', 'coverage')
        self.assertEqual(identity['producer_job_id'], 9)
        jobs[0]['name'] = 'held-out'
        with self.assertRaises(ValueError):
            retain.collection(run, artifact, report, 'e6qu/fun-refactor', jobs,
                              'repository-flow-coverage', 'coverage')

    def test_failed_or_unfinished_collection_is_not_promoted(self):
        for field, value in [('conclusion', 'failure'), ('status', 'in_progress'), ('path', '.github/workflows/ci.yml')]:
            run, artifact, report, jobs = self.identity()
            run[field] = value
            with self.assertRaises(ValueError):
                retain.collection(run, artifact, report, 'e6qu/fun-refactor', jobs)

    def test_cross_repository_collection_is_rejected(self):
        for field in ('repository', 'head_repository'):
            run, artifact, report, jobs = self.identity()
            run[field]['full_name'] = 'other/fork'
            with self.assertRaises(ValueError):
                retain.collection(run, artifact, report, 'e6qu/fun-refactor', jobs)

    def test_swapped_or_expired_artifact_is_rejected(self):
        run, original, report, jobs = self.identity()
        for field, value in [('id', 9), ('head_sha', 'c' * 40), ('expired', True), ('name', 'other')]:
            artifact = copy.deepcopy(original)
            target = artifact['workflow_run'] if field in ('id', 'head_sha') else artifact
            target[field] = value
            with self.assertRaises(ValueError):
                retain.collection(run, artifact, report, 'e6qu/fun-refactor', jobs)

    def test_report_must_come_from_the_recorded_collection_commit(self):
        run, artifact, report, jobs = self.identity()
        report['source_commit'] = 'c' * 40
        with self.assertRaises(ValueError):
            retain.collection(run, artifact, report, 'e6qu/fun-refactor', jobs)

    def test_rerun_cannot_relabel_a_carried_forward_artifact(self):
        run, artifact, report, jobs = self.identity()
        jobs[0]['run_attempt'] = 1
        later = {**jobs[0], 'id': 10, 'run_attempt': 2,
                 'started_at': '2026-10-11T08:03:00Z', 'completed_at': '2026-10-11T08:04:00Z'}
        jobs.append(later)
        identity = retain.collection(run, artifact, report, 'e6qu/fun-refactor', jobs)
        self.assertEqual((identity['attempt'], identity['producer_job_id']), (1, 9))

    def test_missing_or_failed_producer_is_rejected(self):
        for mutation in ('missing', 'failed', 'time'):
            run, artifact, report, jobs = self.identity()
            if mutation == 'missing':
                jobs.clear()
            elif mutation == 'failed':
                jobs[0]['conclusion'] = 'failure'
            else:
                artifact['created_at'] = '2026-10-11T08:03:00Z'
            with self.assertRaises(ValueError):
                retain.collection(run, artifact, report, 'e6qu/fun-refactor', jobs)


if __name__ == '__main__':
    unittest.main()
