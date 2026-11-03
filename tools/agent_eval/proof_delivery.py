"""Check delivery of supplied algorithm proofs, without contacting a model."""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import shutil
import subprocess

from . import proof_algorithms as algorithms, selected_tasks as common
from .selected_plan import ROOT, encoded, require

DIRECTORY = Path('tests/agent-eval/proof-delivery')
SETUPS = ('shared-skill', 'finish-card')
LIMITS = {'wall_seconds': 150, 'rss_bytes': 805306368, 'cpu_limit_seconds': 75,
          'disk_bytes': 536870912, 'transcript_bytes': 1048576, 'parallel_attempts': 1}


def protocol():
    from .selected_local_grade import core_digest
    requirements = json.loads((ROOT / DIRECTORY / 'requirements.json').read_text())
    require(requirements['conditions'] == list(SETUPS) and requirements['repetitions'] == 2 and
            requirements['limits'] == LIMITS and requirements['live_calls_enabled'] is False,
            'preserve the explicit comparison, attempt limits and disabled live calls')
    profiles = json.loads((ROOT / requirements['profiles_file']).read_text())
    clients = json.loads((ROOT / requirements['clients_file']).read_text())
    require(list(profiles) == ['codex-luna-fast', 'opencode2-glm-flash', 'opencode2-kimi-k3'],
            'retain all three selected profiles')
    attempts = []
    for repetition in range(2):
        for index, profile in enumerate(profiles):
            order = SETUPS if (repetition + index) % 2 == 0 else SETUPS[::-1]
            for setup in order:
                attempts.append({'id': f'{profile}-delivery-{setup}-{repetition + 1}',
                                 'profile': profile, 'setup': setup, 'repetition': repetition + 1,
                                 'status': 'unstarted', 'outcome': None})
    paths = [ROOT / DIRECTORY / 'requirements.json', ROOT / requirements['profiles_file'],
             ROOT / requirements['clients_file'],
             ROOT / 'tools/agent_eval/proof_delivery.py', ROOT / 'tools/agent_eval/test_proof_delivery.py']
    paths += [path for path in (ROOT / 'skills/fr').rglob('*') if path.is_file()]
    paths += [ROOT / algorithms.DIRECTORY / 'controls' / (name.replace('_', '-') + '.lean')
              for name in algorithms.OBLIGATIONS]
    paths += [ROOT / algorithms.DIRECTORY / 'fixtures' / name for name in ('Euclid.lean', 'FrSpecs.lean')]
    paths.append(ROOT / '.github/workflows/proof-algorithms.yml')
    value = {'requirements': requirements, 'profiles': profiles, 'clients': clients, 'attempts': attempts,
             'bindings': {str(path.relative_to(ROOT)): common.digest(path) for path in sorted(paths)},
             'algorithm_checker_sha256': core_digest('M3-algorithm-facts'),
             'requests': {setup: request(setup, requirements) for setup in SETUPS},
             'live_calls_enabled': False,
             'scope': 'Prepared delivery-only comparison; no proof-discovery or live-agent result.'}
    return {**value, 'sha256': hashlib.sha256(encoded(value)).hexdigest()}


def request(setup, requirements):
    require(setup in SETUPS, 'choose the delivery condition explicitly')
    text = requirements['request']
    if setup == 'finish-card':
        text += ' Read .fr/finish-card.txt before acting.'
    return text


def install(project, setup, lake):
    require(setup in SETUPS, 'choose the delivery condition explicitly')
    directory = project / '.fr'
    require(not directory.is_symlink(), 'the supplied drafts need a real project directory')
    directory.mkdir(exist_ok=True)
    protected = []
    for name in algorithms.OBLIGATIONS:
        draft = directory / (name + '.tactics')
        require(not draft.exists(), 'do not replace a supplied proof draft')
        draft.write_bytes((ROOT / algorithms.DIRECTORY / 'controls' /
                           (name.replace('_', '-') + '.lean')).read_bytes())
        protected.append(draft)
    checks = directory / 'checks.json'
    require(not checks.exists(), 'do not replace package checks')
    common.save(checks, {'schema': 1, 'checks': [{'name': 'lean',
        'argv': [str(lake), '-d', 'specs', 'build', '--wfail'], 'cwd': '.', 'timeout_seconds': 120,
        'covers': ['both fixed proofs and the complete Lean package']}]})
    protected.append(checks)
    skill = project / '.agents/skills/fr'
    shutil.copytree(ROOT / 'skills/fr', skill)
    protected += [path for path in skill.rglob('*') if path.is_file()]
    if setup == 'finish-card':
        card = directory / 'finish-card.txt'
        card.write_text('\n\n'.join(protocol()['requirements']['finish_card']) + '\n')
        protected.append(card)
    return {str(path.relative_to(project)): {'sha256': common.digest(path),
            'mode': path.stat().st_mode & 0o777} for path in protected}


def submission(project, protected):
    """Only the requested root patch is delivered; never search for a misplaced copy."""
    for name, entry in protected.items():
        path = project / name
        if any((project / parent).is_symlink() for parent in Path(name).parents):
            return None, 'protected input is reached through a symlink'
        if (path.is_symlink() or not path.is_file() or common.digest(path) != entry['sha256'] or
                path.stat().st_mode & 0o777 != entry['mode']):
            return None, 'protected input changed: ' + name
    patch = project / 'change.patch'
    if patch.is_symlink() or not patch.is_file():
        return None, 'missing regular change.patch in the project root'
    if not 0 < patch.stat().st_size <= 1024**2:
        return None, 'patch must be nonempty and at most 1 MiB'
    return patch, None


def completion(resources, patch_observed_seconds, accounting_complete):
    """Read collector-owned measurements, never an agent's declaration of success."""
    if not isinstance(resources, dict):
        return False
    if any(resources.get(name) is not None for name in ('stop_reason', 'monitor_error', 'launch_error')):
        return False
    if any(name not in resources for name in ('stop_reason', 'monitor_error', 'launch_error')):
        return False
    if type(resources.get('exit_code')) is not int or resources['exit_code'] != 0 or accounting_complete is not True:
        return False
    ceilings = {'elapsed_seconds': LIMITS['wall_seconds'],
                'sampled_aggregate_rss_bytes': LIMITS['rss_bytes'],
                'sampled_cpu_seconds': LIMITS['cpu_limit_seconds'],
                'sampled_disk_growth_bytes': LIMITS['disk_bytes'],
                'transcript_bytes': LIMITS['transcript_bytes']}
    for name, ceiling in ceilings.items():
        value = resources.get(name)
        if type(value) not in (int, float) or not 0 <= value <= ceiling or not math.isfinite(value):
            return False
    return (type(patch_observed_seconds) in (int, float) and
            0 <= patch_observed_seconds <= resources['elapsed_seconds'] <= LIMITS['wall_seconds'] and
            math.isfinite(patch_observed_seconds))


def timing_controls():
    """Synthetic observations test the scorer; these are not measured attempt times."""
    resources = {'exit_code': 0, 'stop_reason': None, 'monitor_error': None, 'launch_error': None,
                 'elapsed_seconds': 100, 'sampled_aggregate_rss_bytes': 1024,
                 'sampled_cpu_seconds': 1, 'sampled_disk_growth_bytes': 1024, 'transcript_bytes': 1024}
    cases = {'on-time': (resources, 90, True, True),
             'late-patch': ({**resources, 'elapsed_seconds': 151}, 151, True, False),
             'late-accounting': ({**resources, 'elapsed_seconds': 151}, 90, True, False),
             'unfinished-accounting': (resources, 90, False, False),
             'stopped': ({**resources, 'stop_reason': 'wall_seconds'}, 90, True, False)}
    result = {}
    for name, (record, observed, accounting, expected) in cases.items():
        accepted = completion(record, observed, accounting)
        require(accepted == expected, 'completion control misgraded: ' + name)
        result[name] = {'completion_accepted': accepted, 'synthetic_observation': True}
    return result


def qualify(binary, qualification, output):
    from . import selected_grading as grading
    require(os.environ.get('GITHUB_ACTIONS') == 'true', 'run full delivery checks on GitHub')
    controls = json.loads((qualification / 'controls.json').read_text())
    algorithms.validate_controls(controls, 'M3-algorithm-facts')
    require(controls['fr_sha256'] == common.digest(binary), 'use the qualified fr binary')
    output.mkdir(parents=True, exist_ok=False)
    plan = protocol()
    common.save(output / 'protocol.json', plan)
    original = qualification / 'original'
    prepared = json.loads((qualification / 'inputs/euclid/project.json').read_text())
    rust = Path(subprocess.check_output(['rustc', '--print', 'sysroot'], text=True, timeout=10).strip())
    installed_lake = shutil.which('lake')
    require(installed_lake is not None, 'use the explicitly installed Lean toolchain')
    lake = Path(installed_lake).resolve()
    results = {}
    for setup in SETUPS:
        project = output / setup
        common.checkout(original, project)
        protected = install(project, setup, lake)
        transactions = []
        for name in algorithms.OBLIGATIONS:
            target = algorithms.SPEC + '::' + name
            draft = '.fr/' + name + '.tactics'
            checked = common.fr(binary, project, ['spec', 'proof-check', target, '--from', draft,
                                '--token-limit', '4096'], output / (setup + '-' + name + '-check.json'))
            require(checked['passed'] is True, 'the supplied proof must be correct')
            saved = common.fr(binary, project, ['spec', 'prove', target, '--from', draft, '--write'],
                              output / (setup + '-' + name + '-save.json'))
            require(saved['status'] == 'written' and saved['proof_checked'] is True and saved['applied'] is True,
                    'save both supplied proofs through fr')
            transactions.append(str(saved['transaction']))
        package = common.command([str(lake), '-d', 'specs', 'build', '--wfail'], project,
                                 output / (setup + '-package.json'))
        require(package['returncode'] == 0, 'the saved package must pass')
        exported = common.fr(binary, project, ['history', 'patch', *transactions, '--output', 'change.patch'],
                             output / (setup + '-export.json'))
        require([str(row['id']) for row in exported['transactions']] == transactions,
                'export both saved edits in save order')
        patch, error = submission(project, protected)
        require(error is None, 'correct delivery was rejected')
        verdict = grading.grade(controls['private_image'], 'euclid', original, prepared, binary, rust,
                                patch, output / ('grade-' + setup), milestone='M3-algorithm-facts', tools={})
        require(verdict['passed'] is True, 'independently check both delivered proofs')
        results[setup] = {'patch_accepted': True, 'patch_sha256': common.digest(patch),
                          'transactions': transactions, 'package_passed': True}
        if setup == SETUPS[0]:
            latest = project / '.fr/last-edit.patch'
            common.fr(binary, project, ['history', 'patch', transactions[-1], '--output', str(latest)],
                      output / 'last-edit-only-export.json')
            latest_verdict = grading.grade(controls['private_image'], 'euclid', original, prepared, binary, rust,
                                          latest, output / 'grade-last-edit-only',
                                          milestone='M3-algorithm-facts', tools={})
            require(latest_verdict['passed'] is False, 'the last edit alone cannot deliver the earlier proof')
            results['last-edit-only'] = {'patch_accepted': False}
            solved = (project / algorithms.SPEC).read_text()
            original_text = (original / algorithms.SPEC).read_text()
            for variant in ('termination-only', 'correctness-only', 'failed-package', 'changed-program', 'invalid-patch'):
                candidate = output / variant
                common.checkout(original, candidate)
                text = solved
                if variant == 'termination-only':
                    text = algorithms.insert(text, 'euclid_correct', 'sorry')
                elif variant == 'correctness-only':
                    text = algorithms.insert(text, 'euclid_decreases', 'sorry')
                elif variant == 'failed-package':
                    text = algorithms.insert(text, 'euclid_correct', 'exact False.elim (by contradiction)')
                elif variant == 'changed-program':
                    text = text.replace('then a else', 'then 0 else')
                    require(text != solved, 'the changed-program control must change the executable')
                require(algorithms.masked(original_text) == algorithms.masked(solved), 'preserve the fixed question')
                (candidate / algorithms.SPEC).write_text(text)
                bad_patch = candidate / 'change.patch'
                bad_patch.write_bytes(common.comparison_grade.BASE.git(candidate, 'diff', '--binary', 'HEAD').stdout)
                if variant == 'invalid-patch':
                    bad_patch.write_text('not a patch\n')
                rejected = grading.grade(controls['private_image'], 'euclid', original, prepared, binary, rust,
                                         bad_patch, output / ('grade-' + variant),
                                         milestone='M3-algorithm-facts', tools={})
                require(rejected['passed'] is False, 'incomplete or invalid delivery was accepted: ' + variant)
                results[variant] = {'patch_accepted': False}
            patch.rename(project / '.fr/change.patch')
            require(submission(project, protected)[0] is None, 'never recover a misplaced patch automatically')
            results['wrong-path'] = {'submission_accepted': False, 'stage': 'patch location'}
            (project / '.fr/change.patch').rename(patch)
            settings = project / '.fr/checks.json'
            settings.write_text('{"checks": []}\n')
            require(submission(project, protected)[0] is None, 'changed checking settings must fail')
            results['changed-settings'] = {'submission_accepted': False, 'stage': 'protected settings'}
    result = {'passed': True, 'live_model_calls': 0, 'live_calls_enabled': False,
              'protocol_sha256': plan['sha256'], 'fr_sha256': common.digest(binary),
              'qualification_run': controls['run'], 'controls': results, 'timing_controls': timing_controls(),
              'scope': 'Scripted delivery controls, not agent successes. Live workflow admission is still required.'}
    common.save(output / 'summary.json', result)
    return result


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--fr', type=Path, required=True)
    parser.add_argument('--qualification', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    print(json.dumps(qualify(args.fr.resolve(), args.qualification.resolve(), args.output.resolve())))
