"""Refuse clipped RPC transcripts, expired waits and unexpected approval requests."""
import json
from pathlib import Path
import sys
import tempfile
import time
import unittest
from unittest.mock import MagicMock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agent_eval.codex_client import CodexServer


class CodexTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.server = CodexServer('/unused/client', Path(temporary.name), {}, Path(temporary.name) / 'out',
                                  call_seconds=1, transcript_bytes=1024)
        self.server.process = MagicMock()
        self.server.selector = MagicMock()

    @patch('agent_eval.codex_client.os.read')
    def test_interleaved_notification_does_not_hide_the_response(self, read):
        values = [{'method': 'notice', 'params': {'message': 'ready'}}, {'id': 1, 'result': {'model': 'gpt-6-luna'}}]
        read.return_value = b''.join(json.dumps(value).encode() + b'\n' for value in values)
        self.assertEqual(self.server.request('config/read', {}), {'model': 'gpt-6-luna'})
        self.assertEqual(self.server.transcript, values)
        read.assert_called_once()

    @patch('agent_eval.codex_client.os.read', return_value=b'x' * 1025)
    def test_oversized_transcript_refuses(self, read):
        with self.assertRaisesRegex(ValueError, 'byte limit'):
            self.server.receive(time.monotonic() + 1)

    @patch('agent_eval.codex_client.os.read')
    def test_approval_request_cannot_be_mistaken_for_a_success(self, read):
        read.return_value = b'{"id":99,"method":"item/commandExecution/requestApproval","params":{}}\n'
        with self.assertRaisesRegex(ValueError, 'unexpected Codex approval'):
            self.server.request('config/read', {})

    @patch('agent_eval.codex_client.os.read')
    def test_expired_deadline_does_not_read_or_extend_the_wait(self, read):
        with self.assertRaisesRegex(ValueError, 'deadline expired'):
            self.server.receive(time.monotonic() - 1)
        read.assert_not_called()
        self.server.selector.select.assert_not_called()


if __name__ == '__main__':
    unittest.main()
