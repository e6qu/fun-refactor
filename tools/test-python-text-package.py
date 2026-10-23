import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('text_package', Path(__file__).with_name('python-text-package.py'))
package = importlib.util.module_from_spec(spec)
spec.loader.exec_module(package)


class CollectionIdentity(unittest.TestCase):
    def example(self):
        repository = 'owner/project'
        run = {'id': package.FIRST_RUN, 'head_sha': package.FIRST_SOURCE, 'path': package.WORKFLOW,
            'event': 'workflow_dispatch', 'status': 'completed', 'conclusion': 'success', 'run_attempt': 1,
            'repository': {'full_name': repository}, 'head_repository': {'full_name': repository},
            'html_url': 'https://github.com/owner/project/actions/runs/' + str(package.FIRST_RUN)}
        artifact = {'id': 42, 'name': 'python-text-package', 'expired': False, 'size_in_bytes': 1024,
            'workflow_run': {'id': package.FIRST_RUN, 'head_sha': package.FIRST_SOURCE},
            'created_at': '2026-10-23T21:30:30Z', 'digest': 'sha256:' + 'a' * 64}
        jobs = [{'id': 7, 'name': 'question', 'run_id': package.FIRST_RUN, 'run_attempt': 1,
            'status': 'completed', 'conclusion': 'success', 'started_at': '2026-10-23T21:30:00Z',
            'completed_at': '2026-10-23T21:31:00Z'}]
        return run, artifact, jobs, repository

    def test_the_dedicated_workflow_qualifies_without_claiming_another_workflow(self):
        run, artifact, jobs, repository = self.example()
        receipt = package.checked_collection(run, artifact, jobs, repository)
        self.assertEqual(receipt['workflow'], package.WORKFLOW)
        self.assertEqual(receipt['source_commit'], package.FIRST_SOURCE)

    def test_another_workflow_source_job_attempt_or_expired_artifact_refuses(self):
        for change in ('workflow', 'source', 'job', 'attempt', 'expired', 'late'):
            run, artifact, jobs, repository = copy.deepcopy(self.example())
            if change == 'workflow': run['path'] = '.github/workflows/refinement-evidence.yml'
            elif change == 'source': artifact['workflow_run']['head_sha'] = 'b' * 40
            elif change == 'job': jobs[0]['name'] = 'retain'
            elif change == 'attempt': run['run_attempt'] = 2
            elif change == 'expired': artifact['expired'] = True
            else: artifact['created_at'] = '2026-10-23T21:31:01Z'
            with self.subTest(change=change), self.assertRaises(ValueError):
                package.checked_collection(run, artifact, jobs, repository)


class CheckerIdentity(unittest.TestCase):
    def test_checker_or_python_client_changes_cannot_inherit_the_first_qualification(self):
        for name in ('src/subject.rs', 'sdk/python/src/fr_ir/_version.py', 'queries/python.scm'):
            roots = ('src', 'queries')
            original = {'source_commit': package.FIRST_SOURCE, 'complete': True, 'source_bindings': {name: 'old'}}
            frozen = {'source_commit': package.FIRST_SOURCE, 'selection': package.SELECTION}
            data = {'result.json': json.dumps(original).encode(), 'freeze.json': json.dumps(frozen).encode()}
            with tempfile.TemporaryDirectory() as temporary, patch.object(package, 'native_source_roots', return_value=roots), \
                    patch.object(package, 'checker_bindings', return_value={name: 'new'}):
                with self.subTest(name=name), self.assertRaisesRegex(ValueError, 'checker differs'):
                    package.original_audit(data, Path(temporary))


if __name__ == '__main__':
    unittest.main()
