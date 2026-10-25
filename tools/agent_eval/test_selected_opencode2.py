"""Check v2 readiness, exact release admission and bounded replies."""
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import MagicMock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agent_eval.opencode2_client import OpenCode2ResponseError, OpenCode2Server, scripted_configuration


class OpenCode2Tests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.server = OpenCode2Server('/unused/client', self.root, {}, self.root / 'output',
            version='2.0.32', startup_seconds=1, call_seconds=1, reply_bytes=1024)
        self.server.port = 1234

    def test_scripted_profile_cannot_reach_a_paid_origin_or_choose_another_model(self):
        profile = {'client': 'opencode2', 'model': {'providerID': 'kimi-code-plan-global', 'id': 'k3', 'variant': 'high'}}
        for origin in ('https://api.kimi.ai', 'http://127.0.0.1', 'http://user@127.0.0.1:9000', 'http://127.0.0.1:9000/v1'):
            with self.subTest(origin=origin), self.assertRaises(ValueError):
                scripted_configuration(profile, origin)
        profile['model']['id'] = 'k3.1-pro'
        with self.assertRaisesRegex(ValueError, 'selected model'):
            scripted_configuration(profile, 'http://127.0.0.1:9000')

    @patch('agent_eval.opencode2_client.os.killpg')
    @patch('agent_eval.opencode2_client.os.getpgid', return_value=999)
    @patch('agent_eval.opencode2_client.subprocess.Popen')
    def test_startup_waits_for_readiness_without_accepting_the_503(self, launch, group, stop):
        launch.return_value.pid = 999
        launch.return_value.poll.return_value = None
        launch.return_value.wait.return_value = 0
        responses = [OpenCode2ResponseError('GET', '/api/info', 503, 200, b'not ready'),
                     {'version': '2.0.32', 'pid': 999}]
        with patch.object(self.server, 'request', side_effect=responses) as request:
            with self.server:
                self.assertEqual(self.server.info['version'], '2.0.32')
            self.assertEqual(request.call_count, 2)
            self.assertTrue(all(0 < call.args[4] <= 1 for call in request.call_args_list))
        stop.assert_called_once()
        self.assertEqual(self.server.shutdown, {'exit_code': 0, 'forced': False})

    @patch('agent_eval.opencode2_client.os.killpg')
    @patch('agent_eval.opencode2_client.subprocess.Popen')
    def test_unauthorized_server_refuses_without_retry(self, launch, stop):
        launch.return_value.pid = 999
        launch.return_value.poll.return_value = None
        launch.return_value.wait.return_value = 0
        with patch.object(self.server, 'request', side_effect=OpenCode2ResponseError('GET', '/api/info', 401, 200, b'')) as request:
            with self.assertRaises(OpenCode2ResponseError):
                self.server.__enter__()
            self.assertEqual(request.call_count, 1)
        stop.assert_called_once()

    @patch('agent_eval.opencode2_client.http.client.HTTPConnection')
    def test_large_replies_and_invalid_routes_refuse(self, connect):
        reply = connect.return_value.getresponse.return_value
        reply.status = 200
        reply.read.return_value = b'x' * 1025
        with self.assertRaisesRegex(ValueError, 'byte limit'):
            self.server.request('GET', '/api/info', None, 200, 1)
        with self.assertRaisesRegex(ValueError, 'explicit v2'):
            self.server.request('GET', '/global/health', None, 200, 1)
        with self.assertRaisesRegex(ValueError, 'deadline expired'):
            self.server.request('GET', '/api/info', None, 200, 0)
        self.assertEqual(connect.call_count, 1)

    @patch('agent_eval.opencode2_client.http.client.HTTPConnection')
    def test_v2_data_is_not_unwrapped_as_a_v1_response(self, connect):
        reply = connect.return_value.getresponse.return_value
        reply.status = 200
        payload = {'data': {'id': 'session_m1'}}
        reply.read.return_value = json.dumps(payload).encode()
        self.assertEqual(self.server.request('POST', '/api/session', {'title': 'M1'}, 200, 1), payload)


if __name__ == '__main__':
    unittest.main()
