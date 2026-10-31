"""Check small proof repairs and construction without replacing the full M3 study."""
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
from .selected_plan import CASE_DIRECTORIES, ROOT, encoded, prepare, require

DIRECTORY = Path('tests/agent-eval/proof-feedback')
TASK = 'division-repair'
VARIANTS = ('reference', 'unfinished', 'bad-tactics', 'changed-claim', 'changed-source', 'missing-patch', 'wrong-patch')
check = proof.check


def case_details(milestone):
    require(milestone in {'M3-feedback', 'M3-construction'}, 'select the explicit small proof study')
    if milestone == 'M3-feedback':
        return {'directory': DIRECTORY, 'task': TASK, 'source': 'src/quotient.rs',
                'fixture': 'fixtures/quotient.rs', 'property': 'nonnegative', 'draft': 'initial-tactics.txt',
                'arithmetic': {'source': 'signed-64 division with a nonnegative dividend and positive divisor',
                               'model': 'Lean Int.tdiv', 'source_implementation_proved': False}}
    settings = json.loads((ROOT / CASE_DIRECTORIES[milestone] / 'requirements.json').read_text())
    require(settings['source'] == 'fixtures/scaled.rs' and settings['project_source'] == 'src/scaled.rs' and
            settings['property_name'] == 'at_least_input' and
            settings['proof_package'] == 'specs' and settings['tactics_file'] == '.fr/proof-tactics' and
            settings['starting_proof'] == 'empty' and settings['initial_error_report'] is False,
            'keep the fixed empty construction task')
    return {'directory': CASE_DIRECTORIES[milestone], 'task': 'scaled-total', 'source': settings['project_source'],
            'fixture': settings['source'],
            'property': settings['property_name'], 'draft': None,
            'arithmetic': {'source': 'signed-64 multiplication; overflow is not established',
                           'model': 'Lean unbounded Int multiplication', 'source_implementation_proved': False}}


def property_for(task, milestone):
    details = case_details(milestone)
    a, b = proof.variable('a'), proof.variable('b')
    zero, one = proof.integer(0, 'Int'), proof.integer(1, 'Int')
    assumptions = proof.conjunction(proof.relation('less-or-equal', zero, a), proof.relation('less-or-equal', one, b))
    conclusion = proof.relation('less-or-equal', zero if milestone == 'M3-feedback' else a, proof.model(a, b))
    return {'schema': 'fr-formal-property-1', 'task_digest': task['object_digest'], 'name': details['property'],
            'parameters': [{'name': name, 'lean_type': 'Int'} for name in ('a', 'b')],
            'proposition': proof.implies(assumptions, conclusion)}


def prepare_inputs(binary, project, output, milestone):
    details = case_details(milestone)
    require(not project.exists(), 'prepare the diagnostic in a new project')
    project.mkdir(parents=True)
    output.mkdir(parents=True, exist_ok=False)
    (project / 'src').mkdir()
    (project / details['source']).write_bytes((ROOT / details['directory'] / details['fixture']).read_bytes())
    if details['draft'] is not None:
        (project / details['draft']).write_bytes((ROOT / details['directory'] / 'fixtures' / details['draft']).read_bytes())
    (project / 'Cargo.toml').write_text('[workspace]\n')
    common.fr(binary, project, ['spec', 'init', 'specs', '--write'], output / 'init.json')
    symbol = 'quotient' if milestone == 'M3-feedback' else 'scaled_total'
    shape = common.fr(binary, project, ['spec', 'property-task', details['source'] + '::' + symbol], output / 'shape.json')
    claim = property_for(shape, milestone)
    common.save(output / 'property.json', claim)
    plan = common.fr(binary, project, ['spec', 'plan', details['source'] + '::' + symbol, '--property-from',
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
    prepared = {'task': details['task'], 'source': details['source'], 'spec': spec, 'obligation': obligation,
                'target': spec + '::' + obligation, 'original': common.sources(project),
                'protected_proof': proof.masked_proof((project / spec).read_text(), obligation),
                'goal_digest': goals['catalog'][0]['id'], 'property': claim, 'model': plan['kernel']['lean_definition'],
                'assumptions': plan['assumptions'],
                'arithmetic': details['arithmetic']}
    common.save(output / 'project.json', prepared)
    common.retain_seed(project, prepared, output)
    return prepared


def prompt(checkers, milestone):
    if milestone == 'M3-construction':
        request = prepare(ROOT, milestone)['requirements']['tasks']['scaled-total']['request']
        return (request + '\n\nTactics are proof steps. Read the fixed claim before writing a draft. '
                'Use the installed Lean standard library; Mathlib is unavailable. Its source is ' +
                str(checkers['lean']['source_root']) + '. A successful fr command can report passed=false; read the report. '
                'Only the marked proof region may change. Preserve .fr/checks.json; store reports elsewhere.')
    require(milestone == 'M3-feedback', 'choose the explicit prompt study')
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


def validate_controls(value, milestone):
    from . import selected_local as local
    from . import selected_local_grade as public
    rejection = 'initial_draft_rejected' if milestone == 'M3-feedback' else 'unfinished_draft_rejected'
    require(value['study'] == milestone and set(value) == {'passed', 'live_model_calls', 'source_commit', 'run', 'bindings',
            'study', rejection, 'native_exchanges', 'controls', 'private', 'fr_sha256', 'core_checker_sha256'},
            'diagnostic controls need their exact declared fields')
    expected = {variant: {'status': 'passed', 'accepted': variant == 'reference'} for variant in VARIANTS}
    require(value['passed'] is True and value['live_model_calls'] == 0 and value['controls'] == expected,
            'all seven independent diagnostic patch controls must pass')
    require(value['bindings'] == local.implementation_bindings(milestone) and
            len(value['source_commit']) == 40 and str(value['run']).isdigit(), 'diagnostic checking inputs changed')
    require(value['private']['image'].startswith('sha256:') and len(value['private']['image_archive_sha256']) == 64,
            'freeze the diagnostic checker image and archive')
    require(value[rejection] is True and value['native_exchanges'] == 4,
            'require the actual initial rejection and all four scripted native exchanges')
    require(value['core_checker_sha256'] == public.core_digest(milestone) and len(value['fr_sha256']) == 64,
            'diagnostic checking code or native executable identity differs')
    return value


def controls(binary, output, native, milestone):
    from . import selected_grading as grading
    from . import selected_local as local
    from . import selected_local_grade as public
    require(os.environ.get('GITHUB_ACTIONS') == 'true', 'run independent controls on GitHub')
    output.mkdir(parents=True, exist_ok=False)
    details = case_details(milestone)
    task = details['task']
    prepared = prepare_inputs(binary, output / 'original', output / 'inputs' / task, milestone)
    original = output / 'original'
    if milestone == 'M3-feedback':
        initial_check(binary, original, prepared, dict(os.environ), output / 'initial-check.json', 120)
    else:
        unfinished = output / 'unfinished.lean'; unfinished.write_text('refine ?_\n')
        rejected = common.fr(binary, original, ['spec', 'proof-check', prepared['target'], '--from',
                             str(unfinished.resolve()), '--token-limit', '4096'], output / 'initial-check.json')
        require(rejected['passed'] is False and rejected['receipt'] is None, 'the empty construction proof must fail')
    profiles = prepare(ROOT, milestone)['profiles']
    require(native['passed'] is True and native['live_model_calls'] == 0 and native['profiles'] == profiles and
            set(native['exchanges']) == {name + '-' + setup for name in profiles for setup in ('files', 'fr')} and
            all(row['status'] == 'passed' for row in native['exchanges'].values()), 'scripted native exchanges must pass')
    image = grading.build(output / 'image')
    rust = Path(subprocess.check_output(['rustc', '--print', 'sysroot'], text=True).strip())
    rejection = 'initial_draft_rejected' if milestone == 'M3-feedback' else 'unfinished_draft_rejected'
    result = {'passed': False, 'live_model_calls': 0, 'source_commit': local.identity()['commit'], 'study': milestone,
              'run': os.environ['GITHUB_RUN_ID'], 'bindings': local.implementation_bindings(milestone),
              rejection: True, 'native_exchanges': 4, 'controls': {}, 'private': {'image': image},
              'fr_sha256': common.digest(binary), 'core_checker_sha256': public.core_digest(milestone)}
    common.save(output / 'controls.json', result)
    for variant in VARIANTS:
        candidate = output / ('candidate-' + variant)
        common.checkout(original, candidate)
        if variant in {'reference', 'changed-source', 'missing-patch', 'wrong-patch'}:
            tactics = ROOT / details['directory'] / 'controls/reference.lean'
            common.fr(binary, candidate, ['spec', 'prove', prepared['target'], '--from', str(tactics), '--write'],
                      output / (variant + '-write.json'))
        if variant == 'changed-source':
            source = candidate / prepared['source']
            operator = ' / ' if milestone == 'M3-feedback' else ' * '
            require(source.read_text().count(operator) == 1, 'change exactly the source arithmetic operator')
            source.write_text(source.read_text().replace(operator, ' + ', 1))
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
        verdict = grading.grade(image, task, original, prepared, binary, rust,
                                None if variant == 'missing-patch' else patch, output / ('grade-' + variant),
                                milestone=milestone, tools={})
        require(verdict['passed'] == (variant == 'reference'), 'wrong diagnostic control outcome: ' + variant)
        result['controls'][variant] = {'status': 'passed', 'accepted': verdict['passed']}
        common.save(output / 'controls.json', result)
    archive = output / 'grader-image.tar.gz'
    with archive.open('wb') as stream:
        subprocess.run(['bash', '-o', 'pipefail', '-c', '"$@" | gzip -1', 'feedback-image', *grading.DOCKER, 'save', image],
                       stdout=stream, check=True, timeout=120)
    result['private']['image_archive_sha256'] = common.digest(archive)
    result['passed'] = True
    validate_controls(result, milestone)
    common.save(output / 'controls.json', result)
    return result


def outcomes(rows, milestone):
    schedule = prepare(ROOT, milestone)
    require([row['id'] for row in rows] == [row['id'] for row in schedule['attempts']],
            'retain every diagnostic outcome in its frozen order')
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
                observations = 'feedback_observations' if milestone == 'M3-feedback' else 'proof_observations'
                validity = row[observations]['execution_validity']
                require(validity in {'valid', 'service-failed', 'collection-failed', 'unknown'}, 'review execution validity explicitly')
                usable = usable and validity == 'valid' and not row['collection_error_recorded'] and not row['checking_input_changes']
                usable = usable and row['stop_reason'] in {None, 'wall_seconds'} and row['suspended_seconds'] < 1
                if row['independent_outcome'] == 'passed':
                    require(row['native_status'] == 'completed' and row['stop_reason'] is None and
                            row['complete_attempt_deadline_missed'] is False and row['total_seconds'] <= 150 and
                            not row['checking_input_changes'] and not row['collection_error_recorded'] and
                            row['independent_check']['patch_accepted'] is True,
                            'a timed-out diagnostic remains failed even if its patch passes')
                    if milestone == 'M3-construction':
                        require(construction_checks_passed(row), 'unobserved construction checks remain failed')
            counts[condition] = sum(row['independent_outcome'] == 'passed' for row in selected)
        result[profile] = {'accepted': counts, 'execution_usable': usable}
    return result


def decisions(rows):
    result = outcomes(rows, 'M3-feedback')
    schedule = prepare(ROOT, 'M3-feedback')
    for profile, row in result.items():
        counts = row['accepted']
        key = str(counts['supplied-feedback']) + ',' + str(counts['self-check'])
        rules = schedule['requirements']['decisions']
        decision = rules[key] if key in rules else rules['mixed']
        if not row['execution_usable']:
            decision = 'Repair or clarify experiment execution before interpreting these results'
        row['decision'] = decision
    return result


def construction_checks_passed(row):
    return all(row['proof_observations'][key] is True for key in
               ('proof_check_passed', 'proof_written_with_fr', 'package_check_passed'))


def construction_decision(rows):
    result = outcomes(rows, 'M3-construction')
    ready = all(row['execution_usable'] and row['accepted']['fr'] == 2 for row in result.values())
    requirements = prepare(ROOT, 'M3-construction')['requirements']
    return {'profiles': result, 'all_four_completed': ready,
            'decision': requirements['decision_if_all_four_pass' if ready else 'decision_if_any_fail']}


def grade_public(data, artifacts, binary, output, control_run, milestone):
    from . import selected_grading as grading
    from . import selected_local as local
    from . import selected_local_grade as public
    require(os.environ.get('GITHUB_ACTIONS') == 'true', 'grade diagnostic patches independently on GitHub')
    output.mkdir(parents=True, exist_ok=False)
    supplied = json.loads((data / 'summary.json').read_text())
    task = case_details(milestone)['task']
    rows = public.verify(supplied, milestone)
    receipt = validate_controls(json.loads((artifacts / 'controls.json').read_text()), milestone)
    require(receipt['run'] == control_run and supplied['controls'] == receipt and common.digest(binary) == receipt['fr_sha256'],
            'use the exact diagnostic controls and native tool frozen before model calls')
    archive = artifacts / 'grader-image.tar.gz'
    require(common.digest(archive) == receipt['private']['image_archive_sha256'], 'checker image archive differs')
    subprocess.run(grading.DOCKER + ['load', '--input', str(archive)], check=True, timeout=60)
    image = receipt['private']['image']
    require(subprocess.check_output(grading.DOCKER + ['image', 'inspect', '--format', '{{.Id}}', image],
                                    text=True, timeout=10).strip() == image, 'loaded diagnostic image differs')
    inputs = artifacts / 'inputs' / task
    prepared = json.loads((inputs / 'project.json').read_text())
    seed = json.loads((inputs / 'seed.json').read_text())
    require(hashlib.sha256(encoded(seed['original'])).hexdigest() == supplied['task_source_sha256'][task],
            'independent task source differs from the frozen diagnostic')
    rust = Path(subprocess.check_output(['rustc', '--print', 'sysroot'], text=True).strip())
    count = len(prepare(ROOT, milestone)['attempts'])
    result = {'passed': False, f'all_{count}_checked': False, 'checked_attempts': 0,
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
                verdict = grading.grade(image, task, original, prepared, binary, rust, patch,
                                        output / row['id'], milestone=milestone, tools={})
                accepted = row['native_status'] == 'completed' and verdict['passed']
                if milestone == 'M3-construction':
                    accepted = accepted and construction_checks_passed(row)
                result['attempts'][row['id']] = {'status': 'checked', 'native_status': row['native_status'],
                    'patch_sha256': row['patch_sha256'], 'patch_accepted': verdict['passed'],
                    'outcome': 'passed' if accepted else 'failed',
                    'check_stage': verdict['verdict']['stage'],
                    'undo_exact': verdict['verdict']['undo_exact'], 'redo_exact': verdict['verdict']['redo_exact']}
            require(len(rows) == count, 'independently check every original slot')
            result.update(passed=True, checked_attempts=len(rows))
            result[f'all_{count}_checked'] = True
    finally:
        common.save(output / 'checked.json', result)
    return result


def markdown(summary):
    if summary['study'] == 'M3-construction':
        result = construction_decision(summary['attempts'])
        lines = ['# Can the agents construct the multiplication proof and finish?', '',
                 'Four fresh development attempts start with an unfinished theorem and the same corrected instructions.',
                 'Each complete attempt keeps its 150-second limit and unchanged check settings.',
                 'This single-condition check has no timing baseline or causal improvement claim.', '',
                 result['decision'], '', '| Agent | Complete accepted attempts / 2 |', '| --- | ---: |']
        for profile, row in result['profiles'].items():
            lines.append(f'| {profile} | {row["accepted"]["fr"]} |')
        lines += ['', 'Only the fixed mathematical model is proved; source overflow and equivalence remain open.',
                  'Only submitted patches and summaries are public. Actual charges and unavailable usage stay unknown.', '',
                  '| Attempt | Outcome | Complete seconds | Patch |', '| --- | --- | ---: | --- |']
        for row in summary['attempts']:
            patch = '[patch](' + row['patch_file'] + ')' if row['patch_file'] else 'none'
            lines.append(f'| {row["id"]} | {row["independent_outcome"]} | {row["total_seconds"]:.3f} | {patch} |')
        return '\n'.join(lines) + '\n'
    require(summary['study'] == 'M3-feedback', 'choose the explicit diagnostic report')
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
    for command in (preparing, grading, reporting):
        command.add_argument('--study', choices=('M3-feedback', 'M3-construction'), required=True)
    args = parser.parse_args()
    if args.operation == 'controls':
        print(json.dumps(controls(args.fr.resolve(), args.output.resolve(), json.loads(args.native.read_text()), args.study)))
    elif args.operation == 'grade':
        print(json.dumps(grade_public(args.data.resolve(), args.artifacts.resolve(), args.fr.resolve(), args.output.resolve(), args.control_run, args.study)))
    else:
        summary = json.loads((args.data / 'summary.json').read_text())
        require(summary['study'] == args.study, 'requested report differs from the recorded study')
        (args.data / 'README.md').write_text(markdown(summary))
