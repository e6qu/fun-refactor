"""Initial-goal spot checks cannot stand in for complete native admission."""
import copy
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from agent_eval import algorithm_admission as admission


class AlgorithmAdmissionChecks(unittest.TestCase):
    def test_complete_script_checks_rejection_both_proofs_package_and_delivery(self):
        script = admission.script(Path('/pinned-lean'), '/pinned-tools:/usr/bin:/bin')
        self.assertIn('export PATH=/pinned-tools:/usr/bin:/bin', script)
        self.assertIn('Possibly looping simp theorem', script)
        self.assertIn('/pinned-lean/bin/lake -d specs build --wfail', script)
        self.assertIn('git diff --binary HEAD > change.patch', script)
        self.assertEqual(script.count(' spec proof-check '), 3)
        self.assertEqual(script.count(' spec prove '), 2)
        for name in admission.algorithms.OBLIGATIONS:
            self.assertIn(str(admission.ROOT / admission.algorithms.DIRECTORY / 'controls' /
                              (name.replace('_', '-') + '.lean')), script)

    def test_admission_rejects_spot_checks_stale_inputs_and_inadequate_margin(self):
        profiles = json.loads((admission.ROOT / 'tests/agent-eval/proof-writing/profiles.json').read_text())
        binaries = {'fr': {'sha256': 'f' * 64}, 'opencode2': {'sha256': 'a' * 64}, 'codex': {'sha256': 'b' * 64}}
        with tempfile.TemporaryDirectory() as temporary:
            guard = Path(temporary) / 'guard.py'
            guard.write_text('guard')
            rows = {name: {'passed': True, 'profile': name, 'live_model_calls': 0, 'upstream_forwarding': False,
                'scripted_requests': 2, 'scripted_errors': 0, 'complete_workflow': True, 'recursive_loop_rejected': True,
                'session_accounting': True, 'fr_sha256': 'f' * 64, 'client_sha256': binaries[profile['client']]['sha256'],
                'archive_sha256': 'c' * 64, 'guard_sha256': admission.common.digest(guard), 'qualified_run': '12',
                'implementation': {'tool': 'hash'}, 'client_context_reserve_bytes': admission.RESERVE_BYTES,
                'resources': {'limits': {'wall_seconds': 60, 'rss_bytes': 805306368, 'cpu_seconds': 25,
                    'disk_bytes': 67108864, 'transcript_bytes': 131072}, 'exit_code': 0, 'stop_reason': None,
                    'monitor_error': None, 'launch_error': None, 'elapsed_seconds': 10,
                    'sampled_aggregate_rss_bytes': 500000000, 'sampled_cpu_seconds': 3,
                    'sampled_disk_growth_bytes': 1000, 'transcript_bytes': 1000}} for name, profile in profiles.items()}
            with patch.object(admission, 'GUARD', guard), \
                    patch.object(admission.local, 'implementation_bindings', return_value={'tool': 'hash'}):
                result = admission.validate_receipts(rows, binaries, 'c' * 64, '12')
                self.assertEqual(set(result['profiles']), set(profiles))
                self.assertNotIn('implementation', result['profiles']['codex-luna-fast'])
                for field, value in [('complete_workflow', False), ('recursive_loop_rejected', False),
                                     ('session_accounting', False), ('archive_sha256', 'changed'),
                                     ('client_context_reserve_bytes', 0), ('implementation', {})]:
                    changed = copy.deepcopy(rows)
                    changed['opencode2-glm-flash'][field] = value
                    with self.subTest(field=field), self.assertRaises(ValueError):
                        admission.validate_receipts(changed, binaries, 'c' * 64, '12')
                for value in (805306368, float('nan'), -1, True):
                    changed = copy.deepcopy(rows)
                    changed['codex-luna-fast']['resources']['sampled_aggregate_rss_bytes'] = value
                    with self.subTest(rss=value), self.assertRaises(ValueError):
                        admission.validate_receipts(changed, binaries, 'c' * 64, '12')


if __name__ == '__main__':
    unittest.main()
