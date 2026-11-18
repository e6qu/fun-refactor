"""Freeze the two product comparisons without opening validation source or calling models."""
import hashlib
import json
from pathlib import Path
import re
import subprocess

from .selected_plan import DIRECTORY as CLIENTS, encoded, require

DIRECTORY = Path('tests/agent-eval/product-validation')
STUDIES = {'P5-initial': 'initial', 'P5-validation': 'validation'}
TASKS = {
    'initial': ('utils-rename', 'sqlparse-graph', 'slugify-bytes', 'clamp-proof'),
    'validation': ('text-rename', 'functools-graph', 'pathspec-change', 'remaining-proof'),
}
FAMILIES = ('refactoring', 'graph-analysis', 'behavior-change', 'proof')
LIMITS = {'wall_seconds': 150, 'rss_bytes': 768 * 1024**2,
          'disk_bytes': 512 * 1024**2, 'cpu_limit_seconds': 75,
          'transcript_bytes': 1024**2, 'parallel_attempts': 1}


def read(root, path):
    return json.loads((root / path).read_text())


def file_digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def production_trees(root):
    """Bind product code and instructions without reading reserved project archives."""
    paths = [line.rstrip('/') for line in (root / 'native-source-inputs.txt').read_text().splitlines()
             if line and not line.startswith('!')]
    paths += ['sdk/python/src', 'skills/fr']
    result = {}
    for path in sorted(set(paths)):
        result[path] = subprocess.check_output(
            ['git', '-C', str(root), 'rev-parse', 'HEAD:' + path], text=True, timeout=10).strip()
    return result


def validation_release(root):
    path = root / DIRECTORY / 'validation-release.json'
    require(path.is_file(), 'validation source stays reserved until the initial decision and implementation are frozen')
    value = json.loads(path.read_text())
    require(set(value) == {'schema', 'implementation_commit', 'initial_results_commit',
                          'initial_summary_sha256', 'decision', 'production_trees'},
            'validation release must identify the checked initial results and frozen product')
    require(value['schema'] == 'fr-product-validation-release-1' and
            value['decision'] in {'keep', 'revise', 'discard'}, 'unknown validation release or decision')
    for key in ('implementation_commit', 'initial_results_commit'):
        require(isinstance(value[key], str) and re.fullmatch(r'[0-9a-f]{40}', value[key]),
                'validation release needs exact committed inputs')
    require(isinstance(value['initial_summary_sha256'], str) and
            re.fullmatch(r'[0-9a-f]{64}', value['initial_summary_sha256']), 'bind the original initial-round summary')
    require(value['production_trees'] == production_trees(root),
            'product code or instructions changed after the validation freeze')
    summary_path = DIRECTORY / 'initial/results/summary.json'
    original = subprocess.check_output(['git', '-C', str(root), 'show',
        value['initial_results_commit'] + ':' + str(summary_path)], timeout=10)
    require(hashlib.sha256(original).hexdigest() == value['initial_summary_sha256'],
            'initial-round result differs from its committed release')
    summary = json.loads(original)
    require(summary.get('round') == 'initial' and len(summary.get('attempts', [])) == 48 and
            {row['id'] for row in summary['attempts']} ==
            {row['id'] for row in campaign(root)['attempts'] if row['round'] == 'initial'} and
            all(row.get('status') in {'completed', 'failed'} and
                row.get('independent_outcome') in {'passed', 'failed'} for row in summary['attempts']),
            'finish and independently grade every initial slot before releasing validation source')
    frozen = subprocess.check_output(['git', '-C', str(root), 'rev-parse',
        value['implementation_commit'] + '^{commit}'], text=True, timeout=10).strip()
    require(frozen == value['implementation_commit'], 'missing frozen implementation commit')
    for name, tree in value['production_trees'].items():
        recorded = subprocess.check_output(['git', '-C', str(root), 'rev-parse',
            frozen + ':' + name], text=True, timeout=10).strip()
        require(recorded == tree, 'validation product differs from the named frozen implementation')
    return value


def configuration(root):
    from .selected_plan import prepare
    original = prepare(root, 'M1')  # Reuse the exact selected client/profile checks.
    protocol = read(root, DIRECTORY / 'protocol.json')
    require(protocol['schema'] == 'fr-product-comparison-1' and protocol['study'] == 'P5',
            'choose the product comparison explicitly')
    require(protocol['maximum_live_attempts'] == protocol['authorization']['maximum_attempts'] == 96 and
            protocol['attempts_per_round'] == 48 and protocol['repetitions'] == 2 and
            protocol['rounds'] == ['initial', 'validation'] and
            protocol['families'] == list(FAMILIES) and protocol['setups'] == ['files', 'fr'] and
            protocol['profiles'] == list(original['profiles']), 'preserve the authorized 96-slot comparison')
    require(protocol['limits'] == LIMITS and all(type(value) is int for value in protocol['limits'].values()) and
            protocol['client_finish_seconds'] == 10 and
            protocol['retry_failed_slots'] is False, 'product comparison limits or retry policy changed')
    require(protocol['workstation'] == {'wall_seconds': 180, 'rss_bytes': 1024**3,
            'target_bytes': 2 * 1024**3, 'free_disk_bytes': 64 * 1024**3},
            'preserve the outer workstation limits')
    return protocol, original


def campaign(root):
    """Keep all 96 slots visible while validation task contents remain unopened."""
    protocol, original = configuration(root)
    rows = []
    for phase in protocol['rounds']:
        for repetition in (1, 2):
            for profile_index, profile in enumerate(protocol['profiles']):
                for task_index, task in enumerate(TASKS[phase]):
                    order = protocol['setups'][::(-1 if (repetition - 1 + profile_index + task_index) % 2 else 1)]
                    for position, setup in enumerate(order, 1):
                        rows.append({'id': f'P5-{phase}-{profile}-{task}-{setup}-{repetition}',
                            'round': phase, 'profile': profile, 'task': task, 'family': FAMILIES[task_index],
                            'setup': setup, 'repetition': repetition, 'position': position,
                            'status': 'unstarted', 'outcome': None, 'usage': None, 'cost': None,
                            'fr_contribution': None})
    require(len(rows) == len({row['id'] for row in rows}) == 96, 'retain every distinct campaign slot')
    return {'schema': 'fr-product-campaign-1', 'live_calls_enabled': False,
            'profiles': original['profiles'], 'clients': original['clients'], 'attempts': rows}


def prepare(root, milestone):
    require(milestone in STUDIES, 'choose an explicit product comparison round')
    phase = STUDIES[milestone]
    released = validation_release(root) if phase == 'validation' else None
    protocol, original = configuration(root)
    reservation = read(root, DIRECTORY / 'reservation.json')
    require(reservation['schema'] == 'fr-product-validation-reservation-1' and
            reservation['maximum_live_attempts'] == 96, 'retain the original project reservation')
    task_path = DIRECTORY / phase / 'tasks.json'
    tasks = read(root, task_path)
    require(tasks['schema'] == 'fr-product-tasks-1' and tasks['round'] == phase and
            tuple(tasks['tasks']) == TASKS[phase], 'keep all four tasks in the declared order')
    for family, task in zip(FAMILIES, tasks['tasks'].values()):
        require(task['family'] == family and task['request'] and task['check'],
                'each task needs its declared family, request and public check')
        if family != 'proof':
            reserved, = [row for row in reservation['projects'] if row['round'] == phase and row['task'] == family]
            require(all(task[key] == reserved[key] for key in ('repository', 'revision')),
                    'task source differs from the project reserved before inspection')
    paths = [DIRECTORY / name for name in ('protocol.json', 'reservation.json', 'checkers.json')]
    paths += [task_path, CLIENTS / 'clients.json', CLIENTS / 'profiles.json']
    paths += [p.relative_to(root) for p in sorted((root / DIRECTORY / phase / 'fixtures').rglob('*')) if p.is_file()]
    if released is not None:
        paths.append(DIRECTORY / 'validation-release.json')
    result = {'milestone': milestone, 'live_calls_enabled': False, 'profiles': original['profiles'],
        'clients': original['clients'], 'requirements': {**protocol, 'tasks': tasks['tasks']},
        'bindings': {str(path): file_digest(root / path) for path in paths},
        'attempts': [row for row in campaign(root)['attempts'] if row['round'] == phase],
        'billing': {'actual': None, 'estimated': None, 'authorization': protocol['authorization']}}
    return {**result, 'sha256': hashlib.sha256(encoded(result)).hexdigest()}
