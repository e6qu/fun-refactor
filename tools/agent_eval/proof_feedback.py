"""Check small proof repairs and construction without replacing the full M3 study."""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import shlex
import shutil
import re
import subprocess
import tempfile
import tomllib

from . import proof_writing_tasks as proof
from . import selected_tasks as common
from .selected_plan import PROOF_DIAGNOSTICS, REPAIR_STUDY, CASE_DIRECTORIES, ROOT, encoded, prepare, require

DIRECTORY = Path('tests/agent-eval/proof-feedback')
TASK = 'division-repair'
VARIANTS = ('reference', 'unfinished', 'bad-tactics', 'changed-claim', 'changed-source', 'missing-patch', 'wrong-patch')
check = proof.check


def case_details(milestone):
    require(milestone in PROOF_DIAGNOSTICS, 'select the explicit small proof study')
    if milestone in {'M3-page-count', REPAIR_STUDY}:
        entry = proof.CASES['page-count']
        return {'directory': CASE_DIRECTORIES[milestone], 'task': 'page-count', 'source': entry['path'],
                'fixture': None, 'property': entry['property'], 'draft': None,
                'arithmetic': {'source': 'signed-64', 'model': 'Lean mathematical Int with truncating division',
                               'source_implementation_proved': False}}
    if milestone == 'M3-feedback':
        return {'directory': DIRECTORY, 'task': TASK, 'source': 'src/quotient.rs',
                'fixture': 'fixtures/quotient.rs', 'property': 'nonnegative', 'draft': 'initial-tactics.txt',
                'arithmetic': {'source': 'signed-64 division with a nonnegative dividend and positive divisor',
                               'model': 'Lean Int.tdiv', 'source_implementation_proved': False}}
    if milestone == 'M3-facts':
        return {'directory': CASE_DIRECTORIES[milestone], 'task': 'division-facts', 'source': 'src/parts.rs',
                'fixture': 'fixtures/parts.rs', 'property': 'at_most_input', 'draft': None,
                'arithmetic': {'source': 'signed-64 division and remainder; overflow is not established',
                               'model': 'Lean Int.tdiv and Int.tmod', 'source_implementation_proved': False}}
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
    if milestone in {'M3-page-count', REPAIR_STUDY}:
        return proof.property_for('page-count', task)
    details = case_details(milestone)
    a, b = proof.variable('a'), proof.variable('b')
    zero, one = proof.integer(0, 'Int'), proof.integer(1, 'Int')
    assumptions = proof.conjunction(proof.relation('less-or-equal', zero, a), proof.relation('less-or-equal', one, b))
    conclusion = (proof.relation('less-or-equal', proof.model(a, b), a) if milestone == 'M3-facts' else
                  proof.relation('less-or-equal', zero if milestone == 'M3-feedback' else a, proof.model(a, b)))
    return {'schema': 'fr-formal-property-1', 'task_digest': task['object_digest'], 'name': details['property'],
            'parameters': [{'name': name, 'lean_type': 'Int'} for name in ('a', 'b')],
            'proposition': proof.implies(assumptions, conclusion)}


def prepare_inputs(binary, project, output, milestone):
    if milestone in {'M3-page-count', REPAIR_STUDY}:
        return proof.prepare('page-count', binary, project, output)
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
    symbol = {'M3-feedback': 'quotient', 'M3-construction': 'scaled_total', 'M3-facts': 'division_parts'}[milestone]
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
    goals = common.fr(binary, project, ['proof', 'list', 'specs', '--bytes', '4096', '--limit', '32'], output / 'goals.json')
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
    if milestone == 'M3-facts':
        settings = prepare(ROOT, milestone)['requirements']
        arguments = tomllib.loads((project / 'specs/lakefile.toml').read_text())['lean_lib'][0]['moreLeanArgs']
        require(arguments == settings['lean_arguments'] == ['-j1'] and
                (project / spec).read_text().startswith('prelude\nimport Init.Data.Int.DivMod.Lemmas\n') and
                (project / 'specs/FrSpecs.lean').read_text().startswith('prelude\n') and
                settings['lean_imports'] == ['Init.Data.Int.DivMod.Lemmas'],
                'qualify the generated division imports and one compiler worker without setup rewrites')
        prepared.update(lean_arguments=arguments, lean_imports=settings['lean_imports'])
    common.save(output / 'project.json', prepared)
    common.retain_seed(project, prepared, output)
    return prepared


def prompt(checkers, milestone):
    if milestone in {'M3-page-count', REPAIR_STUDY}:
        request = prepare(ROOT, milestone)['requirements']['tasks']['page-count']['request']
        return (request + '\n\nTactics are proof steps. Read all parts of the fixed claim before writing a draft. '
                'Use the installed Lean standard library; Mathlib is unavailable. Its source is ' +
                str(checkers['lean']['source_root']) + '. A rejected fr proof check exits 1 with passed=false; read the report. '
                'Only the marked proof region may change. Preserve .fr/checks.json; store reports elsewhere.')
    if milestone in {'M3-construction', 'M3-facts'}:
        request = prepare(ROOT, milestone)['requirements']['tasks'][case_details(milestone)['task']]['request']
        library = str(checkers['lean']['source_root'])
        if milestone == 'M3-facts':
            library = ('the imported division module ' +
                       str(Path(library) / 'Init/Data/Int/DivMod/Lemmas.lean') +
                       '. Use that file as fr -C when finding declarations. '
                       'Do not index the whole Lean source tree; --limit bounds results, not input work')
        return (request + '\n\nTactics are proof steps. Read the fixed claim before writing a draft. '
                'Use the installed Lean standard library; Mathlib is unavailable. Its source is ' +
                library + '. A rejected fr proof check exits 1 with passed=false; read the report. '
                'Only the marked proof region may change. Preserve .fr/checks.json; store reports elsewhere.')
    require(milestone == 'M3-feedback', 'choose the explicit prompt study')
    request = json.loads((ROOT / DIRECTORY / 'tasks.json').read_text())['tasks'][TASK]['request']
    return (request + '\n\nFr and the project skill are available. Read the skill before choosing commands. '
            'Tactics are proof steps. The theorem assumes a nonnegative dividend and a positive divisor. '
            'A rejected fr proof check exits 1 with passed=false; read its diagnostics. '
            'Keep .fr/checks.json. Only the fixed theorem proof region may change in the delivered patch. '
            'Use the installed Lean standard library; Mathlib is unavailable. Its source is ' +
            str(checkers['lean']['source_root']) + '. Run the supplied package check: ' +
            checkers['lake']['path'] + ' -d specs build --wfail. '
            'Do not alter the initial draft, source, theorem, assumptions, package configuration or proof markers.')


def install_declarations(project, condition):
    require(condition in {'select-facts', 'supplied-declarations'}, 'choose the declaration condition explicitly')
    path = project / '.fr/declarations.txt'
    require(not path.exists(), 'do not overwrite a supplied declaration handout')
    path.parent.mkdir(exist_ok=True)
    content = ((ROOT / CASE_DIRECTORIES['M3-facts'] / 'fixtures/declarations.txt').read_text()
               if condition == 'supplied-declarations' else
               'Select the needed current declarations from the installed Lean standard library.\n')
    path.write_text(content)
    return path


def install_repair_instruction(project, condition):
    require(condition in {'shared-skill', 'argument-repair'}, 'choose the argument repair condition explicitly')
    path = project / '.fr/argument-repair.txt'
    require(not path.exists(), 'do not overwrite a supplied repair instruction')
    path.parent.mkdir(exist_ok=True)
    path.write_text((ROOT / CASE_DIRECTORIES[REPAIR_STUDY] / 'fixtures/argument-repair.txt').read_text()
                    if condition == 'argument-repair' else 'Use the shared fr skill to write and check the proof.\n')
    return path


UNSOLVED = 'assert any("unsolved goals" in d["message"].lower() for d in r["diagnostics"])'
DIAGNOSTIC_ADMISSIONS = {
    REPAIR_STUDY: {'handout': '.fr/argument-repair.txt', 'library_lookup': False, 'delivery': 'package-diff',
                   'rejection': 'assert any(d["draft_location"] and "Int" in str(d["context"]) '
                                'and "0 ≤" in str(d["context"]) for d in r["diagnostics"])'},
    'M3-facts': {'handout': '.fr/declarations.txt', 'library_lookup': True, 'delivery': 'package-diff',
                 'rejection': UNSOLVED},
    'M3-page-count': {'handout': None, 'library_lookup': False, 'delivery': 'deliver', 'rejection': UNSOLVED},
}


def admission_drafts(study):
    """The reference and rejected drafts given only to scripted admission controls."""
    if study in {REPAIR_STUDY, 'M3-page-count'}:
        from .page_count_diagnosis import drafts, REFERENCE
        reference = REFERENCE.read_text()
        return reference, (drafts(reference)['integer-instead-of-proof'] if study == REPAIR_STUDY else 'refine ?_\n')
    return (ROOT / CASE_DIRECTORIES[study] / 'controls/reference.lean').read_text(), 'refine ?_\n'


def install_diagnostic_admission(project, condition, lean_root, prepared, study):
    """Scripted controls receive reference drafts; live attempts never receive them."""
    from . import selected_local as local
    require(study in DIAGNOSTIC_ADMISSIONS, 'choose the complete diagnostic admission explicitly')
    shutil.copytree(ROOT / 'skills/fr', project / '.agents/skills/fr')
    handout = {REPAIR_STUDY: install_repair_instruction, 'M3-facts': install_declarations}.get(study)
    handouts = [handout(project, condition)] if handout else []
    reference, bad = admission_drafts(study)
    directory = project / '.fr'
    directory.mkdir(exist_ok=True)
    (directory / 'admission-target.txt').write_text(prepared['target'])
    (directory / 'admission-reference.lean').write_text(reference)
    (directory / 'admission-bad.lean').write_text(bad)
    common.save(directory / 'checks.json', {'schema': 1, 'checks': [{'name': 'lean',
        'argv': [str(lean_root / 'bin/lake'), '-d', 'specs', 'build', '--wfail'], 'cwd': '.',
        'timeout_seconds': 120, 'covers': ['fixed Lean theorem without proof gaps']}]})
    paths = [*handouts, directory / 'checks.json', *(project / '.agents/skills/fr').rglob('*')]
    return {str(path.relative_to(project)): {'sha256': local.hash_file(path), 'mode': path.stat().st_mode & 0o7777}
            for path in paths if path.is_file()}


def diagnostic_admission_script(lean_root, search_path, condition, study):
    require(study in DIAGNOSTIC_ADMISSIONS and
            condition in prepare(ROOT, study)['requirements']['setups'], 'admit the explicit diagnostic condition')
    from .algorithm_admission import MARKER
    spec = DIAGNOSTIC_ADMISSIONS[study]
    lines = ['set -eu', 'export PATH=' + shlex.quote(search_path),
             'cat ' + ' '.join(['.agents/skills/fr/SKILL.md', *([spec['handout']] if spec['handout'] else [])]) + ' > /dev/null',
             *([shlex.join(['fr', '--json', '-C', str(lean_root / 'src/lean/Init/Data/Int/DivMod/Lemmas.lean'),
                           'project', 'find', 'tdiv', '--contains', '--signature', '--limit', '4']) +
                ' > .fr/admission-library.json',
                shlex.join(['python3', '-c', 'import json; r=json.load(open(".fr/admission-library.json")); '
                    'assert r["coverage"]["indexed_files"] == 1 and r["rows"]; '
                    'assert "signature" in r["columns"]'])] if spec['library_lookup'] else []),
             'proof_target=$(cat .fr/admission-target.txt)',
             'fr --json proof start "$proof_target" --draft .fr/admission-start.lean --bytes 4096 --timeout-seconds 30 > .fr/admission-start.json',
             'fr --json proof state "$proof_target" --draft .fr/admission-start.lean --at end --bytes 4096 --timeout-seconds 30 > .fr/admission-state.json',
             shlex.join(['python3', '-c', 'import json; r=json.load(open(".fr/admission-state.json")); '
                 'assert r["goals"] and r["acceptance"] == "not-checked"; '
                 'assert all(isinstance(g["context"], str) and isinstance(g["goal"], str) for g in r["goals"])']),
             'if fr --json proof check "$proof_target" --draft .fr/admission-bad.lean --bytes 4096 --timeout-seconds 30 > .fr/admission-rejected.json; then exit 1; else test $? = 1; fi',
             shlex.join(['python3', '-c', 'import json; r=json.load(open(".fr/admission-rejected.json")); '
                'assert r["passed"] is False and r["receipt"] is None and r["diagnostics"]; ' + spec['rejection']])]
    if spec['delivery'] == 'deliver':
        # The live request asks for exactly this call: check, save, package check and patch together.
        lines += ['fr --json proof check "$proof_target" --draft .fr/admission-reference.lean --bytes 4096 '
                  '--timeout-seconds 30 > .fr/admission-check.json',
                  shlex.join(['python3', '-c', 'import json; assert json.load(open(".fr/admission-check.json"))["passed"] is True']),
                  'fr --json proof save "$proof_target" --draft .fr/admission-reference.lean --bytes 4096 '
                  '--timeout-seconds 30 --deliver change.patch > .fr/admission-save.json',
                  shlex.join(['python3', '-c', 'import json; r=json.load(open(".fr/admission-save.json")); '
                      'assert r["status"] == "written" and r["proof_checked"] is True; '
                      'd=r["delivery"]; assert d["delivered"] is True and d["checks"]["passed"] is True; '
                      'assert d["transactions"] == [r["transaction"]]'])]
    else:
        for operation, field in [('check', 'passed'), ('save', 'applied')]:
            report = '.fr/admission-' + operation + '.json'
            lines += ['fr --json proof ' + operation + ' "$proof_target" --draft .fr/admission-reference.lean ' +
                      '--bytes 4096 --timeout-seconds 30' + ' > ' + report,
                      shlex.join(['python3', '-c', 'import json; assert json.load(open(' + repr(report) + '))[' + repr(field) + '] is True'])]
        lines += [shlex.join([str(lean_root / 'bin/lake'), '-d', 'specs', 'build', '--wfail']) +
                  ' > .fr/admission-package.log 2>&1', 'git diff --binary HEAD > change.patch']
    lines += ['test -s change.patch', 'printf ' + shlex.quote(MARKER + '\n')]
    return '\n'.join(lines) + '\n'


def qualify_declarations(output):
    from . import selected_local as local
    expected = ROOT / CASE_DIRECTORIES['M3-facts'] / 'fixtures/declarations.txt'
    names = [line.split()[0] for line in expected.read_text().splitlines()]
    require(names == ['Int.mul_le_mul_of_nonneg_right', 'Int.tdiv_nonneg',
                      'Int.tmod_add_mul_tdiv', 'Int.tmod_nonneg'], 'keep the four alphabetically listed declarations')
    probe = output / 'Declarations.lean'
    probe.write_text('import Init\n' + ''.join('#check ' + name + '\n' for name in names))
    result = common.command(['lean', str(probe.resolve())], output, output / 'declarations-check.json')
    require(result['returncode'] == 0 and not result['stderr'].strip() and
            ' '.join(result['stdout'].split()) == ' '.join(expected.read_text().split()),
            'supplied types must match the pinned Lean declarations exactly')
    for condition in ('select-facts', 'supplied-declarations'):
        project = output / ('handout-' + condition);project.mkdir()
        handout = install_declarations(project, condition)
        protected = {'.fr/declarations.txt': common.digest(handout)}
        require(not local.checking_input_changes(project, protected), 'clean handout was altered')
        handout.write_text('changed\n')
        require(local.checking_input_changes(project, protected) == ['.fr/declarations.txt'], 'reject changed handouts')
        handout.unlink()
        require(local.checking_input_changes(project, protected) == ['.fr/declarations.txt'], 'reject deleted handouts')
    return common.digest(expected)


def initial_check(binary, project, prepared, env, output, seconds):
    result = subprocess.run([str(binary), '-C', str(project), '--json', 'proof', 'check', prepared['target'], '--draft', 'initial-tactics.txt', '--bytes', '4096', '--timeout-seconds', '30'],
                            cwd=project, env=env, capture_output=True, text=True, timeout=seconds)
    require(result.returncode == 1, 'initial fr check must return a rejected proof with exit 1: ' + result.stderr[:2048])
    value = json.loads(result.stdout)
    common.save(output, value)
    require(value['passed'] is False and value['receipt'] is None and value['diagnostics'] and
            any('type' in item['message'].lower() or 'argument' in item['message'].lower()
                for item in value['diagnostics']), 'the fixed bad draft must expose its argument-type error')
    return result.stdout, len(result.stdout.encode()) + len(result.stderr.encode())


def unexecuted_prior_attempts():
    """Count earlier attempts of this run that GitHub held for approval and never ran.

    Approving a held pull-request run starts attempt 2, so the attempt number alone
    cannot tell an approval from a rerun. Any earlier attempt that ran a job is a rerun.
    """
    attempt = int(os.environ['GITHUB_RUN_ATTEMPT'])
    run = 'repos/%s/actions/runs/%s/attempts/' % (os.environ['GITHUB_REPOSITORY'], os.environ['GITHUB_RUN_ID'])
    for earlier in range(1, attempt):
        summary = json.loads(subprocess.check_output(['gh', 'api', run + str(earlier)], text=True))
        jobs = json.loads(subprocess.check_output(['gh', 'api', run + str(earlier) + '/jobs'], text=True))
        require(summary['conclusion'] == 'action_required' and jobs['total_count'] == 0,
                'controls rerun after attempt %d ran; start a new run instead' % earlier)
    return attempt - 1


def first_executed_attempt(value):
    held = value.get('unexecuted_prior_attempts', 0)
    return (type(value['run_attempt']) is int and type(held) is int and held >= 0
            and value['run_attempt'] == held + 1)


def validate_controls(value, milestone):
    from . import selected_local as local
    from . import selected_local_grade as public
    rejection = 'initial_draft_rejected' if milestone == 'M3-feedback' else 'unfinished_draft_rejected'
    fields = {'passed', 'live_model_calls', 'source_commit', 'run', 'bindings',
            'study', rejection, 'native_exchanges', 'controls', 'private', 'fr_sha256', 'core_checker_sha256'}
    if milestone == 'M3-facts':
        fields |= {'declarations_sha256', 'handout_changes_rejected', 'lean_arguments', 'lean_imports', 'run_attempt'}
    if milestone == REPAIR_STUDY:
        fields |= {'repair_instruction_sha256', 'handout_changes_rejected', 'argument_type_error_rejected',
                   'lean_arguments', 'lean_imports', 'run_attempt'}
    require(value['study'] == milestone and set(value) - {'unexecuted_prior_attempts'} == fields,
            'diagnostic controls need their exact declared fields')
    require('unexecuted_prior_attempts' not in value or milestone in {'M3-facts', REPAIR_STUDY},
            'diagnostic controls need their exact declared fields')
    if milestone == 'M3-facts':
        require(value['declarations_sha256'] == common.digest(ROOT / CASE_DIRECTORIES[milestone] / 'fixtures/declarations.txt')
                and value['handout_changes_rejected'] is True and value['lean_arguments'] == ['-j1'] and
                value['lean_imports'] == ['Init.Data.Int.DivMod.Lemmas'] and
                first_executed_attempt(value),
                'qualify the exact declarations, protected handout and explicit compiler settings')
    if milestone == REPAIR_STUDY:
        require(value['repair_instruction_sha256'] == common.digest(ROOT / CASE_DIRECTORIES[milestone] /
                'fixtures/argument-repair.txt') and value['handout_changes_rejected'] is True and
                value['argument_type_error_rejected'] is True and value['lean_arguments'] == ['-j1'] and
                value['lean_imports'] == ['Init.Data.Int.DivMod.Lemmas'] and
                first_executed_attempt(value),
                'qualify the exact repair instruction and protected handout')
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
        rejected = common.fr(binary, original, ['proof', 'check', prepared['target'], '--draft', str(unfinished.resolve()), '--bytes', '4096', '--timeout-seconds', '30'], output / 'initial-check.json')
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
    if milestone == 'M3-facts':
        result.update(declarations_sha256=qualify_declarations(output), handout_changes_rejected=True,
                      lean_arguments=prepared['lean_arguments'], lean_imports=prepared['lean_imports'],
                      run_attempt=int(os.environ['GITHUB_RUN_ATTEMPT']),
                      unexecuted_prior_attempts=unexecuted_prior_attempts())
        common.save(output / 'controls.json', result)
    if milestone == REPAIR_STUDY:
        from .page_count_diagnosis import drafts, REFERENCE
        bad = output / 'argument-type-error.lean'
        bad.write_text(drafts(REFERENCE.read_text())['integer-instead-of-proof'])
        rejected = common.fr(binary, original, ['proof', 'check', prepared['target'], '--draft', str(bad.resolve()), '--bytes', '4096', '--timeout-seconds', '30'], output / 'argument-type-error.json')
        require(rejected['passed'] is False and rejected['receipt'] is None and
                any(d['draft_location'] and 'Int' in str(d['context']) and '0 ≤' in str(d['context'])
                    for d in rejected['diagnostics']), 'require the mapped actual and expected argument types')
        for condition in prepare(ROOT, milestone)['requirements']['setups']:
            project = output / ('handout-' + condition);project.mkdir()
            handout = install_repair_instruction(project, condition)
            protected = {'.fr/argument-repair.txt': common.digest(handout)}
            require(local.checking_input_changes(project, protected) == [], 'clean repair handout changed')
            handout.write_text('altered\n')
            require(local.checking_input_changes(project, protected) == ['.fr/argument-repair.txt'],
                    'reject changed repair handouts')
            handout.unlink()
            require(local.checking_input_changes(project, protected) == ['.fr/argument-repair.txt'],
                    'reject deleted repair handouts')
        result.update(argument_type_error_rejected=True, lean_arguments=prepared['lean_arguments'],
                      lean_imports=prepared['lean_imports'], run_attempt=int(os.environ['GITHUB_RUN_ATTEMPT']),
                      unexecuted_prior_attempts=unexecuted_prior_attempts(),
                      repair_instruction_sha256=common.digest(ROOT / CASE_DIRECTORIES[milestone] /
                      'fixtures/argument-repair.txt'), handout_changes_rejected=True)
        common.save(output / 'controls.json', result)
    for variant in VARIANTS:
        candidate = output / ('candidate-' + variant)
        common.checkout(original, candidate)
        if variant in {'reference', 'changed-source', 'missing-patch', 'wrong-patch'}:
            tactics = (ROOT / proof.DIRECTORY / 'controls/page-count.lean' if milestone in {'M3-page-count', REPAIR_STUDY} else
                       ROOT / details['directory'] / 'controls/reference.lean')
            common.fr(binary, candidate, ['proof', 'save', prepared['target'], '--draft', str(tactics), '--bytes', '4096', '--timeout-seconds', '30'],
                      output / (variant + '-write.json'))
        if variant == 'changed-source':
            source = candidate / prepared['source']
            operator = ' / ' if milestone in {'M3-feedback', 'M3-page-count', 'M3-facts', REPAIR_STUDY} else ' * '
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
    from .selected_local import study_limits
    wall_seconds = study_limits(milestone)['wall_seconds']
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
                if milestone == 'M3-facts':
                    usable = usable and (row['stop_reason'] == 'wall_seconds' or
                        (row['reported_usage_complete'] is True and row['client_model_recorded'] is True))
                if row['independent_outcome'] == 'passed':
                    if milestone in {REPAIR_STUDY, 'M3-facts'}:
                        require(row['reported_usage_complete'] is True and row['client_model_recorded'] is True,
                                'incomplete native accounting cannot count as complete success')
                    require(row['native_status'] == 'completed' and row['stop_reason'] is None and
                            row['complete_attempt_deadline_missed'] is False and row['total_seconds'] <= wall_seconds and
                            not row['checking_input_changes'] and not row['collection_error_recorded'] and
                            row['independent_check']['patch_accepted'] is True,
                            'a timed-out diagnostic remains failed even if its patch passes')
                    if milestone in {'M3-construction', 'M3-page-count', 'M3-facts', REPAIR_STUDY}:
                        require(construction_checks_passed(row), 'unobserved construction checks remain failed')
            counts[condition] = sum(row['independent_outcome'] == 'passed' for row in selected)
        result[profile] = {'accepted': counts, 'execution_usable': usable}
    return result


def decisions(rows, milestone):
    require(milestone in {'M3-feedback', 'M3-facts', REPAIR_STUDY}, 'choose the explicit two-condition diagnostic')
    result = outcomes(rows, milestone)
    schedule = prepare(ROOT, milestone)
    control, supplied = schedule['requirements']['setups']
    for profile, row in result.items():
        counts = row['accepted']
        key = str(counts[supplied]) + ',' + str(counts[control])
        rules = schedule['requirements']['decisions']
        decision = rules[key] if key in rules else rules['mixed']
        if not row['execution_usable']:
            decision = 'Repair or clarify experiment execution before interpreting these results'
        row['decision'] = decision
    return result


def validate_repair_observations(value, total_seconds):
    require(isinstance(value, dict) and set(value) ==
            {'first_argument_type_error', 'next_action', 'next_proof_check'},
            'review the first mapped argument error, next action and next check separately')
    error, action, checked = (value[name] for name in ('first_argument_type_error', 'next_action', 'next_proof_check'))
    if error is None:
        require(action is None and checked is None, 'no argument error means the repair mechanism is unobserved')
        return 'unobserved'
    require(isinstance(error, dict) and set(error) == {'seconds', 'diagnostic_sha256', 'summary'},
            'identify the first mapped argument error')
    records = [(error, 'diagnostic_sha256')]
    if action is not None:
        require(isinstance(action, dict) and set(action) == {'seconds', 'kind', 'summary'} and
                action['kind'] in {'read-diagnostic', 'edit', 'proof-check', 'theorem-search', 'other'},
                'describe the next actual action')
        records.append((action, None))
    if checked is not None:
        require(action is not None and isinstance(checked, dict) and set(checked) ==
                {'seconds', 'report_sha256', 'argument_mismatch_fixed', 'passed', 'theorem_search_before_check'} and
                all(type(checked[key]) is bool for key in
                    ('argument_mismatch_fixed', 'passed', 'theorem_search_before_check')),
                'record the next check separately from whole-proof success')
        require(action['kind'] != 'theorem-search' or checked['theorem_search_before_check'],
                'do not hide a theorem search before the next check')
        records.append((checked, 'report_sha256'))
    previous = 0
    for row, digest in records:
        seconds = row['seconds']
        require(type(seconds) in {int, float} and math.isfinite(seconds) and previous <= seconds <= total_seconds,
                'observations must follow the actual attempt order')
        previous = seconds
        if digest:
            require(isinstance(row[digest], str) and re.fullmatch('[0-9a-f]{64}', row[digest]),
                    'bind the private diagnostic or proof-check report')
        if 'summary' in row:
            require(isinstance(row['summary'], str) and 0 < len(row['summary']) <= 240 and '\n' not in row['summary'],
                    'publish a concise reviewed observation, not a raw record')
    return ('met' if checked is not None and checked['argument_mismatch_fixed'] and
            not checked['theorem_search_before_check'] else 'not-met')


def repair_decision(rows):
    result = decisions(rows, REPAIR_STUDY)
    for profile, outcome in result.items():
        outcome['mechanism'] = {}
        for condition in ('shared-skill', 'argument-repair'):
            counts = {'met': 0, 'not-met': 0, 'unobserved': 0}
            for row in rows:
                if row['profile'] == profile and row['setup'] == condition:
                    observation = validate_repair_observations(row['argument_repair_observations'], row['total_seconds'])
                    counts[observation] += 1
            outcome['mechanism'][condition] = counts
    complete = all(row['execution_usable'] and row['accepted']['argument-repair'] >= 1 for row in result.values())
    return {'profiles': result, 'completion_prediction_met': complete,
            'next_step': ('Inspect the mechanism observations before choosing a transfer task; M3 remains open.' if complete else
                          'Keep the broader 36-attempt comparison paused; inspect the first unresolved step.')}


def construction_checks_passed(row):
    return all(row['proof_observations'][key] is True for key in
               ('proof_check_passed', 'proof_written_with_fr', 'package_check_passed'))


def construction_decision(rows):
    result = outcomes(rows, 'M3-construction')
    ready = all(row['execution_usable'] and row['accepted']['fr'] == 2 for row in result.values())
    requirements = prepare(ROOT, 'M3-construction')['requirements']
    return {'profiles': result, 'all_four_completed': ready,
            'decision': requirements['decision_if_all_four_pass' if ready else 'decision_if_any_fail']}


def page_count_decision(rows):
    result = outcomes(rows, 'M3-page-count')
    ready = all(row['execution_usable'] and row['accepted']['fr'] >= 1 for row in result.values())
    requirements = prepare(ROOT, 'M3-page-count')['requirements']
    return {'profiles': result, 'prediction_met': ready,
            'decision': requirements['decision_if_prediction_met' if ready else 'decision_if_prediction_fails']}


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
                if milestone in {'M3-construction', 'M3-page-count', 'M3-facts', REPAIR_STUDY}:
                    accepted = accepted and construction_checks_passed(row)
                if milestone in {REPAIR_STUDY, 'M3-facts'}:
                    accepted = accepted and row['reported_usage_complete'] is True and row['client_model_recorded'] is True
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
    if summary['study'] == REPAIR_STUDY:
        result = repair_decision(summary['attempts'])
        lines = ['# Does repairing typed arguments help finish page count?', '',
                 'Eight fresh attempts compare the same complete page-count claim, fr and shared skill.',
                 'Only one condition adds the post-error argument-repair instruction. Each complete attempt has 150 seconds.',
                 'The baseline is the same agent with the shared skill alone; this does not measure fr overhead against ordinary Lean.', '',
                 '| Agent | Shared skill: accepted / 2 | Repair instruction: accepted / 2 | Decision |',
                 '| --- | ---: | ---: | --- |']
        for profile, row in result['profiles'].items():
            lines.append(f'| {profile} | {row["accepted"]["shared-skill"]} | {row["accepted"]["argument-repair"]} | {row["decision"]} |')
        lines += ['', result['next_step'], '',
                  'A successful proof without an argument-type error leaves the proposed mechanism unobserved.',
                  'First-check timing is observational. These two repetitions cannot establish a general success rate.',
                  'An accepted proof establishes only the fixed mathematical model; its connection to Rust execution stays unproved.',
                  'Only submitted patches and reviewed summaries are public; raw agent records stay private.', '',
                  '| Attempt | Complete outcome | Seconds | Argument-repair mechanism |', '| --- | --- | ---: | --- |']
        for row in summary['attempts']:
            observed = validate_repair_observations(row['argument_repair_observations'], row['total_seconds'])
            lines.append(f'| {row["id"]} | {row["independent_outcome"]} | {row["total_seconds"]:.3f} | {observed} |')
        return '\n'.join(lines) + '\n'
    if summary['study'] in {'M3-construction', 'M3-page-count'}:
        if summary['study'] == 'M3-page-count':
            result = page_count_decision(summary['attempts'])
            title = '# Can both agents finish the page-count proof?'
            introduction = ('Four attempts use fr and the unchanged M3 page-count claim. '
                            'The prediction requires at least one complete accepted attempt from each agent. '
                            'Read [what the agents actually did](../README.md).')
        else:
            result = construction_decision(summary['attempts'])
            title = '# Can the agents construct the multiplication proof and finish?'
            introduction = 'Four fresh development attempts start with an unfinished theorem and the same corrected instructions.'
        lines = [title, '', introduction,
                 'Each complete attempt keeps its 150-second limit and unchanged check settings.',
                 'This single-condition check has no timing baseline or causal improvement claim.', '',
                 result['decision'], '', '| Agent | Complete accepted attempts / 2 |', '| --- | ---: |']
        for profile, row in result['profiles'].items():
            lines.append(f'| {profile} | {row["accepted"]["fr"]} |')
        lines += ['', 'This experiment asks for proofs of the fixed mathematical model; the connection to source execution remains unproved.',
                  'Only patches and reviewed summaries are public. Raw agent records stay local.', '',
                  '| Attempt | Outcome | Complete seconds | Patch |', '| --- | --- | ---: | --- |']
        for row in summary['attempts']:
            patch = '[patch](' + row['patch_file'] + ')' if row['patch_file'] else 'none'
            lines.append(f'| {row["id"]} | {row["independent_outcome"]} | {row["total_seconds"]:.3f} | {patch} |')
        return '\n'.join(lines) + '\n'
    require(summary['study'] in {'M3-feedback', 'M3-facts', REPAIR_STUDY}, 'choose the explicit diagnostic report')
    result = decisions(summary['attempts'], summary['study'])
    control, supplied = prepare(ROOT, summary['study'])['requirements']['setups']
    lines = (['# Can the agents select and apply current division facts?', '',
             'Eight attempts compare the same small claim, fr, instructions and complete 150-second limit.',
             'One condition supplies four current library declaration types; neither supplies a proof.',
             'This development example cannot establish full page-count success or fr overhead against ordinary tools.', '',
             '| Agent | Select facts: accepted / 2 | Supplied declarations: accepted / 2 | Next question |',
             '| --- | ---: | ---: | --- |'] if summary['study'] == 'M3-facts' else
            ['# Can the agents use an initial arithmetic error report?', '',
             'Eight assisted repair attempts compare the same small division claim, bad draft, fr and instructions.',
             'Only initial visibility of the actual rejection report differs. Each complete attempt keeps its 150-second limit.',
             'Both conditions include generating the initial check. Failed attempts remain in the totals.', '',
             '| Agent | Finds feedback: accepted / 2 | Supplied feedback: accepted / 2 | Next question |',
             '| --- | ---: | ---: | --- |'])
    for profile, row in result.items():
        lines.append(f'| {profile} | {row["accepted"][control]} | '
                     f'{row["accepted"][supplied]} | {row["decision"]} |')
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
        command.add_argument('--study', choices=tuple(sorted(PROOF_DIAGNOSTICS)), required=True)
    args = parser.parse_args()
    if args.operation == 'controls':
        print(json.dumps(controls(args.fr.resolve(), args.output.resolve(), json.loads(args.native.read_text()), args.study)))
    elif args.operation == 'grade':
        print(json.dumps(grade_public(args.data.resolve(), args.artifacts.resolve(), args.fr.resolve(), args.output.resolve(), args.control_run, args.study)))
    else:
        summary = json.loads((args.data / 'summary.json').read_text())
        require(summary['study'] == args.study, 'requested report differs from the recorded study')
        (args.data / 'README.md').write_text(markdown(summary))
