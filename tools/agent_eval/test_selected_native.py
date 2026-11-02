"""Preserve every native outcome, and stop on resource or cleanup failures."""
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agent_eval import selected_native as native


class SelectedNativeTests(unittest.TestCase):
    def exercise(self, stop_reason, complete, profiles_path):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        binary = root / 'fr'
        binary.write_bytes(b'not executed')
        output = root / 'results'
        calls = []

        def exchange(image, profile, setup, directory):
            calls.append((profile, setup))
            if len(calls) == 1:
                directory.mkdir()
                (directory / 'result.json').write_text(json.dumps({
                    'resources': {'complete': complete, 'stop_reason': stop_reason},
                    'cleanup': {'process': {'exit_code': 0, 'stop_reason': None}}}))
                raise ValueError('prescribed native failure')
            return {'image': image}

        with patch.dict('os.environ', {'GITHUB_ACTIONS': 'true'}), \
             patch.object(native.packages, 'install', return_value=root / 'client'), \
             patch.object(native, 'build', return_value='sha256:' + 'a' * 64) as building, \
             patch.object(native, 'exchange', side_effect=exchange):
            with self.assertRaises(ValueError):
                native.run(binary, output, profiles_path)
        self.assertTrue(all(call.args[-1] == profiles_path for call in building.call_args_list))
        return calls, json.loads((output / 'summary.json').read_text())

    def test_a_protocol_failure_does_not_hide_the_other_five_outcomes(self):
        calls, summary = self.exercise(None, True, native.ROOT / native.DIRECTORY / 'profiles.json')
        self.assertEqual(len(calls), 6)
        self.assertFalse(summary['passed'])
        self.assertEqual([row['status'] for row in summary['exchanges'].values()].count('failed'), 1)
        self.assertEqual([row['status'] for row in summary['exchanges'].values()].count('passed'), 5)
        self.assertEqual(summary['live_model_calls'], 0)

    def test_resource_stops_and_incomplete_cleanup_leave_remaining_work_unstarted(self):
        for reason, complete in [('memory_bytes', True), (None, False)]:
            with self.subTest(reason=reason, complete=complete):
                calls, summary = self.exercise(reason, complete, native.ROOT / native.DIRECTORY / 'profiles.json')
                self.assertEqual(len(calls), 1)
                self.assertEqual([row['status'] for row in summary['exchanges'].values()].count('unstarted'), 5)
                self.assertFalse(summary['passed'])

    def test_m3_native_controls_record_the_explicit_proof_profile(self):
        path = native.ROOT / 'tests/agent-eval/proof-writing/profiles.json'
        calls, summary = self.exercise(None, True, path)
        self.assertEqual(len(calls), 6)
        self.assertEqual(summary['profiles'], json.loads(path.read_text()))
        self.assertEqual(summary['profiles']['opencode2-glm-flash']['model']['variant'], 'low')
        self.assertEqual(summary['profiles']['opencode2-kimi-k3']['model']['variant'], 'low')


if __name__ == '__main__':
    unittest.main()
