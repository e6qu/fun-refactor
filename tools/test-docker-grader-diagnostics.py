#!/usr/bin/env python3
"""Host evidence survives unavailable Docker without changing operation limits."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('diagnostics', Path(__file__).with_name('docker-grader-diagnostics.py'))
diagnostics = importlib.util.module_from_spec(spec)
spec.loader.exec_module(diagnostics)


class DockerDiagnostics(unittest.TestCase):
    def test_stalled_daemon_does_not_discard_host_evidence_or_raise_limits(self):
        calls = []
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / 'docker-host.json'
            def execute(command, data, workspace, timeout, cap):
                calls.append(command)
                self.assertEqual((timeout, cap), (5, 65536))
                if command[0] == 'docker':
                    return {'exit_code': 124, 'stop_reason': 'wall_seconds'}, b'', b'daemon stalled'
                self.assertEqual(len(json.loads(output.read_text())['commands']), len(calls) - 1)
                return {'exit_code': 0, 'stop_reason': None}, b'host state', b''
            report = diagnostics.collect(output, execute=execute)
            retained = json.loads(output.read_text())
            self.assertEqual(len(retained['commands']), len(diagnostics.COMMANDS))
            self.assertEqual(retained['commands'][0]['process']['stop_reason'], 'wall_seconds')
            self.assertEqual(retained['commands'][2]['stdout'], 'host state')
            self.assertEqual(retained['disk_bytes'], report['disk_bytes'])
            self.assertEqual(retained['load_average'], list(report['load_average']))


if __name__ == '__main__':
    unittest.main()
