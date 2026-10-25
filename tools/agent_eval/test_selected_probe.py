"""A required helper must not suppress the native Codex configuration probe."""
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agent_eval.selected_plan import ROOT, DIRECTORY

spec = importlib.util.spec_from_file_location('selected_package_probe', ROOT / 'tools/selected-agent-probe.py')
probe = importlib.util.module_from_spec(spec)
spec.loader.exec_module(probe)


class SelectedProbeTests(unittest.TestCase):
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
