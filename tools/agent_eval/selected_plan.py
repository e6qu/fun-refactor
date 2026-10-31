"""Freeze an explicitly selected milestone without launching clients or spending money."""
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
DIRECTORY = Path('tests/agent-eval/selected-agents')
INPUTS = ('clients.json', 'profiles.json', 'tasks.json')
CASE_DIRECTORIES = {'M1': DIRECTORY, 'M2': Path('tests/agent-eval/project-edits'),
                    'M3': Path('tests/agent-eval/proof-writing'),
                    'M3-feedback': Path('tests/agent-eval/proof-feedback'),
                    'M3-construction': Path('tests/agent-eval/proof-construction')}
PROFILE_PATHS = {'M1': DIRECTORY / 'profiles.json', 'M2': DIRECTORY / 'profiles.json',
                 'M3': CASE_DIRECTORIES['M3'] / 'profiles.json',
                 'M3-feedback': CASE_DIRECTORIES['M3-feedback'] / 'profiles.json',
                 'M3-construction': CASE_DIRECTORIES['M3'] / 'profiles.json'}
TASK_PATHS = {name: path / ('requirements.json' if name == 'M3-construction' else 'tasks.json')
              for name, path in CASE_DIRECTORIES.items()}


def encoded(value):
    return json.dumps(value, sort_keys=True, ensure_ascii=False, separators=(',', ':'), allow_nan=False).encode()


def require(condition, message):
    if not condition:
        raise ValueError(message)


def prepare(root, milestone):
    require(milestone in CASE_DIRECTORIES, 'choose the study explicitly')
    case_directory = CASE_DIRECTORIES[milestone]
    paths = {name: (case_directory if name == 'tasks.json' else DIRECTORY) / name for name in INPUTS}
    paths['profiles.json'] = PROFILE_PATHS[milestone]
    paths['tasks.json'] = TASK_PATHS[milestone]
    documents = {name: json.loads((root / path).read_text()) for name, path in paths.items()}
    profiles, tasks = documents['profiles.json'], documents['tasks.json']
    if milestone == 'M3-construction':
        require(tasks['profiles_file'] == '../proof-writing/profiles.json' and
                tasks['profiles'] == ['opencode2-glm-flash', 'opencode2-kimi-k3'], 'construction profiles changed')
        profiles = {name: profiles[name] for name in tasks['profiles']}
    expected_profiles = ({'opencode2-glm-flash', 'opencode2-kimi-k3'} if milestone in {'M3-feedback', 'M3-construction'} else
                         {'codex-luna-fast', 'opencode2-glm-flash', 'opencode2-kimi-k3'})
    require(set(profiles) == expected_profiles, 'the experiment requires its exact selected profiles')
    expected_tasks = {'M1': {'boltons-rename', 'unicode-dice', 'access-proof'},
                      'M2': {'rust-parameter', 'typescript-alias', 'regex-escape-len', 'python-invoice'},
                      'M3': {'access-rust', 'page-count', 'fits-window'},
                      'M3-feedback': {'division-repair'}, 'M3-construction': {'scaled-total'}}[milestone]
    require(set(tasks['tasks']) == expected_tasks, milestone + ' requires its fixed tasks')
    expected_setups = (['self-check', 'supplied-feedback'] if milestone == 'M3-feedback' else
                       ['fr'] if milestone == 'M3-construction' else ['files', 'fr'])
    require(tasks['repetitions'] == 2 and tasks['setups'] == expected_setups,
            'the study requires its declared repetitions and conditions')
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
            variant = {'M1': 'high', 'M2': 'high', 'M3': 'low', 'M3-feedback': 'low', 'M3-construction': 'low'}[milestone]
            require(profile == {'client': 'opencode2', 'model': {'providerID': provider, 'id': model, 'variant': variant}},
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
    bound_paths = list(paths.values())
    if milestone in {'M2', 'M3', 'M3-feedback', 'M3-construction'}:
        bound_paths += [case_directory / 'checkers.json']
        if milestone == 'M3':
            bound_paths += [case_directory / 'cases.json']
        if milestone in {'M3-feedback', 'M3-construction'}:
            bound_paths += [path.relative_to(root) for path in (root / case_directory / 'controls').glob('*.lean')]
        bound_paths += [path.relative_to(root) for path in sorted((root / case_directory / 'fixtures').rglob('*'))
                        if path.is_file()]
    bindings = {str(path): hashlib.sha256((root / path).read_bytes()).hexdigest() for path in bound_paths}
    result = {'milestone': milestone, 'live_calls_enabled': False, 'profiles': profiles,
              'clients': documents['clients.json'], 'requirements': tasks,
              'bindings': bindings, 'attempts': attempts,
              'billing': {'actual': None, 'estimated': None, 'authorization': None}}
    expected_count = len(expected_tasks) * len(expected_profiles) * 2 * len(expected_setups)
    if milestone == 'M3-construction':
        require(tasks['attempts'] == expected_count == 4 and tasks['client_finish_seconds'] == 10,
                'construction needs exactly four attempts and the original finishing reserve')
    require(len(attempts) == expected_count and len({row['id'] for row in attempts}) == expected_count,
            milestone + ' needs all ' + str(expected_count) + ' distinct attempts')
    return {**result, 'sha256': hashlib.sha256(encoded(result)).hexdigest()}


def verify(value, root, milestone):
    require(value == prepare(root, milestone), milestone + ' preparation differs from current inputs; review a new plan')
    return value
