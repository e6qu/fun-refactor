"""A required helper must not suppress the native Codex configuration probe."""
import importlib.util
import hashlib
import io
import json
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import unittest
import urllib.error
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agent_eval.selected_plan import ROOT, DIRECTORY

spec = importlib.util.spec_from_file_location('selected_package_probe', ROOT / 'tools/selected-agent-probe.py')
probe = importlib.util.module_from_spec(spec)
spec.loader.exec_module(probe)


class SelectedProbeTests(unittest.TestCase):
    @staticmethod
    def package():
        buffer = io.BytesIO()
        with tarfile.open(fileobj=buffer, mode='w:gz') as archive:
            member = tarfile.TarInfo('package/client')
            member.size = 3
            archive.addfile(member, io.BytesIO(b'CLI'))
        data = buffer.getvalue()
        return data, {'url': 'https://example.invalid/client.tar.gz', 'version': '1.0.0',
                      'digest': {'algorithm': 'sha256', 'encoding': 'hex',
                                 'value': hashlib.sha256(data).hexdigest()},
                      'member': 'package/client', 'executable': 'client',
                      'unpacked_bytes': 3, 'companions': {}}

    def test_transient_download_reuses_the_pinned_url_and_remaining_deadline(self):
        data, entry = self.package()
        clock = [0.0]
        failure = urllib.error.HTTPError(entry['url'], 500, 'temporary', None, None)
        with tempfile.TemporaryDirectory() as temporary, \
             patch.object(probe.time, 'monotonic', side_effect=lambda: clock[0]), \
             patch.object(probe.time, 'sleep', side_effect=lambda seconds: clock.__setitem__(0, clock[0] + seconds)), \
             patch.object(probe.urllib.request, 'urlopen', side_effect=[failure, io.BytesIO(data)]) as request:
            binary = probe.install(entry, Path(temporary) / 'client')
            self.assertEqual(binary.read_bytes(), b'CLI')
            self.assertEqual([call.args for call in request.call_args_list], [(entry['url'],), (entry['url'],)])
            self.assertEqual([call.kwargs['timeout'] for call in request.call_args_list], [30, 29])

    def test_authentication_missing_files_and_wrong_hashes_do_not_retry(self):
        for code in (401, 403, 404):
            with self.subTest(code=code), tempfile.TemporaryDirectory() as temporary:
                _, entry = self.package()
                failure = urllib.error.HTTPError(entry['url'], code, 'refused', None, None)
                with patch.object(probe.urllib.request, 'urlopen', side_effect=failure) as request, \
                     patch.object(probe.time, 'sleep') as sleep:
                    with self.assertRaises(urllib.error.HTTPError):
                        probe.install(entry, Path(temporary) / 'client')
                    request.assert_called_once()
                    sleep.assert_not_called()
        data, entry = self.package()
        entry['digest']['value'] = 'wrong'
        with tempfile.TemporaryDirectory() as temporary, \
             patch.object(probe.urllib.request, 'urlopen', return_value=io.BytesIO(data)) as request:
            with self.assertRaisesRegex(ValueError, 'identity differs'):
                probe.install(entry, Path(temporary) / 'client')
            request.assert_called_once()

    def test_download_retries_stop_at_three_requests_or_the_original_deadline(self):
        for exhausted in (False, True):
            with self.subTest(deadline_exhausted=exhausted), tempfile.TemporaryDirectory() as temporary:
                _, entry = self.package()
                clock = [0.0]
                def refuse(*args, **kwargs):
                    if exhausted:
                        clock[0] = 29.5
                    raise urllib.error.HTTPError(entry['url'], 500, 'temporary', None, None)
                with patch.object(probe.time, 'monotonic', side_effect=lambda: clock[0]), \
                     patch.object(probe.time, 'sleep', side_effect=lambda seconds: clock.__setitem__(0, clock[0] + seconds)) as sleep, \
                     patch.object(probe.urllib.request, 'urlopen', side_effect=refuse) as request:
                    with self.assertRaises(urllib.error.HTTPError):
                        probe.install(entry, Path(temporary) / 'client')
                    self.assertEqual(request.call_count, 1 if exhausted else 3)
                    self.assertEqual(sleep.call_count, 0 if exhausted else 2)
                    self.assertEqual([call.args for call in request.call_args_list], [(entry['url'],)] * request.call_count)
                    self.assertEqual([call.kwargs['timeout'] for call in request.call_args_list],
                                     [30] if exhausted else [30, 29, 27])

    def test_codex_companion_does_not_hide_effective_config_and_model_list(self):
        calls = []
        profile = json.loads((ROOT / DIRECTORY / 'profiles.json').read_text())['codex-luna-fast']

        class Server:
            initialized = {'server': 'prescribed test'}
            shutdown = {'exit_code': 0, 'forced': False}

            def __init__(self, *args, **kwargs):
                pass

            def __enter__(self):
                return self

            def __exit__(self, *args):
                pass

            def request(self, method, arguments):
                calls.append(method)
                return ({'config': {'model': profile['model'], 'model_provider': profile['provider'],
                                   'model_reasoning_effort': profile['reasoning_effort'],
                                   'service_tier': profile['service_tier']}}
                        if method == 'config/read' else {'data': [], 'nextCursor': None})

        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            client = json.loads((ROOT / DIRECTORY / 'clients.json').read_text())['codex']
            binary = root / 'codex'
            binary.write_bytes(b'not executed')
            (root / 'codex-code-mode-host').write_bytes(b'required companion')
            reply = subprocess.CompletedProcess([], 0, b'codex-cli 0.164.0\n', b'')
            with patch.object(probe, 'install', return_value=binary), \
                 patch.object(probe.subprocess, 'run', return_value=reply), \
                 patch.object(probe, 'CodexServer', Server):
                report = probe.inspect('codex', client, root)
            self.assertEqual(calls, ['config/read', 'model/list'])
            self.assertEqual(set(report['companions']), {'codex-code-mode-host'})
            self.assertEqual(report['shutdown'], {'exit_code': 0, 'forced': False})
            self.assertTrue((root / 'effective-config.json').is_file())
            self.assertTrue((root / 'models.json').is_file())


if __name__ == '__main__':
    unittest.main()
