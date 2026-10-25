"""Catch model substitutions, missing tool results and undeclared extra requests."""
import copy
import json
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agent_eval.selected_plan import ROOT, DIRECTORY
from agent_eval.selected_provider import validate, warmup

PROFILES = json.loads((ROOT / DIRECTORY / 'profiles.json').read_text())
EXPECTED = ['normalize_total', 'return value + 2']


class SelectedProviderTests(unittest.TestCase):
    def codex(self):
        return {'type': 'response.create', 'model': 'gpt-6-luna', 'reasoning': {'effort': 'high'}, 'service_tier': 'priority',
                'input': [{'type': 'additional_tools', 'tools': [{'type': 'namespace', 'name': 'functions',
                    'tools': [{'type': 'custom', 'name': 'exec'}]}]}]}

    def test_native_profiles_keep_their_identity_effort_and_speed(self):
        for name, profile in PROFILES.items():
            with self.subTest(profile=name):
                if profile['client'] == 'codex':
                    path, request = '/v1/responses', self.codex()
                    initial = copy.deepcopy(request)
                    validate(profile, path, request, 0, EXPECTED, initial)
                    request['input'] = [{'type': 'custom_tool_call_output', 'call_id': 'call_m1', 'output': '\n'.join(EXPECTED)}]
                else:
                    path = '/v1/chat/completions'
                    request = {'model': profile['model']['id'], 'reasoning_effort': 'high', 'stream': True,
                               'tools': [{'type': 'function', 'function': {'name': 'bash'}}], 'messages': []}
                    initial = copy.deepcopy(request)
                    validate(profile, path, request, 0, EXPECTED, initial)
                    request['messages'] = [{'role': 'tool', 'tool_call_id': 'call_m1', 'content': '\n'.join(EXPECTED)}]
                validate(profile, path, request, 1, EXPECTED, initial)

    def test_codex_model_reasoning_and_fast_are_checked_on_the_wire(self):
        original = self.codex()
        for key, value in [('model', 'gpt-6-astra'), ('reasoning', {'effort': 'low'}), ('service_tier', 'default')]:
            request = copy.deepcopy(original)
            request[key] = value
            with self.subTest(key=key), self.assertRaises(ValueError):
                validate(PROFILES['codex-luna-fast'], '/v1/responses', request, 0, EXPECTED, self.codex())

    def test_missing_failed_or_duplicate_tool_results_do_not_complete_the_exchange(self):
        request = self.codex()
        for outputs in ([], [{'type': 'custom_tool_call_output', 'call_id': 'call_m1', 'output': 'normalize_total: command failed'}],
                        [{'type': 'custom_tool_call_output', 'call_id': 'call_m1', 'output': '\n'.join(EXPECTED)}] * 2):
            request['input'] = outputs
            with self.subTest(outputs=outputs), self.assertRaises(ValueError):
                validate(PROFILES['codex-luna-fast'], '/v1/responses', request, 1, EXPECTED, self.codex())

    def test_extra_requests_and_other_routes_refuse_without_substitution(self):
        for path, index in [('/v1/responses', 2), ('/v1/chat/completions', 0)]:
            with self.subTest(path=path, index=index), self.assertRaises(ValueError):
                validate(PROFILES['codex-luna-fast'], path, self.codex(), index, EXPECTED, self.codex())

    def test_delegation_and_tool_result_misrouting_refuse(self):
        request = self.codex()
        request['input'][0]['tools'][0]['tools'].append({'type': 'function', 'name': 'spawn_agent'})
        with self.assertRaisesRegex(ValueError, 'delegation'):
            validate(PROFILES['codex-luna-fast'], '/v1/responses', request, 0, EXPECTED, request)
        request = self.codex()
        request['input'] = [{'type': 'custom_tool_call_output', 'call_id': 'someone_else', 'output': '\n'.join(EXPECTED)}]
        with self.assertRaisesRegex(ValueError, 'different call'):
            validate(PROFILES['codex-luna-fast'], '/v1/responses', request, 1, EXPECTED, self.codex())

    def test_warmup_cannot_hide_a_prompt_or_change_the_profile(self):
        request = self.codex()
        request.update(generate=False, input=[])
        events = warmup(PROFILES['codex-luna-fast'], '/v1/responses', request)
        self.assertEqual(events[-1]['response']['output'], [])
        request['input'] = [{'type': 'message', 'content': 'This would be model work'}]
        with self.assertRaisesRegex(ValueError, 'must not generate'):
            warmup(PROFILES['codex-luna-fast'], '/v1/responses', request)
        request.update(input=[], model='gpt-6-astra')
        with self.assertRaisesRegex(ValueError, 'changed model'):
            warmup(PROFILES['codex-luna-fast'], '/v1/responses', request)


if __name__ == '__main__':
    unittest.main()
