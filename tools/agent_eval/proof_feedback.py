"""One assisted proof-repair diagnostic; never a replacement for the full M3 study."""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import subprocess
import tempfile

from . import proof_writing_tasks as proof
from . import selected_tasks as common
from .selected_plan import ROOT, encoded, require

DIRECTORY = Path('tests/agent-eval/proof-feedback')
TASK = 'division-repair'
VARIANTS = ('reference', 'unfinished', 'bad-tactics', 'changed-claim', 'changed-source', 'missing-patch', 'wrong-patch')
check = proof.check


def property_for(task):
    a, b = proof.variable('a'), proof.variable('b')
    zero, one = proof.integer(0, 'Int'), proof.integer(1, 'Int')
    assumptions = proof.conjunction(proof.relation('less-or-equal', zero, a), proof.relation('less-or-equal', one, b))
    return {'schema': 'fr-formal-property-1', 'task_digest': task['object_digest'], 'name': 'nonnegative',
            'parameters': [{'name': name, 'lean_type': 'Int'} for name in ('a', 'b')],
            'proposition': proof.implies(assumptions, proof.relation('less-or-equal', zero, proof.model(a, b)))}


def prepare_inputs(binary, project, output):
    require(not project.exists(), 'prepare the diagnostic in a new project')
    project.mkdir(parents=True)
    output.mkdir(parents=True, exist_ok=False)
    (project / 'src').mkdir()
    (project / 'src/quotient.rs').write_bytes((ROOT / DIRECTORY / 'fixtures/quotient.rs').read_bytes())
    (project / 'initial-tactics.txt').write_bytes((ROOT / DIRECTORY / 'fixtures/initial-tactics.txt').read_bytes())
    (project / 'Cargo.toml').write_text('[workspace]\n')
    common.fr(binary, project, ['spec', 'init', 'specs', '--write'], output / 'init.json')
    shape = common.fr(binary, project, ['spec', 'property-task', 'src/quotient.rs::quotient'], output / 'shape.json')
    claim = property_for(shape)
    common.save(output / 'property.json', claim)
    plan = common.fr(binary, project, ['spec', 'plan', 'src/quotient.rs::quotient', '--property-from',
                                    str(output / 'property.json')], output / 'plan-log.json')
    common.save(output / 'plan.json', plan)
    common.fr(binary, project, ['spec', 'scaffold', '--from', str(output / 'plan.json'), '--package', 'specs', '--write'],
              output / 'scaffold.json')
    spec = 'specs/FrSpecs/' + plan['kernel']['module'] + '.lean'
    obligation = plan['properties'][0]['name']
    built = common.command(['lake', 'build'], project / 'specs', output / 'build.json')
    require(built['returncode'] == 0, 'the original diagnostic package must build')
    goals = common.fr(binary, project, ['spec', 'goals', 'specs'], output / 'goals.json')
    require(len(goals['catalog']) == 1 and goals['omitted'] == 0, 'the diagnostic must have one fixed goal')
    for name in common.sources(project):
        (project / name).chmod(0o644)
    common.comparison_grade.BASE.git(project, 'init', '-q', '-b', 'main')
    (project / '.git/info/exclude').write_text('\n'.join(sorted(common.IGNORED - {'.git'})) + '\n')
    common.comparison_grade.BASE.initialize(project)
    prepared = {'task': TASK, 'source': 'src/quotient.rs', 'spec': spec, 'obligation': obligation,
                'target': spec + '::' + obligation, 'original': common.sources(project),
                'protected_proof': proof.masked_proof((project / spec).read_text(), obligation),
                'goal_digest': goals['catalog'][0]['id'], 'property': claim, 'model': plan['kernel']['lean_definition'],
                'assumptions': plan['assumptions'],
                'arithmetic': {'source': 'signed-64 division with a nonnegative dividend and positive divisor',
                               'model': 'Lean Int.tdiv', 'source_implementation_proved': False}}
    common.save(output / 'project.json', prepared)
    common.retain_seed(project, prepared, output)
    return prepared


def prompt(checkers):
    request = json.loads((ROOT / DIRECTORY / 'tasks.json').read_text())['tasks'][TASK]['request']
    return (request + '\n\nFr and the project skill are available. Read the skill before choosing commands. '
            'Tactics are proof steps. The theorem assumes a nonnegative dividend and a positive divisor. '
            'Read passed in fr JSON: a successful process exit can still report passed=false. '
            'Keep .fr/checks.json. Only the fixed theorem proof region may change in the delivered patch. '
            'Use the installed Lean standard library; Mathlib is unavailable. Its source is ' +
            str(checkers['lean']['source_root']) + '. Run the supplied package check: ' +
            checkers['lake']['path'] + ' -d specs build --wfail. '
            'Do not alter the initial draft, source, theorem, assumptions, package configuration or proof markers.')


def initial_check(binary, project, prepared, env, output, seconds):
    result = subprocess.run([str(binary), '-C', str(project), '--json', 'spec', 'proof-check', prepared['target'],
                             '--from', 'initial-tactics.txt', '--token-limit', '4096'],
                            cwd=project, env=env, capture_output=True, text=True, timeout=seconds)
    require(result.returncode == 0, 'initial fr check did not return its JSON report: ' + result.stderr[:2048])
    value = json.loads(result.stdout)
    common.save(output, value)
    require(value['passed'] is False and value['receipt'] is None and value['diagnostics'] and
            any('type' in item['message'].lower() or 'argument' in item['message'].lower()
                for item in value['diagnostics']), 'the fixed bad draft must expose its argument-type error')
    return result.stdout, len(result.stdout.encode()) + len(result.stderr.encode())


def validate_controls(value):
    from . import selected_local as local
    from . import selected_local_grade as public
    require(set(value) == {'passed', 'live_model_calls', 'source_commit', 'run', 'bindings',
            'initial_draft_rejected', 'native_exchanges', 'controls', 'private', 'fr_sha256', 'core_checker_sha256'},
            'diagnostic controls need their exact declared fields')
    expected = {variant: {'status': 'passed', 'accepted': variant == 'reference'} for variant in VARIANTS}
    require(value['passed'] is True and value['live_model_calls'] == 0 and value['controls'] == expected,
            'all seven independent diagnostic patch controls must pass')
    require(value['bindings'] == local.implementation_bindings('M3-feedback') and
            len(value['source_commit']) == 40 and str(value['run']).isdigit(), 'diagnostic checking inputs changed')
    require(value['private']['image'].startswith('sha256:') and len(value['private']['image_archive_sha256']) == 64,
            'freeze the diagnostic checker image and archive')
    require(value['initial_draft_rejected'] is True and value['native_exchanges'] == 4,
            'require the actual initial rejection and all four scripted native exchanges')
    require(value['core_checker_sha256'] == public.core_digest('M3-feedback') and len(value['fr_sha256']) == 64,
            'diagnostic checking code or native executable identity differs')
    return value


def controls(binary, output, native):
    from . import selected_grading as grading
    from . import selected_local as local
    from . import selected_local_grade as public
    require(os.environ.get('GITHUB_ACTIONS') == 'true', 'run independent controls on GitHub')
    output.mkdir(parents=True, exist_ok=False)
    prepared = prepare_inputs(binary, output / 'original', output / 'inputs' / TASK)
    original = output / 'original'
    initial_check(binary, original, prepared, dict(os.environ), output / 'initial-check.json', 120)
    profiles = json.loads((ROOT / DIRECTORY / 'profiles.json').read_text())
    require(native['passed'] is True and native['live_model_calls'] == 0 and native['profiles'] == profiles and
            set(native['exchanges']) == {name + '-' + setup for name in profiles for setup in ('files', 'fr')} and
            all(row['status'] == 'passed' for row in native['exchanges'].values()), 'scripted native exchanges must pass')
    image = grading.build(output / 'image')
    rust = Path(subprocess.check_output(['rustc', '--print', 'sysroot'], text=True).strip())
    result = {'passed': False, 'live_model_calls': 0, 'source_commit': local.identity()['commit'],
              'run': os.environ['GITHUB_RUN_ID'], 'bindings': local.implementation_bindings('M3-feedback'),
              'initial_draft_rejected': True, 'native_exchanges': 4, 'controls': {}, 'private': {'image': image},
              'fr_sha256': common.digest(binary), 'core_checker_sha256': public.core_digest('M3-feedback')}
    common.save(output / 'controls.json', result)
    for variant in VARIANTS:
        candidate = output / ('candidate-' + variant)
        common.checkout(original, candidate)
        if variant in {'reference', 'changed-source', 'missing-patch', 'wrong-patch'}:
            tactics = ROOT / DIRECTORY / 'controls/reference.lean'
            common.fr(binary, candidate, ['spec', 'prove', prepared['target'], '--from', str(tactics), '--write'],
                      output / (variant + '-write.json'))
        if variant == 'changed-source':
            (candidate / prepared['source']).write_text('pub fn quotient(a: i64, b: i64) -> i64 { a + b }\n')
        if variant == 'changed-claim':
            model = candidate / prepared['spec']; text = model.read_text()
            lines = text.splitlines(keepends=True)
            index = next(i for i, line in enumerate(lines) if line.startswith('theorem ' + prepared['obligation'] + ' '))
            lines[index] = 'theorem ' + prepared['obligation'] + ' : True := by\n'
            model.write_text(''.join(lines).replace('  sorry\n', '  trivial\n', 1))
        if variant == 'bad-tactics':
            model = candidate / prepared['spec']
            model.write_text(model.read_text().replace('  sorry\n', '  exact false\n', 1))
        if variant == 'unfinished':
            model = candidate / prepared['spec']
            model.write_text(model.read_text().replace('  sorry\n', '  sorry -- unfinished\n', 1))
        patch = output / (variant + '.patch')
        patch.write_bytes(b'not a patch\n' if variant == 'wrong-patch' else
                          common.comparison_grade.BASE.git(candidate, 'diff', '--binary', 'HEAD').stdout)
        verdict = grading.grade(image, TASK, original, prepared, binary, rust,
                                None if variant == 'missing-patch' else patch, output / ('grade-' + variant),
                                milestone='M3-feedback', tools={})
        require(verdict['passed'] == (variant == 'reference'), 'wrong diagnostic control outcome: ' + variant)
        result['controls'][variant] = {'status': 'passed', 'accepted': verdict['passed']}
        common.save(output / 'controls.json', result)
    archive = output / 'grader-image.tar.gz'
    with archive.open('wb') as stream:
        subprocess.run(['bash', '-o', 'pipefail', '-c', '"$@" | gzip -1', 'feedback-image', *grading.DOCKER, 'save', image],
                       stdout=stream, check=True, timeout=120)
    result['private']['image_archive_sha256'] = common.digest(archive)
    result['passed'] = True
    validate_controls(result)
    common.save(output / 'controls.json', result)
    return result


def decisions(rows):
    from .selected_plan import prepare
    schedule = prepare(ROOT, 'M3-feedback')
    require([row['id'] for row in rows] == [row['id'] for row in schedule['attempts']],
            'retain all eight diagnostic outcomes in their frozen order')
    require(all(all(row[key] == planned[key] for key in ('profile', 'task', 'setup', 'repetition'))
                for row, planned in zip(rows, schedule['attempts'])), 'diagnostic profile or condition changed')
    require(all(type(row['total_seconds']) in {int, float} and math.isfinite(row['total_seconds']) and
                row['total_seconds'] >= 0 for row in rows), 'diagnostic times must be finite nonnegative seconds')
    result = {}
    for profile in schedule['profiles']:
        counts = {}
        usable = True
        for condition in schedule['requirements']['setups']:
            selected = [row for row in rows if row['profile'] == profile and row['setup'] == condition]
            for row in selected:
                require(row['independent_outcome'] in {'passed', 'failed'}, 'independently grade every diagnostic attempt')
                validity = row['feedback_observations']['execution_validity']
                require(validity in {'valid', 'service-failed', 'collection-failed', 'unknown'}, 'review execution validity explicitly')
                usable = usable and validity == 'valid' and not row['collection_error_recorded'] and not row['checking_input_changes']
                usable = usable and row['stop_reason'] in {None, 'wall_seconds'} and row['suspended_seconds'] < 1
                if row['independent_outcome'] == 'passed':
                    require(row['native_status'] == 'completed' and row['stop_reason'] is None and
                            row['complete_attempt_deadline_missed'] is False and row['total_seconds'] <= 150 and
                            row['independent_check']['patch_accepted'] is True,
                            'a timed-out diagnostic remains failed even if its patch passes')
            counts[condition] = sum(row['independent_outcome'] == 'passed' for row in selected)
        key = str(counts['supplied-feedback']) + ',' + str(counts['self-check'])
        rules = schedule['requirements']['decisions']
        decision = rules[key] if key in rules else rules['mixed']
        if not usable:
            decision = 'Repair or clarify experiment execution before interpreting these results'
        result[profile] = {'accepted': counts, 'execution_usable': usable, 'decision': decision}
    return result


def grade_public(data, artifacts, binary, output, control_run):
    from . import selected_grading as grading
    from . import selected_local as local
    from . import selected_local_grade as public
    require(os.environ.get('GITHUB_ACTIONS') == 'true', 'grade diagnostic patches independently on GitHub')
    output.mkdir(parents=True, exist_ok=False)
    supplied = json.loads((data / 'summary.json').read_text())
    rows = public.verify(supplied, 'M3-feedback')
    receipt = validate_controls(json.loads((artifacts / 'controls.json').read_text()))
    require(receipt['run'] == control_run and supplied['controls'] == receipt and common.digest(binary) == receipt['fr_sha256'],
            'use the exact diagnostic controls and native tool frozen before model calls')
    archive = artifacts / 'grader-image.tar.gz'
    require(common.digest(archive) == receipt['private']['image_archive_sha256'], 'checker image archive differs')
    subprocess.run(grading.DOCKER + ['load', '--input', str(archive)], check=True, timeout=60)
    image = receipt['private']['image']
    require(subprocess.check_output(grading.DOCKER + ['image', 'inspect', '--format', '{{.Id}}', image],
                                    text=True, timeout=10).strip() == image, 'loaded diagnostic image differs')
    inputs = artifacts / 'inputs' / TASK
    prepared = json.loads((inputs / 'project.json').read_text())
    seed = json.loads((inputs / 'seed.json').read_text())
    require(hashlib.sha256(encoded(seed['original'])).hexdigest() == supplied['task_source_sha256'][TASK],
            'independent task source differs from the frozen diagnostic')
    rust = Path(subprocess.check_output(['rustc', '--print', 'sysroot'], text=True).strip())
    result = {'passed': False, 'all_8_checked': False, 'checked_attempts': 0,
              'core_checker_sha256': supplied['core_checker_sha256'], 'image': image,
              'control_policy': 'frozen', 'live_model_calls': 0, 'attempts': {}}
    try:
        with tempfile.TemporaryDirectory(prefix='feedback-independent-') as temp:
            original = Path(temp) / 'original'
            local.unpack(inputs / 'original.tar.gz', seed, original)
            for row in rows:
                require(row['native_status'] in {'completed', 'failed'}, 'finish recording every diagnostic slot')
                patch = data / row['patch_file'] if row['patch_file'] is not None else None
                if patch is not None:
                    require(patch.is_file() and not patch.is_symlink() and common.digest(patch) == row['patch_sha256'],
                            'submitted diagnostic patch changed')
                verdict = grading.grade(image, TASK, original, prepared, binary, rust, patch,
                                        output / row['id'], milestone='M3-feedback', tools={})
                accepted = row['native_status'] == 'completed' and verdict['passed']
                result['attempts'][row['id']] = {'status': 'checked', 'native_status': row['native_status'],
                    'patch_sha256': row['patch_sha256'], 'patch_accepted': verdict['passed'],
                    'outcome': 'passed' if accepted else 'failed',
                    'check_stage': verdict['verdict']['stage'],
                    'undo_exact': verdict['verdict']['undo_exact'], 'redo_exact': verdict['verdict']['redo_exact']}
            require(len(rows) == 8, 'independently check all eight slots')
            result.update(passed=True, all_8_checked=True, checked_attempts=len(rows))
    finally:
        common.save(output / 'checked.json', result)
    return result


def markdown(summary):
    result = decisions(summary['attempts'])
    lines = ['# Can the agents use an initial arithmetic error report?', '',
             'Eight assisted repair attempts compare the same small division claim, bad draft, fr and instructions.',
             'Only initial visibility of the actual rejection report differs. Each complete attempt keeps its 150-second limit.',
             'Both conditions include generating the initial check. Failed attempts remain in the totals.', '',
             '| Agent | Finds feedback: accepted / 2 | Supplied feedback: accepted / 2 | Next question |',
             '| --- | ---: | ---: | --- |']
    for profile, row in result.items():
        lines.append(f'| {profile} | {row["accepted"]["self-check"]} | '
                     f'{row["accepted"]["supplied-feedback"]} | {row["decision"]} |')
    lines += ['', 'These two repetitions guide the next diagnostic; they establish no general success rate or definitive cause.',
              'This assisted development example cannot close M3 or replace any page-count failure.',
              'Only submitted patches and summaries are public. Native logs, initial reports, prompts and source snapshots stay local.',
              'Missing observations and actual account charges remain unknown.', '',
              '| Attempt | Outcome | Complete seconds | Patch |', '| --- | --- | ---: | --- |']
    for row in summary['attempts']:
        patch = '[patch](' + row['patch_file'] + ')' if row['patch_file'] else 'none'
        lines.append(f'| {row["id"]} | {row["independent_outcome"]} | {row["total_seconds"]:.3f} | {patch} |')
    return '\n'.join(lines) + '\n'


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='operation', required=True)
    preparing = commands.add_parser('controls')
    for name in ('fr', 'output', 'native'):
        preparing.add_argument('--' + name, type=Path, required=True)
    grading = commands.add_parser('grade')
    for name in ('data', 'artifacts', 'fr', 'output'):
        grading.add_argument('--' + name, type=Path, required=True)
    grading.add_argument('--control-run', required=True)
    reporting = commands.add_parser('report')
    reporting.add_argument('--data', type=Path, required=True)
    args = parser.parse_args()
    if args.operation == 'controls':
        print(json.dumps(controls(args.fr.resolve(), args.output.resolve(), json.loads(args.native.read_text()))))
    elif args.operation == 'grade':
        print(json.dumps(grade_public(args.data.resolve(), args.artifacts.resolve(), args.fr.resolve(), args.output.resolve(), args.control_run)))
    else:
        (args.data / 'README.md').write_text(markdown(json.loads((args.data / 'summary.json').read_text())))
