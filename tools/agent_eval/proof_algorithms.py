"""Qualify fixed executable Lean algorithms before any live agent attempt."""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import re
import subprocess

from . import selected_tasks as common
from .selected_plan import ALGORITHM_STUDIES, ROOT, encoded, require

DIRECTORY = Path('tests/agent-eval/proof-algorithms')
SPEC = 'specs/FrSpecs/Euclid.lean'
OBLIGATIONS = ('euclid_decreases', 'euclid_correct')
DECLARATIONS = ('euclid_decreases', 'euclid', 'euclid_correct')
CASES = ((0, 0), (0, 7), (7, 0), (1, 1), (48, 18), (18, 48),
         (1071, 462), (13, 17), (12, 36), (36, 12), (144, 89),
         (1000000000000000000000000, 250000000000000000000000))


def spans(text):
    """Require exactly the original, nonoverlapping, whole-line marker pairs."""
    result = {}
    markers = list(re.finditer(r'^  -- fr:proof-(begin|end) ([^\n]+)\n', text, re.M))
    require([(item[1], item[2]) for item in markers] ==
            [(kind, name) for name in OBLIGATIONS for kind in ('begin', 'end')],
            'preserve both proof marker pairs in their original order')
    require(text.count('fr:proof-') == len(markers), 'unexpected proof marker')
    for begin, end in zip(markers[::2], markers[1::2]):
        require(begin.end() < end.start(), 'proof region is empty or reversed')
        result[begin[2]] = (begin.end(), end.start())
    return result


def masked(text):
    for name, (begin, end) in reversed(list(spans(text).items())):
        text = text[:begin] + '<proof ' + name + '>\n' + text[end:]
    return text


def insert(text, name, tactics):
    begin, end = spans(text)[name]
    return text[:begin] + ''.join('  ' + line + '\n' for line in tactics.strip().splitlines()) + text[end:]


def boundaries(project, prepared):
    current, original = common.sources(project), prepared['original']
    changed = sorted(name for name in current.keys() | original.keys() if current.get(name) != original.get(name))
    require(set(changed) <= {SPEC} and SPEC in current, 'change only the two declared proof regions')
    require(current[SPEC]['mode'] == original[SPEC]['mode'], 'preserve the source file mode')
    require(masked((project / SPEC).read_text()) == prepared['protected_source'],
            'preserve the executable algorithm, claims, assumptions and surrounding source')
    return changed


def prepare(binary, project, output, fixture):
    require(not project.exists(), 'use a fresh algorithm project')
    project.mkdir(parents=True)
    output.mkdir(parents=True, exist_ok=False)
    common.fr(binary, project, ['spec', 'init', 'specs', '--write'], output / 'init.json')
    source = project / SPEC
    source.parent.mkdir(exist_ok=True)
    source.write_bytes(fixture.read_bytes())
    (project / 'specs/FrSpecs.lean').write_bytes((fixture.parent / 'FrSpecs.lean').read_bytes())
    built = common.command(['lake', 'build'], project / 'specs', output / 'build.json')
    require(built['returncode'] == 0, 'the original executable algorithm must build with disclosed proof debt')
    goals = common.fr(binary, project, ['spec', 'goals', 'specs'], output / 'goals.json')
    require(goals['omitted'] == 0 and {row['name'] for row in goals['catalog']} == set(OBLIGATIONS),
            'disclose exactly the termination and correctness obligations')
    for name in OBLIGATIONS:
        task = common.fr(binary, project, ['spec', 'proof-task', SPEC + '::' + name,
                        '--token-limit', '4096'], output / (name + '-task.json'))
        require(task['goal']['name'] == name, 'the proof task names the wrong obligation')
    for name in common.sources(project):
        (project / name).chmod(0o644)
    common.comparison_grade.BASE.git(project, 'init', '-q', '-b', 'main')
    (project / '.git/info/exclude').write_text('\n'.join(sorted((common.IGNORED - {'.git'}) | {'change.patch'})) + '\n')
    common.comparison_grade.BASE.initialize(project)
    prepared = {'task': 'euclid', 'spec': SPEC, 'obligations': list(OBLIGATIONS),
                'fixture': str(fixture.relative_to(ROOT / DIRECTORY)),
                'original': common.sources(project), 'protected_source': masked(source.read_text()),
                'proved_program': 'The actual executable Lean Algorithm.euclid, for every pair of Nat inputs.',
                'source_model_bridge': 'No Rust/Python translation is claimed or involved.'}
    common.save(output / 'project.json', prepared)
    common.retain_seed(project, prepared, output)
    return prepared


def axiom_report(text):
    """Read all three declarations, never infer a whole algorithm from one theorem."""
    result = {}
    pattern = (r"'Algorithm\.([^']+)' (?:does not depend on any axioms|"
               r'depends on axioms:\s*\[([^]]*)\])')
    for match in re.finditer(pattern, text):
        name = match[1]
        require(name in DECLARATIONS and name not in result, 'unexpected or repeated axiom audit declaration')
        result[name] = sorted(item.strip() for item in (match[2] or '').split(',') if item.strip())
    require(set(result) == set(DECLARATIONS), 'audit both proofs and the executable function')
    return result


def check(project, prepared, binary, output):
    output.mkdir(parents=True, exist_ok=False)
    try:
        changed = boundaries(project, prepared)
    except ValueError as error:
        return {'passed': False, 'stage': 'protected source', 'reason': str(error)}
    strict = common.command([str(binary), '-C', str(project), '--json', 'spec', 'check',
                             'specs', '--strict', '--max-debt', '0'], project, output / 'strict.json')
    if strict['returncode'] != 0:
        return {'passed': False, 'stage': 'unfinished proof', 'checks': [strict]}
    built = common.command(['lake', 'build', '--wfail'], project / 'specs', output / 'build.json')
    if built['returncode'] != 0:
        return {'passed': False, 'stage': 'Lean package', 'checks': [strict, built]}
    audit = output / 'Axioms.lean'
    audit.write_text('prelude\nimport FrSpecs.Euclid\n' + ''.join('#print axioms Algorithm.' + name + '\n' for name in DECLARATIONS))
    axioms = common.command(['lake', 'env', 'lean', '-j1', '-DwarningAsError=true', str(audit.resolve())],
                            project / 'specs', output / 'axioms.json')
    if axioms['returncode'] != 0:
        return {'passed': False, 'stage': 'axiom audit', 'checks': [strict, built, axioms]}
    dependencies = axiom_report(axioms['stdout'])
    approved = all(set(names) <= common.APPROVED_AXIOMS for names in dependencies.values())
    runtime = output / 'Execution.lean'
    runtime.write_text('prelude\nimport FrSpecs.Euclid\n' + ''.join(
        f'#eval Algorithm.euclid {a} {b}\n' for a, b in CASES))
    executed = common.command(['lake', 'env', 'lean', '-j1', '-DwarningAsError=true', str(runtime.resolve())],
                             project / 'specs', output / 'execution.json')
    expected = ''.join(str(math.gcd(a, b)) + '\n' for a, b in CASES)
    return {'passed': approved and executed['returncode'] == 0 and executed['stdout'] == expected,
            'stage': 'complete', 'changed_paths': changed, 'axioms': dependencies,
            'executable_lean_proved': approved, 'finite_execution_cases': len(CASES),
            'scope': prepared['proved_program'], 'source_model_bridge': prepared['source_model_bridge'],
            'checks': [strict, built, axioms, executed]}


def qualify_declarations(output):
    expected = (ROOT / DIRECTORY / 'declarations.txt').read_text()
    names = [line.split()[0].split('.{', 1)[0] for line in expected.splitlines() if not line.startswith(' ')]
    require(names == ['Nat.gcd_comm', 'Nat.gcd_rec', 'Nat.gcd_zero_right',
                      'Nat.mod_lt', 'Nat.pos_of_ne_zero', 'Nat.strongRecOn'], 'preserve the fixed fact handout')
    probe = output / 'Declarations.lean'
    source = (ROOT / DIRECTORY / 'fixtures/Euclid.lean').read_text()
    imports, separator, _ = source.partition('namespace Algorithm\n')
    require(separator, 'qualify declarations with the actual task imports')
    probe.write_text(imports + ''.join('#check ' + name + '\n' for name in names))
    checked = common.command(['lean', '-j1', str(probe.resolve())], output, output / 'declarations.json')
    require(checked['returncode'] == 0 and ' '.join(checked['stdout'].split()) == ' '.join(expected.split()),
            'supplied declarations differ from the pinned Lean installation')
    return common.digest(ROOT / DIRECTORY / 'declarations.txt')


def qualify_reserved_reference(output):
    source = (ROOT / DIRECTORY / 'reserved/Reverse.lean').read_text()
    require(source.count('  sorry\n') == 1, 'keep the one reserved transfer proof')
    tactics = (ROOT / DIRECTORY / 'controls/reverse-into-correct.lean').read_text()
    checked_source = source.replace('  -- fr:debt reverseInto_correct\n', '').replace(
        '  sorry\n', ''.join('  ' + line + '\n' for line in tactics.splitlines()))
    probe = output / 'ReservedReference.lean'
    probe.write_text(checked_source + '\n#print axioms Transfer.reverseInto\n'
                     '#print axioms Transfer.reverseInto_correct\n'
                     '#eval Transfer.reverseInto [1, 2, 3] [9]\n')
    checked = common.command(['lean', '-j1', '-DwarningAsError=true', str(probe.resolve())],
                             output, output / 'reserved-reference.json')
    require(checked['returncode'] == 0 and 'sorryAx' not in checked['stdout'] and
            '[3, 2, 1, 9]\n' in checked['stdout'], 'the reserved executable reference must prove its unchanged claim')


def validate_controls(value, study):
    from . import selected_local as local
    from . import selected_local_grade as public
    require(study in ALGORITHM_STUDIES and value['passed'] is True and value['live_model_calls'] == 0,
            'require successful hosted algorithm controls without live calls')
    require(value['controls'] == {name: {'accepted': name in POSITIVE_CONTROLS} for name in VARIANTS},
            'both reference proofs and all twelve rejection controls must pass')
    require(value['bindings'] == local.implementation_bindings(study) and
            value['core_checker_sha256'] == public.core_digest(study) and
            value['declarations_sha256'] == common.digest(ROOT / DIRECTORY / 'declarations.txt') and
            value['native'] == {'passed': True, 'live_model_calls': 0, 'exchanges': 6} and
            value['reference_proofs_in_seed'] is False and value['private_image'].startswith('sha256:') and
            len(value['source_commit']) == 40 and str(value['run']).isdigit(),
            'freeze the exact checker, handouts, native clients and reference-free task seed')
    require(re.fullmatch('[0-9a-f]{64}', value['private_image_archive_sha256']) is not None,
            'freeze the qualified private checker archive')
    retained = ROOT / DIRECTORY / 'interrupted-results/summary.json'
    require(value['retained_agent_patch']['patch_accepted'] is True and
            value['retained_agent_patch']['summary_sha256'] == common.digest(retained),
            'retain and independently check the earlier submitted algorithm proof')
    return value


POSITIVE_CONTROLS = {'reference', 'reference-with-multiline-lambda'}
VARIANTS = ('reference', 'reference-with-multiline-lambda', 'unfinished', 'termination-only', 'correctness-only', 'bad-tactics', 'recursive-simp-loop',
            'changed-claim', 'changed-source', 'library-substitution', 'extra-axiom',
            'changed-package', 'missing-patch', 'wrong-patch')


def retained_submission(directory, prepared):
    summary = json.loads((directory / 'summary.json').read_text())
    require(summary['task_source_sha256'] == hashlib.sha256(encoded(prepared['original'])).hexdigest(),
            'recheck the original algorithm source supplied to the interrupted batch')
    submitted = [row for row in summary['attempts'] if row['patch_file'] is not None]
    require(len(submitted) == 1, 'retain the one originally submitted patch')
    row, = submitted
    require(row['patch_file'] == 'patches/' + row['id'] + '.patch' and
            re.fullmatch('[a-z0-9-]+', row['id']) is not None, 'unsafe retained patch path')
    patch = directory / row['patch_file']
    require(not patch.is_symlink() and patch.is_file() and common.digest(patch) == row['patch_sha256'],
            'the original interrupted-batch patch changed')
    return row, patch


def controls(binary, output, native):
    from . import selected_grading as grading
    from . import selected_local as local
    from . import selected_local_grade as public
    require(os.environ.get('GITHUB_ACTIONS') == 'true', 'run full algorithm qualification on GitHub')
    output.mkdir(parents=True, exist_ok=False)
    original = output / 'original'
    prepared = prepare(binary, original, output / 'inputs/euclid', ROOT / DIRECTORY / 'fixtures/Euclid.lean')
    declarations_digest = qualify_declarations(output)
    qualify_reserved_reference(output)
    profiles_path = ROOT / 'tests/agent-eval/proof-writing/profiles.json'
    require(native['profiles'] == json.loads(profiles_path.read_text()) and
            native['profiles_sha256'] == common.digest(profiles_path), 'native controls checked different model settings')
    require(native['passed'] is True and native['live_model_calls'] == 0 and
            len(native['exchanges']) == 6 and all(row['status'] == 'passed' for row in native['exchanges'].values()),
            'qualify all three native clients before the algorithm comparisons')
    image = grading.build(output / 'private-image')
    image_archive = output / 'grader-image.tar.gz'
    subprocess.run(grading.DOCKER + ['save', '--output', str(output / 'grader-image.tar'), image], check=True, timeout=60)
    import gzip
    import shutil
    with (output / 'grader-image.tar').open('rb') as source, gzip.open(image_archive, 'wb') as destination:
        shutil.copyfileobj(source, destination)
    (output / 'grader-image.tar').unlink()
    rust = Path(subprocess.check_output(['rustc', '--print', 'sysroot'], text=True).strip())
    solved = (original / SPEC).read_text()
    for name in OBLIGATIONS:
        tactics = (ROOT / DIRECTORY / 'controls' / (name.replace('_', '-') + '.lean')).read_text()
        solved = insert(solved, name, tactics)
    verdicts = {}
    for variant in VARIANTS:
        candidate = output / ('candidate-' + variant)
        common.checkout(original, candidate)
        text = solved
        if variant == 'unfinished':
            text = insert((original / SPEC).read_text(), 'euclid_correct', 'skip')
        elif variant == 'termination-only':
            text = insert(solved, 'euclid_correct', 'sorry')
        elif variant == 'correctness-only':
            text = insert(solved, 'euclid_decreases', 'sorry')
        elif variant == 'bad-tactics':
            text = insert(solved, 'euclid_correct', 'exact Nat.gcd_comm a b')
        elif variant == 'recursive-simp-loop':
            text = insert(solved, 'euclid_correct', 'induction b <;> simp [euclid, Nat.gcd]')
        elif variant == 'changed-claim':
            text = solved.replace('euclid a b = Nat.gcd a b', 'euclid a b = euclid a b')
        elif variant == 'changed-source':
            text = solved.replace('then a else', 'then 0 else')
        elif variant == 'library-substitution':
            begin, end = solved.index('def euclid '), solved.index('\ntheorem euclid_correct ')
            text = solved[:begin] + 'def euclid (a b : Nat) : Nat := Nat.gcd a b\n' + solved[end:]
        elif variant == 'extra-axiom':
            text = solved + '\naxiom cheat : False\n'
        elif variant == 'changed-package':
            path = candidate / 'specs/lakefile.toml'
            path.write_text(path.read_text() + '\n# edited by candidate\n')
        (candidate / SPEC).write_text(text)
        if variant in POSITIVE_CONTROLS:
            # Qualify the actual interactive authoring path, not only direct replacement.
            (candidate / SPEC).write_bytes((original / SPEC).read_bytes())
            for name in OBLIGATIONS:
                draft = candidate / '.fr' / (name + '.tactics')
                draft.parent.mkdir(exist_ok=True)
                draft.write_text('skip\n')
                state = common.fr(binary, candidate, ['spec', 'proof-state', SPEC + '::' + name,
                    '--from', str(draft), '--at', '1:5', '--token-limit', '4096', '--timeout-seconds', '30'],
                    output / (variant + '-' + name + '-state.json'))
                require(state['acceptance'] == 'not-checked', 'interactive state is not proof acceptance')
                control = 'euclid-correct-lambda.lean' if (variant == 'reference-with-multiline-lambda' and
                                                          name == 'euclid_correct') else name.replace('_', '-') + '.lean'
                draft.write_text((ROOT / DIRECTORY / 'controls' / control).read_text())
                checked = common.fr(binary, candidate, ['spec', 'proof-check', SPEC + '::' + name,
                                    '--from', str(draft), '--token-limit', '4096'], output / (variant + '-' + name + '-check.json'))
                require(checked['passed'] is True, 'reference tactics must pass through fr')
                saved = common.fr(binary, candidate, ['spec', 'prove', SPEC + '::' + name,
                                  '--from', str(draft), '--write'], output / (variant + '-' + name + '-save.json'))
                require(saved['applied'] is True, 'reference tactics must be saved through fr')
        patch = output / (variant + '.patch')
        patch.write_bytes(common.comparison_grade.BASE.git(candidate, 'diff', '--binary', 'HEAD').stdout)
        if variant == 'wrong-patch':
            patch.write_text('not a patch\n')
        verdict = grading.grade(image, 'euclid', original, prepared, binary, rust,
                                None if variant == 'missing-patch' else patch, output / ('grade-' + variant),
                                milestone='M3-algorithm-facts', tools={})
        common.save(output / ('verdict-' + variant + '.json'), verdict)
        require(verdict['passed'] == (variant in POSITIVE_CONTROLS), 'algorithm control misgraded: ' + variant)
        verdicts[variant] = {'accepted': verdict['passed']}
    retained = ROOT / DIRECTORY / 'interrupted-results'
    historical = output / 'historical-original'
    historical_prepared = prepare(binary, historical, output / 'historical-inputs', retained / 'Euclid.lean')
    row, patch = retained_submission(retained, historical_prepared)
    checked = grading.grade(image, 'euclid', historical, historical_prepared, binary, rust, patch,
                            output / 'grade-retained-agent', milestone='M3-algorithm-facts', tools={})
    require(checked['passed'] is True, 'the retained Kimi patch must independently prove the fixed algorithm')
    retained_check = {'patch_accepted': True, 'id': row['id'], 'patch_sha256': common.digest(patch),
                      'summary_sha256': common.digest(retained / 'summary.json'),
                      'scope': 'Current qualified checker replays the original submitted patch. This does not repair the interrupted resource accounting or establish a guidance advantage.'}
    common.save(output / 'retained-agent-patch.json', retained_check)
    result = {'passed': True, 'live_model_calls': 0, 'run': os.environ['GITHUB_RUN_ID'],
              'source_commit': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
              'fr_sha256': common.digest(binary), 'controls': verdicts,
              'program': prepared['proved_program'], 'source_model_bridge': prepared['source_model_bridge'],
              'reference_proofs_in_seed': False,
              'bindings': local.implementation_bindings('M3-algorithm-facts'),
              'core_checker_sha256': public.core_digest('M3-algorithm-facts'),
              'declarations_sha256': declarations_digest, 'private_image': image,
              'private_image_archive_sha256': common.digest(image_archive),
              'retained_agent_patch': retained_check,
              'native': {'passed': True, 'live_model_calls': 0, 'exchanges': 6}}
    for study in ALGORITHM_STUDIES:
        validate_controls(result, study)
    common.save(output / 'controls.json', result)
    return result


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--fr', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--native', type=Path, required=True)
    arguments = parser.parse_args()
    print(json.dumps(controls(arguments.fr.resolve(), arguments.output.resolve(), json.loads(arguments.native.read_text()))))
