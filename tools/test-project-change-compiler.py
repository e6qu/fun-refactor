"""Keep compiler inputs independent of changing host directories and tools."""
import os
from pathlib import Path
import unittest
from unittest.mock import patch

import project_change_checkers as compiler


class PinnedCompiler(unittest.TestCase):
    def test_different_hosts_use_identical_inner_paths_flags_and_image(self):
        commands = [compiler.compiler_command(*(Path('/' + host) / name
            for name in ('source', 'rust', 'registry', 'target'))) for host in ('first', 'second')]
        for command in commands:
            self.assertIn('--network=none', command)
            self.assertIn('CARGO_NET_OFFLINE=true', command)
            self.assertIn('CARGO_TARGET_DIR=/build', command)
            self.assertIn('RUSTFLAGS=-D warnings', command)
            self.assertIn('CARGO_PROFILE_DEV_DEBUG=0', command)
            self.assertIn('CARGO_INCREMENTAL=0', command)
            self.assertIn('CARGO_BUILD_JOBS=2', command)
            self.assertEqual(command[-6:], [compiler.COMPILER_IMAGE,
                '/rust/bin/cargo', 'build', '--locked', '--bin', 'fr'])
            self.assertRegex(compiler.COMPILER_IMAGE, r'^gcc:15\.2\.0@sha256:[0-9a-f]{64}$')
        def inner(command):
            return [part.split(',src=', 1)[-1].split(',dst=', 1)[-1]
                    if part.startswith('type=bind,') else part for part in command]
        self.assertEqual(inner(commands[0]), inner(commands[1]))

    def test_local_compilation_refuses_before_running_a_command(self):
        with patch.dict(os.environ, {'GITHUB_ACTIONS': 'false'}), \
             patch.object(compiler.subprocess, 'run') as run, \
             patch.object(compiler.subprocess, 'check_output') as output:
            with self.assertRaisesRegex(AssertionError, 'GitHub runners'):
                compiler.compile_binary(Path('/unused'))
            run.assert_not_called()
            output.assert_not_called()


if __name__ == '__main__':
    unittest.main()
