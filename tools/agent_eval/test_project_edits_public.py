"""Keep public check installation, failures and deadlines observable."""
import os
from pathlib import Path
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agent_eval import project_edits_public as public


class PublicBehaviorTests(unittest.TestCase):
    def test_installed_checker_is_a_standalone_copy_outside_task_source(self):
        with tempfile.TemporaryDirectory() as temporary:
            project = Path(temporary)
            installed = public.install(project)
            self.assertEqual(installed, project / '.fr/check-escape')
            self.assertEqual(installed.read_bytes(), Path(public.__file__).read_bytes())

    def test_expired_deadline_does_not_start_a_command(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            with patch.object(public.subprocess, 'Popen') as popen:
                with self.assertRaisesRegex(RuntimeError, 'expired before public-behavior'):
                    public.command(root, root, 'public-behavior', ['unused'], time.monotonic() - 1)
                popen.assert_not_called()

    def test_failed_command_keeps_diagnostics_and_fails_the_check(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            with self.assertRaisesRegex(RuntimeError, 'exit.*7.*\nallocation failure'):
                public.command(root, root, 'public-behavior',
                    [sys.executable, '-c', 'import sys; print("allocation failure", file=sys.stderr); sys.exit(7)'],
                    time.monotonic() + 5)
            self.assertEqual((root / 'public-behavior.stderr').read_text(), 'allocation failure\n')

    def test_check_requires_an_explicit_unoptimized_single_job_environment(self):
        with tempfile.TemporaryDirectory() as temporary, patch.dict(os.environ, {}, clear=True):
            root = Path(temporary)
            with self.assertRaisesRegex(ValueError, 'optimization 0 and one compiler job'):
                public.check(root, root / 'target', Path('/cargo'), Path('/rustc'), time.time() + 10)
            self.assertFalse((root / 'target').exists())


if __name__ == '__main__':
    unittest.main()
