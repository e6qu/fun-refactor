"""Keep failed first attempts without accepting substituted package evidence."""
import copy
import importlib.util
import json
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location('retain_questions', Path(__file__).with_name('retain-package-questions.py'))
retain = importlib.util.module_from_spec(spec)
spec.loader.exec_module(retain)


class PackageCollectionIdentity(unittest.TestCase):
    def example(self, conclusion):
        run = {'id': 17, 'head_sha': 'a' * 40, 'run_attempt': 1, 'status': 'completed',
               'conclusion': conclusion, 'path': '.github/workflows/refinement-evidence.yml',
               'event': 'workflow_dispatch', 'repository': {'full_name': 'owner/repo'},
               'head_repository': {'full_name': 'owner/repo'}}
        artifact = {'name': 'package-questions', 'expired': False,
                    'workflow_run': {'id': 17, 'head_sha': 'a' * 40}, 'created_at': '2026-10-19T10:00:10Z'}
        jobs = [{'name': 'package-questions', 'run_id': 17, 'run_attempt': 1, 'status': 'completed',
                 'conclusion': conclusion, 'started_at': '2026-10-19T10:00:00Z',
                 'completed_at': '2026-10-19T10:00:20Z'}]
        value = {'source_commit': 'a' * 40, 'phase': 'held-out', 'milestones_complete': False,
                 'projects': [{'error': 'original first failure', 'complete': False}]}
        return run, artifact, jobs, {'result.json': json.dumps(value).encode()}

    def check(self, example):
        return retain.checked_collection(*example, 'owner/repo', 'held-out')

    def test_failed_first_attempt_preserves_its_actual_outcome(self):
        value, producer = self.check(self.example('failure'))
        self.assertEqual(value['projects'], [{'error': 'original first failure', 'complete': False}])
        self.assertEqual(producer['conclusion'], 'failure')

    def test_successful_attempt_keeps_the_original_report_values(self):
        example = self.example('success')
        self.assertEqual(self.check(example)[0], json.loads(example[3]['result.json']))

    def test_foreign_sources_replaced_phases_and_unfinished_jobs_refuse(self):
        for mutation in ('run-source', 'archive-source', 'repository', 'workflow', 'phase', 'producer', 'attempt', 'rerun', 'time', 'queued', 'expired', 'duplicate'):
            with self.subTest(mutation=mutation):
                run, artifact, jobs, data = self.example('failure')
                if mutation == 'run-source': run['head_sha'] = 'b' * 40
                elif mutation == 'archive-source': artifact['workflow_run']['head_sha'] = 'b' * 40
                elif mutation == 'repository': run['head_repository']['full_name'] = 'other/repo'
                elif mutation == 'workflow': run['path'] = 'other.yml'
                elif mutation == 'phase':
                    value = json.loads(data['result.json'])
                    value['phase'] = 'development'
                    data['result.json'] = json.dumps(value).encode()
                elif mutation == 'producer': jobs[0]['conclusion'] = 'success'
                elif mutation == 'attempt': jobs[0]['run_attempt'] = 2
                elif mutation == 'rerun': run['run_attempt'] = jobs[0]['run_attempt'] = 2
                elif mutation == 'time': artifact['created_at'] = '2026-10-19T11:00:00Z'
                elif mutation == 'queued': run['status'] = 'queued'
                elif mutation == 'expired': artifact['expired'] = True
                elif mutation == 'duplicate': jobs.append(copy.deepcopy(jobs[0]))
                with self.assertRaises(ValueError):
                    self.check((run, artifact, jobs, data))

    def test_failed_job_cannot_supply_completed_milestones(self):
        run, artifact, jobs, data = self.example('failure')
        value = json.loads(data['result.json'])
        value['milestones_complete'] = True
        data['result.json'] = json.dumps(value).encode()
        with self.assertRaisesRegex(ValueError, 'failed collection'):
            self.check((run, artifact, jobs, data))


if __name__ == '__main__':
    unittest.main()
