"""Small, disclosed development examples; no unfamiliar packages are imported."""
import json

CASES = {
    'local': {'imports': False, 'declarations': 8, 'steps': 4096},
    'imported': {'imports': True, 'declarations': 8, 'steps': 4096},
    'many-declarations': {'imports': False, 'declarations': 200, 'steps': 4096},
    'unfinished-budget': {'imports': False, 'declarations': 8, 'steps': 1},
}
STAGES = (
    ('first-question', 'forwards', 'original'),
    ('different-question', 'stopped', 'original'),
    ('same-question', 'stopped', 'original'),
    ('edited-helper', 'stopped', 'edited'),
    ('unknown-startup', 'forwards', 'unknown'),
    ('unknown-again', 'forwards', 'unknown'),
    ('repaired-startup', 'stopped', 'original'),
)
STRATEGIES = ('fresh', 'saved-startup', 'saved-answer')


def sources(case, state):
    settings = CASES[case]
    setup = "table = {}\ndef register():\n    table['label'] = 'ready'\nregister()\n"
    helpers = ('def relay(value):\n    return value\n'
               'def discard(value):\n    return ' + ('value' if state == 'edited' else '0') + '\n')
    unused = ''.join(f'def unused_{i}(value={i}):\n    return value\n'
                     for i in range(settings['declarations']))
    entries = ('def forwards():\n    return sink(relay(source()))\n'
               'def stopped():\n    return sink(discard(source()))\n')
    app = ('from helpers import relay, discard\n' if settings['imports'] else setup + helpers + unused)
    app += entries
    if state == 'unknown':
        app += 'opaque()\n'
    files = {'app.py': app, 'rules.json': json.dumps({
        'version': 'startup-cost', 'sources': ['source'], 'sinks': ['sink'], 'sanitizers': {}})}
    if settings['imports']:
        files['helpers.py'] = setup + helpers + unused
    return files


def install(root, case, state):
    root.mkdir(parents=True, exist_ok=True)
    for name, contents in sources(case, state).items():
        (root / name).write_text(contents, encoding='utf-8')


def expected(case, stage):
    _, function, state = STAGES[stage]
    complete = case != 'unfinished-budget' and state != 'unknown'
    startup_reused = complete and stage in (1, 2)
    answer_reused = complete and stage in (2, 6)
    return {'complete': complete, 'startup_reused': startup_reused,
            'answer_reused': answer_reused,
            'python': {'error': 'NameError', 'returned': None} if state == 'unknown' else {
                'error': None, 'returned': 17 if function == 'forwards' or state == 'edited' else 0}}
