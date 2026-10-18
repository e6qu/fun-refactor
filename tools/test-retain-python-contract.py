#!/usr/bin/env python3
"""Reject swapped, corrupted and unsafe hosted evidence artifacts."""
import copy
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch
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


class CompactReports(unittest.TestCase):
    def capture(self):
        value = {'source_commit': 'a' * 40, 'binary_sha256': 'b' * 64,
                 'cases': [{'source': 'é\n雪', 'values': [True, 1, None, -0.0]}]}
        original = (json.dumps(value, ensure_ascii=False, indent=2) + '\n').encode('utf-8')
        output = io.BytesIO()
        with zipfile.ZipFile(output, 'w', compression=zipfile.ZIP_DEFLATED) as archive:
            archive.writestr('result.json', original)
        data = output.getvalue()
        return value, original, data, {'source_commit': 'a' * 40,
            'artifact_digest': 'sha256:' + hashlib.sha256(data).hexdigest()}

    def test_compact_copy_preserves_types_unicode_and_the_exact_original_capture(self):
        value, original, archive, identity = self.capture()
        with tempfile.TemporaryDirectory() as temporary:
            folder = Path(temporary) / 'capture'
            manifest = retain.store_report(folder, archive, identity)
            saved = (folder / 'result.json').read_bytes()
            self.assertLess(len(saved), len(original))
            self.assertEqual(saved, retain.compact_report(value))
            self.assertEqual((folder / 'artifact.zip').read_bytes(), archive)
            self.assertEqual(retain.checked_report(archive, identity['artifact_digest']), original)
            self.assertEqual(manifest['source_report_sha256'], hashlib.sha256(original).hexdigest())
            self.assertEqual(retain.retained_report(folder / 'result.json'), value)

    def test_an_existing_capture_is_never_overwritten(self):
        _, _, archive, identity = self.capture()
        with tempfile.TemporaryDirectory() as temporary:
            folder = Path(temporary) / 'capture'
            retain.store_report(folder, archive, identity)
            before = {path.name: path.read_bytes() for path in folder.iterdir()}
            with self.assertRaisesRegex(ValueError, 'preserve prior results'):
                retain.store_report(folder, archive, identity)
            self.assertEqual(before, {path.name: path.read_bytes() for path in folder.iterdir()})

    def test_a_report_edit_is_rejected_even_after_recomputing_its_file_digest(self):
        _, _, archive, identity = self.capture()
        with tempfile.TemporaryDirectory() as temporary:
            folder = Path(temporary) / 'capture'
            manifest = retain.store_report(folder, archive, identity)
            path = folder / 'result.json'
            value = json.loads(path.read_bytes())
            value['cases'][0]['values'][0] = 1
            changed = retain.compact_report(value)
            path.write_bytes(changed)
            manifest['files']['result.json'] = hashlib.sha256(changed).hexdigest()
            (folder / 'manifest.json').write_text(json.dumps(manifest))
            with self.assertRaisesRegex(ValueError, 'differs from original captured values'):
                retain.retained_report(path)

    def test_noncanonical_whitespace_is_not_another_current_storage_format(self):
        _, _, archive, identity = self.capture()
        with tempfile.TemporaryDirectory() as temporary:
            folder = Path(temporary) / 'capture'
            manifest = retain.store_report(folder, archive, identity)
            path = folder / 'result.json'
            changed = json.dumps(json.loads(path.read_bytes()), indent=2).encode()
            path.write_bytes(changed)
            manifest['files']['result.json'] = hashlib.sha256(changed).hexdigest()
            (folder / 'manifest.json').write_text(json.dumps(manifest))
            with self.assertRaisesRegex(ValueError, 'differs from original captured values'):
                retain.retained_report(path)

    def test_archive_and_original_report_fingerprints_are_mandatory(self):
        for changed in ('archive', 'original', 'identity', 'missing'):
            with self.subTest(change=changed), tempfile.TemporaryDirectory() as temporary:
                _, _, archive, identity = self.capture()
                folder = Path(temporary) / 'capture'
                manifest = retain.store_report(folder, archive, identity)
                if changed == 'archive':
                    (folder / 'artifact.zip').write_bytes(archive + b'changed')
                    manifest['files']['artifact.zip'] = hashlib.sha256(archive + b'changed').hexdigest()
                elif changed == 'original':
                    manifest['source_report_sha256'] = 'c' * 64
                elif changed == 'identity':
                    manifest['collection']['source_commit'] = 'c' * 40
                else:
                    del manifest['source_report_sha256']
                (folder / 'manifest.json').write_text(json.dumps(manifest))
                with self.assertRaises(ValueError):
                    retain.retained_report(folder / 'result.json')


class ArtifactDownloads(unittest.TestCase):
    def test_reset_discards_partial_bytes_and_reuses_only_the_remaining_time(self):
        artifact, digest = ArtifactIdentity().archive()
        requests = []

        def download(arguments, **kwargs):
            requests.append((arguments, kwargs['timeout']))
            self.assertEqual(kwargs['stdout'].tell(), 0)
            if len(requests) == 1:
                kwargs['stdout'].write(b'partial archive')
                raise subprocess.CalledProcessError(1, arguments, stderr='read: connection reset by peer')
            kwargs['stdout'].write(artifact)

        with patch.object(retain, 'monotonic', side_effect=[100, 100, 125]), \
                patch.object(retain.subprocess, 'run', side_effect=download):
            result = retain.download_artifact('owner/repo', 42)
        self.assertEqual(retain.checked_report(result, digest), b'{"contract_complete":false}')
        self.assertEqual([timeout for _, timeout in requests], [60, 35])
        self.assertEqual(requests[0][0], requests[1][0])
        self.assertEqual(requests[0][0][-1], 'repos/owner/repo/actions/artifacts/42/zip')

    def test_repeated_resets_stop_after_two_attempts(self):
        failure = subprocess.CalledProcessError(1, ['gh'], stderr='connection reset by peer')
        with patch.object(retain, 'monotonic', side_effect=[0, 0, 10]), \
                patch.object(retain.subprocess, 'run', side_effect=failure) as run:
            with self.assertRaises(subprocess.CalledProcessError):
                retain.download_artifact('owner/repo', 42)
        self.assertEqual(run.call_count, 2)

    def test_a_reset_cannot_start_another_attempt_after_the_deadline(self):
        failure = subprocess.CalledProcessError(1, ['gh'], stderr='connection reset by peer')
        with patch.object(retain, 'monotonic', side_effect=[0, 0, 61]), \
                patch.object(retain.subprocess, 'run', side_effect=failure) as run:
            with self.assertRaisesRegex(ValueError, '60-second deadline'):
                retain.download_artifact('owner/repo', 42)
        self.assertEqual(run.call_count, 1)

    def test_authentication_errors_and_process_timeouts_are_not_retried(self):
        for failure in (subprocess.CalledProcessError(1, ['gh'], stderr='HTTP 403: Forbidden'),
                        subprocess.TimeoutExpired(['gh'], 60)):
            with self.subTest(error=type(failure).__name__), \
                    patch.object(retain, 'monotonic', side_effect=[0, 0]), \
                    patch.object(retain.subprocess, 'run', side_effect=failure) as run:
                with self.assertRaises(type(failure)):
                    retain.download_artifact('owner/repo', 42)
                self.assertEqual(run.call_count, 1)

    def test_corrupt_downloads_still_fail_digest_validation_without_a_retry(self):
        artifact, digest = ArtifactIdentity().archive()

        def download(arguments, **kwargs):
            kwargs['stdout'].write(artifact + b'changed')

        with patch.object(retain, 'monotonic', side_effect=[0, 0]), \
                patch.object(retain.subprocess, 'run', side_effect=download) as run:
            result = retain.download_artifact('owner/repo', 42)
            with self.assertRaisesRegex(ValueError, 'digest changed'):
                retain.checked_report(result, digest)
        self.assertEqual(run.call_count, 1)

    def test_oversized_downloads_keep_the_existing_admission_limit(self):
        def download(arguments, **kwargs):
            kwargs['stdout'].write(b'x' * (16 * 1024 * 1024 + 1))

        with patch.object(retain, 'monotonic', side_effect=[0, 0]), \
                patch.object(retain.subprocess, 'run', side_effect=download) as run:
            with self.assertRaisesRegex(ValueError, 'download admission'):
                retain.download_artifact('owner/repo', 42)
        self.assertEqual(run.call_count, 1)


if __name__ == '__main__':
    unittest.main()
