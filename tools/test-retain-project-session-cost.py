"""Reject failed collection steps and unsafe measurement archives."""
import importlib.util
import io
from pathlib import Path
import tempfile
import unittest
import zipfile

spec = importlib.util.spec_from_file_location('retainer', Path(__file__).with_name('retain-project-session-cost.py'))
retainer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(retainer)


class CollectionProvenance(unittest.TestCase):
    def fixture(self):
        run = {'status': 'completed', 'run_attempt': 1,
               'path': '.github/workflows/project-session-cost.yml', 'conclusion': 'failure'}
        names = ['Run cargo build --locked --release --bin fr', 'Inspect the Python client with fr',
                 'Measure five approaches with matching complete answers']
        jobs = [{'name': 'Reject missing session costs and stale answers', 'conclusion': 'success'},
                {'name': 'measure', 'conclusion': 'failure', 'steps': [
                    *({'name': name, 'conclusion': 'success'} for name in names),
                    {'name': 'Retain the audited collection and update current tracking', 'conclusion': 'failure'}]}]
        return run, jobs

    def test_completed_measurement_survives_later_publication_failure(self):
        retainer.authenticate(*self.fixture())

    def test_reject_failed_measurement_not_just_failed_run_label(self):
        run, jobs = self.fixture()
        jobs[1]['steps'][2]['conclusion'] = 'failure'
        with self.assertRaisesRegex(ValueError, 'independent audit'):
            retainer.authenticate(run, jobs)

    def test_reject_failed_controls_and_different_producer(self):
        for change in ('controls', 'workflow', 'attempt', 'incomplete'):
            run, jobs = self.fixture()
            if change == 'controls':
                jobs[0]['conclusion'] = 'failure'
            elif change == 'workflow':
                run['path'] = '.github/workflows/other.yml'
            elif change == 'attempt':
                run['run_attempt'] = 2
            else:
                run['status'] = 'in_progress'
            with self.subTest(change=change), self.assertRaises(ValueError):
                retainer.authenticate(run, jobs)

    def test_reject_unsafe_or_unrelated_artifact(self):
        for name in ('../escape', '/absolute', 'other/result.json'):
            stream = io.BytesIO()
            with zipfile.ZipFile(stream, 'w') as archive:
                archive.writestr(name, '{}')
            with tempfile.TemporaryDirectory() as temporary, self.subTest(name=name):
                with self.assertRaisesRegex(ValueError, 'unsafe collection'):
                    retainer.extract(stream.getvalue(), Path(temporary))


if __name__ == '__main__':
    unittest.main()
