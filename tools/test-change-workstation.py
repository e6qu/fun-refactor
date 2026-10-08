#!/usr/bin/env python3
"""Reject missing or stale hosted admission before any local control is available."""
from pathlib import Path
import runpy
import tempfile
import unittest
from unittest.mock import patch
import zipfile

from agent_eval import source_reviews, terminal_changes as changes
from agent_eval.study import encode

CHECKER = runpy.run_path(str(Path(__file__).with_name('check-change-workstation.py')))


class Admission(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.binary = self.root / 'binary'
        self.binary.write_bytes(b'test executable identity; never executed')
        self.plan = {'runtime': changes.implementation(), 'client_environment': changes.CLIENT_ENVIRONMENTS[-1],
                     'provenance': {'script_sha256': source_reviews.identity(Path(__file__).with_name('check-terminal-changes.py'))},
                     'binary_sha256': source_reviews.identity(self.binary), 'opencode_sha256': source_reviews.identity(self.binary)}
        self.result = {'headroom_admitted': True}
        self.checker = {'CASES': range(5), 'report': lambda _: self.result}

    def archives(self):
        rows = []
        for platform in ('macos-15', 'ubuntu-latest'):
            archive = self.root / (platform + '.zip')
            with zipfile.ZipFile(archive, 'w') as output:
                output.writestr('result.json', encode([self.result] * 5))
                for index in range(5):
                    output.writestr(str(index) + '/plan.json', encode({'plan': self.plan}))
            rows.append({'platform': platform, 'archive': archive.name, 'sha256': source_reviews.identity(archive)})
        (self.root / 'provenance.json').write_bytes(encode({'artifacts': rows}))

    def admit(self):
        with patch.object(CHECKER['runpy'], 'run_path', return_value=self.checker):
            return CHECKER['hosted'](self.root, self.binary, self.binary)

    def test_matching_control_matrix_returns_checker_without_launching_client(self):
        self.archives()
        self.assertIs(self.admit(), self.checker)

    def test_stale_runtime_and_different_executable_are_refused(self):
        for key in ('runtime', 'binary_sha256', 'opencode_sha256'):
            original = self.plan[key]
            self.plan[key] = {} if key == 'runtime' else '0' * 64
            self.archives()
            with self.subTest(key=key), self.assertRaisesRegex(ValueError, 'differs'):
                self.admit()
            self.plan[key] = original

    def test_missing_platform_and_failed_headroom_are_refused(self):
        (self.root / 'provenance.json').write_bytes(encode({'artifacts': []}))
        with self.assertRaisesRegex(ValueError, 'both hosted'):
            self.admit()
        self.result['headroom_admitted'] = False
        self.archives()
        with self.assertRaisesRegex(ValueError, 'admission failed'):
            self.admit()


if __name__ == '__main__':
    unittest.main()
