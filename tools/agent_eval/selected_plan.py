"""Freeze M1's exact 36 attempts without launching clients or spending money."""
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
DIRECTORY = Path('tests/agent-eval/selected-agents')
INPUTS = ('clients.json', 'profiles.json', 'tasks.json')


def encoded(value):
    return json.dumps(value, sort_keys=True, ensure_ascii=False, separators=(',', ':'), allow_nan=False).encode()


def require(condition, message):
    if not condition:
        raise ValueError(message)


def prepare(root):
    documents = {name: json.loads((root / DIRECTORY / name).read_text()) for name in INPUTS}
    profiles, tasks = documents['profiles.json'], documents['tasks.json']
    require(set(profiles) == {'codex-luna-fast', 'opencode2-glm-flash', 'opencode2-kimi-k3'},
            'M1 requires the three selected profiles')
    require(set(tasks['tasks']) == {'boltons-rename', 'unicode-dice', 'access-proof'},
            'M1 requires its three fixed tasks')
    require(tasks['repetitions'] == 2 and tasks['setups'] == ['files', 'fr'],
            'M1 requires two repetitions and both tool setups')
    require(tasks['limits']['parallel_attempts'] == 1, 'attempts must run serially')
    for name, profile in profiles.items():
        require(profile['client'] in documents['clients.json'], f'missing client pin: {name}')
        if profile['client'] == 'codex':
            require(profile == {'client': 'codex', 'provider': 'openai', 'model': 'gpt-6-luna',
                               'reasoning_effort': 'high', 'service_tier': 'fast', 'request_service_tier': 'priority'},
                    'Codex model, effort and speed must match the selected profile')
        else:
            provider, model = ({'opencode2-glm-flash': ('zai-coding-plan', 'glm-5.3-flash'),
                                'opencode2-kimi-k3': ('kimi-code-plan-global', 'k3')})[name]
            require(profile == {'client': 'opencode2', 'model': {'providerID': provider, 'id': model, 'variant': 'high'}},
                    'OpenCode2 provider, model and reasoning must match the selected profile')
    attempts = []
    for repetition in range(tasks['repetitions']):
        for profile_index, profile in enumerate(profiles):
            for task_index, task in enumerate(tasks['tasks']):
                order = tasks['setups'] if (repetition + profile_index + task_index) % 2 == 0 else list(reversed(tasks['setups']))
                for position, setup in enumerate(order):
                    attempts.append({'id': f'{profile}-{task}-{setup}-{repetition + 1}',
                        'profile': profile, 'task': task, 'setup': setup, 'repetition': repetition + 1,
                        'position': position + 1, 'status': 'unstarted', 'outcome': None,
                        'usage': None, 'cost': None, 'fr_contribution': None})
    bindings = {str(DIRECTORY / name): hashlib.sha256((root / DIRECTORY / name).read_bytes()).hexdigest()
                for name in INPUTS}
    result = {'milestone': 'M1', 'live_calls_enabled': False, 'profiles': profiles,
              'clients': documents['clients.json'], 'requirements': tasks,
              'bindings': bindings, 'attempts': attempts,
              'billing': {'actual': None, 'estimated': None, 'authorization': None}}
    require(len(attempts) == 36 and len({row['id'] for row in attempts}) == 36,
            'M1 must contain exactly 36 distinct attempts')
    return {**result, 'sha256': hashlib.sha256(encoded(result)).hexdigest()}


def verify(value, root):
    require(value == prepare(root), 'M1 preparation differs from current inputs; review a new plan')
    return value
