"""Supplied proof controls, kept outside every live agent's project."""
import argparse
import json
from pathlib import Path
import shutil
import tempfile

from . import proof_writing_tasks as tasks
from .selected_plan import ROOT, require


def reference(case):
    name = 'access' if case.startswith('access-') else case
    return (ROOT / tasks.DIRECTORY / 'controls' / (name + '.lean')).read_text()


def exercise(case, binary, project, prepared, output):
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
    return {'model_proof_checked': True, 'negative_tactics_rejected': True, 'receiver_checked': True,
            'source_implementation_proved': False, 'scope': 'Supplied controller tactics; no model calls and no agent success claim.'}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--case', choices=tasks.CASES, required=True)
    parser.add_argument('--fr', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    arguments = parser.parse_args()
    arguments.output.mkdir(parents=True, exist_ok=False)
    prepared = tasks.prepare(arguments.case, arguments.fr.resolve(), arguments.output / 'original', arguments.output / 'inputs')
    result = exercise(arguments.case, arguments.fr.resolve(), arguments.output / 'original', prepared, arguments.output / 'controls')
    tasks.common.save(arguments.output / 'result.json', result)
    print(json.dumps(result))
