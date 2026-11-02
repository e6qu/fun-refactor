"""Freeze algorithm comparisons and render one plan in two equivalent forms."""
import hashlib
import json
from pathlib import Path

from . import selected_tasks as common
from .selected_plan import ROOT, DIRECTORY as CLIENT_DIRECTORY, encoded, require

DIRECTORY = Path('tests/agent-eval/proof-algorithms')
STUDIES = {'M3-algorithm-facts': 'facts', 'M3-algorithm-presentation': 'presentation'}


def prepare(root, study):
    require(study in STUDIES, 'choose the algorithm comparison explicitly')
    requirements = json.loads((root / DIRECTORY / 'requirements.json').read_text())
    phase, = [row for row in requirements['phases'] if row['name'] == STUDIES[study]]
    profiles = json.loads((root / DIRECTORY.parent / 'proof-writing/profiles.json').read_text())
    from .selected_plan import prepare as selected_plan
    require(profiles == selected_plan(root, 'M3')['profiles'], 'preserve the reviewed model pins')
    clients = json.loads((root / CLIENT_DIRECTORY / 'clients.json').read_text())
    expected_profiles = ['opencode2-glm-flash', 'opencode2-kimi-k3']
    expected_conditions = ['select-facts', 'supplied-declarations']
    if STUDIES[study] == 'presentation':
        expected_profiles = ['codex-luna-fast', *expected_profiles]
        expected_conditions = ['checklist', 'graph']
    require(phase['profiles'] == expected_profiles and phase['conditions'] == expected_conditions and
            requirements['repetitions'] == 2 and requirements['limits']['parallel_attempts'] == 1,
            'preserve the selected agents and balanced comparisons')
    profiles = {name: profiles[name] for name in expected_profiles}
    attempts = []
    for repetition in range(2):
        for profile_index, profile in enumerate(profiles):
            order = expected_conditions if (repetition + profile_index) % 2 == 0 else expected_conditions[::-1]
            for position, condition in enumerate(order):
                attempts.append({'id': f'{profile}-euclid-{condition}-{repetition + 1}',
                    'profile': profile, 'task': 'euclid', 'setup': condition, 'repetition': repetition + 1,
                    'position': position + 1, 'status': 'unstarted', 'outcome': None,
                    'usage': None, 'cost': None, 'fr_contribution': None})
    require(len(attempts) == phase['attempts'] == (8 if STUDIES[study] == 'facts' else 12),
            'retain every planned fresh session')
    paths = [root / CLIENT_DIRECTORY / 'clients.json', root / DIRECTORY.parent / 'proof-writing/profiles.json']
    paths += [path for path in (root / DIRECTORY).rglob('*') if path.is_file()
              and path.suffix in {'.json', '.lean', '.txt'} and 'local-results' not in path.parts]
    bindings = {str(path.relative_to(root)): common.digest(path) for path in sorted(paths)}
    result = {'milestone': study, 'live_calls_enabled': False, 'profiles': profiles, 'clients': clients,
              'requirements': {**requirements, 'tasks': {'euclid': {'request': request()}},
                               'setups': expected_conditions, 'attempts': phase['attempts']},
              'bindings': bindings, 'attempts': attempts,
              'billing': {'actual': None, 'estimated': None, 'authorization': None}}
    return {**result, 'sha256': hashlib.sha256(encoded(result)).hexdigest()}


def request():
    return ('Prove termination and correctness of the fixed executable Lean Euclid GCD algorithm. '
        'GCD means the largest nonnegative whole number dividing both inputs; gcd(48,18)=6. '
        'Use the standard convention gcd(0,0)=0. '
        'The function repeatedly replaces (a,b) with (b,a%b), where % is the remainder. '
        'The prelude command turns off the automatic library import; both modules import only what they need. '
        'It returns a when b is zero. Fill BOTH euclid_decreases and euclid_correct in '
        'specs/FrSpecs/Euclid.lean. Change only their marked proof regions. Keep the executable '
        'implementation, claims, assumptions, proof markers and package settings fixed. '
        'Use targets specs/FrSpecs/Euclid.lean::euclid_decreases and specs/FrSpecs/Euclid.lean::euclid_correct. '
        'Read .fr/declarations.txt and .fr/proof-plan.txt and the fr skill. '
        'Read .agents/skills/fr/references/lean-algorithms.md before drafting. '
        'Use fr proof-task and proof-state to inspect each target, proof-check to accept tactics, '
        'and prove --write to save each checked proof. Read passed in the JSON. '
        'A receipt for one region does not finish the other. Run the supplied package check '
        'once after both proofs pass, deliver git diff --binary HEAD as change.patch and finish. '
        'This proves the actual Lean function; no translation from Rust or Python is involved.')


def render(plan, condition):
    require(condition in {'checklist', 'graph'}, 'choose a presentation explicitly')
    # Both renderings contain every exact node, edge and explanation once.
    nodes = [f'[{row["id"]}] {row["text"]}' for row in plan['nodes']]
    if condition == 'checklist':
        edges = [f'{row["from"]} {row["relation"]} {row["to"]}' for row in plan['edges']]
        return '\n'.join('- ' + line for line in nodes + edges) + '\n\n' + plan['meaning'] + '\n'
    edges = [f'{row["from"]} -- {row["relation"]} --> {row["to"]}' for row in plan['edges']]
    return '\n'.join(nodes) + '\n\n' + '\n'.join(edges) + '\n\n' + plan['meaning'] + '\n'


def install(project, condition, study):
    require(study in STUDIES and condition in prepare(ROOT, study)['requirements']['setups'],
            'condition does not belong to the frozen comparison')
    directory = project / '.fr'
    directory.mkdir(exist_ok=True)
    declarations = ((ROOT / DIRECTORY / 'declarations.txt').read_text()
                    if condition != 'select-facts' else
                    'Select current library facts from the installed Lean standard library.\n')
    plan = (render(json.loads((ROOT / DIRECTORY / 'guidance.json').read_text()), condition)
            if STUDIES[study] == 'presentation' else
            'Use the fixed task, shared fr skill and actual Lean feedback to choose your steps.\n')
    protected = {}
    for name, content in (('declarations.txt', declarations), ('proof-plan.txt', plan)):
        path = directory / name
        require(not path.exists(), 'do not overwrite a guidance handout')
        path.write_text(content)
        protected[str(path.relative_to(project))] = common.digest(path)
    return protected
