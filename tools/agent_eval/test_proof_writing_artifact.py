"""A named artifact may be reused only after its native inputs and tests match."""
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agent_eval import proof_writing_artifact as artifact


class ArtifactIdentity(unittest.TestCase):
    def run_verification(self, *, changed, conclusion):
        def command(argv):
            if argv[0] == 'gh':
                if '/jobs?' in argv[-1]:
                    return json.dumps([{'jobs': [{'name': 'Check early returns and numeric proof models', 'conclusion': conclusion}]}])
                return json.dumps({'head_sha': 'a' * 40, 'head_repository': {'full_name': 'example/project'}})
            if 'fetch' in argv:
                return ''
            if changed and argv[-1].startswith('a' * 40 + ':'):
                return '1' * 40
            return '0' * 40
        with tempfile.TemporaryDirectory() as directory:
            binary = Path(directory) / 'fr'
            binary.write_bytes(b'controlled test executable')
            with mock.patch.object(artifact, 'command', side_effect=command), \
                 mock.patch.dict('os.environ', {'GITHUB_ACTIONS': 'true', 'GITHUB_REPOSITORY': 'example/project'}):
                return artifact.verify('123', binary)

    def test_identical_native_inputs_and_passing_tests_allow_explicit_reuse(self):
        result = self.run_verification(changed=False, conclusion='success')
        self.assertTrue(result['passed'])
        self.assertEqual(result['run'], '123')
        self.assertEqual(result['live_model_calls'], 0)

    def test_changed_native_inputs_require_a_new_build(self):
        with self.assertRaisesRegex(ValueError, 'native fr inputs changed'):
            self.run_verification(changed=True, conclusion='success')

    def test_failed_tests_cannot_qualify_an_artifact(self):
        with self.assertRaisesRegex(ValueError, 'must pass'):
            self.run_verification(changed=False, conclusion='failure')


if __name__ == '__main__':
    unittest.main()
