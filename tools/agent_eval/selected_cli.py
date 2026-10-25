"""Exact native CLI settings shared by scripted controls and local attempts."""
import json
from pathlib import Path
import subprocess
import uuid

from .selected_plan import require


def configuration(profile, setup, project):
    selection = profile['model']
    rules = [{'action': '*', 'resource': '*', 'effect': 'deny'}] + [
        {'action': action, 'resource': '*', 'effect': 'allow'}
        for action in ('read', 'glob', 'grep', 'edit', 'write', 'shell', 'skill')]
    model = {'providerID': selection['providerID'], 'model': selection['id'], 'variant': selection['variant']}
    return {'model': model, 'default_agent': 'build',
        'agents': {'build': {'model': model, 'steps': 32, 'permissions': rules}},
        'shell': '/bin/bash', 'update': 'disable', 'share': 'disabled', 'snapshots': False, 'plugins': [],
        'lsp': False, 'mcp': {}, 'compaction': {'auto': False}, 'permissions': rules,
        'skills': [str(project / '.agents/skills')] if setup == 'fr' else []}


def invocation(profile, binaries, project, output, prompt_text, sandbox):
    require(sandbox in {'workspace-write', 'danger-full-access'}, 'choose the explicit CLI sandbox')
    require(sandbox != 'danger-full-access' or Path('/.dockerenv').is_file(),
            'external sandbox mode requires the controlled container')
    if profile['client'] == 'codex':
        arguments = [binaries['codex']['path'], 'exec', '--strict-config', '--ignore-user-config', '--ignore-rules',
            '--ephemeral', '--sandbox', sandbox, '--json', '--color', 'never', '--model', profile['model'],
            '--cd', str(project), '--output-last-message', str(output / 'final.txt')]
        for key, value in {'model_provider': profile['provider'], 'model_reasoning_effort': profile['reasoning_effort'],
            'service_tier': profile['service_tier'], 'approval_policy': 'never', 'web_search': 'disabled', 'features.plugins': False,
            'features.multi_agent': False, 'features.apps': False, 'features.memories': False,
            'features.goals': False, 'features.hooks': False}.items():
            arguments += ['--config', key + '=' + json.dumps(value)]
        return arguments + ['-'], prompt_text.encode()
    selection = profile['model']
    return [binaries['opencode2']['path'], 'run', '--standalone', '--session', 'ses_' + uuid.uuid4().hex,
        '--model', selection['providerID'] + '/' + selection['id'] + '#' + selection['variant'],
        '--agent', 'build', '--format', 'json', '--title', 'M1 measured attempt', prompt_text], b''


def export_session(binary, session, project, env, output):
    exported = subprocess.run([str(binary), 'api', '--standalone', 'GET',
        '/api/experimental/session/' + session + '/export'], env=env, cwd=project, capture_output=True, timeout=10)
    (output / 'export.stderr').write_bytes(exported.stderr)
    (output / 'export.stdout').write_bytes(exported.stdout)
    require(exported.returncode == 0 and len(exported.stdout) <= 1024**2, 'native session export failed or exceeded its limit')
    return json.loads(exported.stdout)['data']


def verify_export(saved, profile, session, events):
    require(saved['info']['id'] == session and saved['info']['model'] == profile['model'] and
            saved['info']['agent'] == 'build', 'native exported model or agent differs')
    assistants = [message for message in saved['messages'] if message['type'] == 'assistant']
    require(all(message['model'] == profile['model'] and message['agent'] == 'build' for message in assistants),
            'native response substituted a model, reasoning variant or agent')
    steps = [event['part'] for event in events if event['type'] == 'step_finish']
    by_id = {message['id']: message for message in assistants}
    require(len(by_id) == len(assistants), 'duplicate native assistant response')
    require(all(step['messageID'] in by_id and step['tokens'] == by_id[step['messageID']]['tokens'] and
                step['cost'] == by_id[step['messageID']]['cost'] for step in steps),
            'native stream and exported usage differ')
    return {'model_observed': bool(assistants), 'session_model': saved['info']['model'],
            'exported_assistant_responses': len(assistants), 'stream_finished_responses': len(steps),
            'outcome': saved['info']['outcome']}
