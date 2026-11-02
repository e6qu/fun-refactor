"""Supplied proof controls, kept outside every live agent's project."""
import argparse
import json
from pathlib import Path
import shutil
import tempfile
from textwrap import dedent, indent

from . import proof_writing_tasks as tasks
from .selected_plan import ROOT, require


def reference(case):
    name = 'access' if case.startswith('access-') else case
    return (ROOT / tasks.DIRECTORY / 'controls' / (name + '.lean')).read_text()


def drift_checks(case, binary, original, prepared, tactics, output):
    common = tasks.common
    output.mkdir()
    with tempfile.TemporaryDirectory(prefix='m3-proof-drift-') as temporary:
        project = Path(temporary) / 'project'
        common.checkout(original, project)
        target = prepared['target']
        model_path = project / prepared['spec']
        model_text = model_path.read_text()
        first = common.fr(binary, project, ['spec', 'proof-task', target], output / 'original-task.json')
        source = project / prepared['source']
        source_text = source.read_text()
        if case == 'page-count':
            changed_source = source_text.replace('items < 0', 'items <= 0', 1)
        elif case == 'fits-window':
            changed_source = source_text.replace('offset <= total', 'offset < total', 1)
        elif case == 'access-python':
            changed_source = source_text.replace('if staff:', 'if not staff:', 1)
        elif case in {'access-java', 'access-typescript', 'access-tsx', 'access-zig'}:
            changed_source = source_text.replace('if (staff)', 'if (!staff)', 1)
        else:
            changed_source = source_text.replace('if staff', 'if !staff', 1)
        require(changed_source != source_text, 'source control did not change its comparison')
        source.write_text(changed_source)
        stale = common.command([str(binary), '-C', str(project), '--json', 'spec', 'proof-task', target], project, output / 'stale-source.json')
        require(stale['returncode'] != 0, 'a changed source cannot retain the old proof task')
        source.write_text(source_text)
        model_path.write_text(model_text + '\n-- changed model context\n')
        changed_model = common.fr(binary, project, ['spec', 'proof-task', target], output / 'changed-model.json')
        require(changed_model['goal']['id'] != first['goal']['id'], 'a model change must expire its goal identity')
        parameters = ''.join(f' ({row["name"]} : {row["lean_type"]})' for row in prepared['property']['parameters'])
        lines = model_text.splitlines(keepends=True)
        selected = [index for index, line in enumerate(lines) if line.startswith('theorem ' + prepared['obligation'] + ' ')]
        require(len(selected) == 1, 'control needs exactly one fixed theorem header')
        lines[selected[0]] = f'theorem {prepared["obligation"]}{parameters} : False := by\n'
        model_path.write_text(''.join(lines))
        changed_claim = common.fr(binary, project, ['spec', 'proof-task', target], output / 'changed-claim.json')
        require(changed_claim['goal']['id'] != first['goal']['id'], 'a changed property must expire the old goal')
        false_claim = common.fr(binary, project, ['spec', 'proof-check', target, '--from', str(tactics)], output / 'false-claim.json')
        require(false_claim['passed'] is False and false_claim['receipt'] is None, 'a false fixed claim must fail Lean checking')
        dependency = project / 'specs/FrSpecs/M3Dependency.lean'
        dependency_text = 'namespace FrSpecs.M3Dependency\ndef enabled : Bool := true\nend FrSpecs.M3Dependency\n'
        dependency.write_text(dependency_text)
        definition = prepared['model']
        head, body = definition.split(':=\n', 1)
        otherwise = '-99' if case == 'page-count' else 'false'
        wrapped = head + ':=\n  if M3Dependency.enabled then\n' + indent(dedent(body), '    ') + '\n  else\n    ' + otherwise
        require(model_text.count(definition) == 1, 'dependency control needs the one generated model definition')
        model_path.write_text('import FrSpecs.M3Dependency\n' + model_text.replace(definition, wrapped, 1))
        dependency_tactics = output / 'dependency-tactics.lean'
        model_name = head.split('def ', 1)[1].split(' ', 1)[0]
        dependency_tactics.write_text(tactics.read_text().replace(
            '[' + model_name, '[' + model_name + ', M3Dependency.enabled'))
        before_dependency = common.fr(binary, project, ['spec', 'proof-task', target], output / 'dependency-task.json')
        dependency_proof = common.fr(binary, project, ['spec', 'proof-check', target, '--from', str(dependency_tactics)], output / 'dependency-accepted.json')
        require(dependency_proof['passed'] is True, 'reference must pass with its declared dependency')
        dependency.write_text(dependency_text.replace(':= true', ':= false'))
        after_dependency = common.fr(binary, project, ['spec', 'proof-task', target], output / 'dependency-changed.json')
        require(before_dependency['goal']['id'] != after_dependency['goal']['id'], 'dependency edits must expire the old proof task')
        rejected_dependency = common.fr(binary, project, ['spec', 'proof-check', target, '--from', str(dependency_tactics)], output / 'dependency-rejected.json')
        require(rejected_dependency['passed'] is False and rejected_dependency['receipt'] is None,
                'a stale compiled dependency cannot preserve an invalid proof')
    return True


def protected_inputs(binary, project, prepared, output):
    """A candidate may supply tactics, never replace the question or its assumptions."""
    output.mkdir()
    model = project / prepared['spec']
    original = model.read_text()
    for name, path, content in (
            ('source', project / prepared['source'], (project / prepared['source']).read_text() + '\n'),
            ('model', model, original + '\n-- changed definition context\n'),
            ('package', project / 'specs/lakefile.toml', (project / 'specs/lakefile.toml').read_text() + '\n'),
            ('axiom', model, original + '\naxiom unapproved : False\n')):
        before = path.read_text()
        try:
            path.write_text(content)
            rejected = tasks.check(project, prepared, binary, output / name)
            require(rejected['passed'] is False and rejected['stage'] == 'protected source',
                    'candidate changed protected inputs: ' + name)
        finally:
            path.write_text(before)
    begin = original.index('\n', original.index('-- fr:proof-begin ' + prepared['obligation'])) + 1
    end = original.rfind('\n', 0, original.index('-- fr:proof-end ' + prepared['obligation'])) + 1
    for tactic in ('sorry', 'admit'):
        try:
            model.write_text(original[:begin] + '  ' + tactic + '\n' + original[end:])
            rejected = tasks.check(project, prepared, binary, output / tactic)
            require(rejected['passed'] is False, 'unfinished proof accepted: ' + tactic)
        finally:
            model.write_text(original)
    return True


def preserve_handwritten(binary, project, prepared, inputs, output):
    common = tasks.common
    output.mkdir()
    model = project / prepared['spec']
    before = model.read_text()
    marker = '-- fr:handwritten-end additional-models-and-proofs'
    note = '-- user proof outside generated definitions\ntheorem user_note : True := by trivial\n'
    require(before.count(marker) == 1, 'the model needs one handwritten extension region')
    model.write_text(before.replace(marker, note + marker))
    plan = common.fr(binary, project, ['spec', 'plan', prepared['source'] + '::' + tasks.CASES[prepared['task']]['symbol'],
                       '--property-from', str(inputs / 'property.json')], output / 'plan-log.json')
    common.save(output / 'plan.json', plan)
    common.fr(binary, project, ['spec', 'scaffold', '--from', str(output / 'plan.json'), '--package', 'specs', '--write'], output / 'regenerate.json')
    require(note in model.read_text(), 'regeneration discarded an unrelated handwritten proof')
    verified = common.command([str(binary), '-C', str(project), '--json', 'spec', 'verify', 'specs'], project, output / 'verify.json')
    require(verified['returncode'] == 0, 'the regenerated package lost an accepted proof')
    return True


def exercise(case, binary, project, prepared, output, inputs):
    """Check diagnostics, an exact proof-region write, patch replay and reversal."""
    output.mkdir(parents=True, exist_ok=False)
    target, common = prepared['target'], tasks.common
    baseline = common.sources(project)
    unfinished = tasks.check(project, prepared, binary, output / 'unfinished')
    require(unfinished['passed'] is False, 'an unchanged proof gap must fail')
    wrong = output / 'wrong.lean'
    wrong.write_text('exact false\n')
    rejected = common.fr(binary, project, ['spec', 'proof-check', target, '--from', str(wrong)], output / 'wrong-check.json')
    require(rejected['passed'] is False and rejected['receipt'] is None and rejected['diagnostics'],
            'invalid tactics must retain useful diagnostics and no receipt')
    require(common.sources(project) == baseline, 'a rejected proof check changed source')
    tactics = output / 'reference.lean'
    tactics.write_text(reference(case))
    checked = common.fr(binary, project, ['spec', 'proof-check', target, '--from', str(tactics)], output / 'reference-check.json')
    require(checked['passed'] is True and checked['receipt'], 'supplied reference proof failed')
    require(common.sources(project) == baseline, 'proof-check must leave the prepared source unchanged')
    common.fr(binary, project, ['spec', 'prove', target, '--from', str(tactics)], output / 'preview.json')
    require(common.sources(project) == baseline, 'a proof preview changed source')
    written = common.fr(binary, project, ['spec', 'prove', target, '--from', str(tactics), '--write'], output / 'write.json')
    require(written['receipt'] == checked['receipt'], 'previewed proof and written proof must bind the same checked context')
    accepted = tasks.check(project, prepared, binary, output / 'accepted')
    require(accepted['passed'] is True, 'reference failed the independent proof and axiom checks')
    patch = common.comparison_grade.BASE.git(project, 'diff', '--binary', 'HEAD').stdout
    (output / 'reference.patch').write_bytes(patch)
    with tempfile.TemporaryDirectory(prefix='m3-proof-receiver-') as temporary:
        receiver = Path(temporary) / 'receiver'
        common.checkout(project, receiver)
        common.comparison_grade.BASE.git(receiver, 'apply', '--binary', str((output / 'reference.patch').resolve()))
        delivered = tasks.check(receiver, prepared, binary, output / 'receiver')
        require(delivered['passed'] is True, 'reference patch failed in a clean receiver')
    completed = common.sources(project)
    transaction = str(written['transaction'])
    common.fr(binary, project, ['history', 'undo', transaction, '--write'], output / 'undo.json')
    require(common.sources(project) == baseline, 'proof undo must restore the original bytes and modes')
    common.fr(binary, project, ['history', 'redo', transaction, '--write'], output / 'redo.json')
    require(common.sources(project) == completed, 'proof redo must restore the accepted bytes and modes')
    protected = protected_inputs(binary, project, prepared, output / 'protected')
    stale = drift_checks(case, binary, project, prepared, tactics, output / 'drift')
    preserved = preserve_handwritten(binary, project, prepared, inputs, output / 'handwritten')
    return {'case': case, 'live_model_calls': 0, 'model_proof_checked': True, 'negative_tactics_rejected': True, 'receiver_checked': True,
            'stale_records_checked': stale, 'protected_inputs_checked': protected, 'handwritten_preserved': preserved,
            'source_implementation_proved': False, 'scope': 'Supplied controller tactics; no model calls and no agent success claim.'}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--case', choices=tasks.CASES, required=True)
    parser.add_argument('--fr', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    arguments = parser.parse_args()
    arguments.output.mkdir(parents=True, exist_ok=False)
    prepared = tasks.prepare(arguments.case, arguments.fr.resolve(), arguments.output / 'original', arguments.output / 'inputs')
    result = exercise(arguments.case, arguments.fr.resolve(), arguments.output / 'original', prepared,
                      arguments.output / 'controls', arguments.output / 'inputs')
    tasks.common.save(arguments.output / 'result.json', result)
    print(json.dumps(result))
