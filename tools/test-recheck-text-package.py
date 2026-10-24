import copy
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location('recheck_text', Path(__file__).with_name('recheck-text-package.py'))
recheck = importlib.util.module_from_spec(spec)
spec.loader.exec_module(recheck)


class MaintenanceClaims(unittest.TestCase):
    def example(self):
        return {'schema': 'fr-rechecked-text-package', 'complete': True, 'all_projects_familiar': True,
            'current_checker_was_unread': False, 'original_auditor': recheck.original.HISTORICAL_AUDITOR,
            'original_source_commit': recheck.original.FIRST_SOURCE, 'original_audit_passed': True,
            'original_unread': {'project': recheck.SELECTION['repository'], 'qualified': True,
                'first_evaluation_run': recheck.original.FIRST_RUN},
            'independent_projects': sorted([recheck.SELECTION['repository'], 'dateutil/dateutil', 'mahmoud/boltons', 'phn/jdcal']),
            'milestone_1_demonstrated': True, 'milestone_2_demonstrated': True, 'milestones_complete': True,
            'benchmark': False, 'live_model_calls': 0}

    def test_current_maintenance_does_not_claim_another_unread_trial(self):
        recheck.validate_claims(self.example())
        for field, value in [('all_projects_familiar', False), ('current_checker_was_unread', True),
                             ('original_audit_passed', False), ('original_source_commit', '0' * 40),
                             ('independent_projects', []), ('milestones_complete', False), ('benchmark', True)]:
            data = copy.deepcopy(self.example())
            data[field] = value
            with self.subTest(field=field), self.assertRaises(ValueError):
                recheck.validate_claims(data)


if __name__ == '__main__':
    unittest.main()
