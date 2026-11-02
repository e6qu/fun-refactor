"""Audit the original study at its frozen source, then replay patches with current fr."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

from . import proof_algorithms as algorithms
from . import selected_grading as grading
from . import selected_tasks as common
from .selected_plan import ROOT, encoded, require

DATA = ROOT / algorithms.DIRECTORY / 'local-results'


def github_run(run, source, workflow):
    value = json.loads(subprocess.check_output(
        ['gh', 'api', f'repos/e6qu/fun-refactor/actions/runs/{run}'], text=True, timeout=30))
    require(value['head_sha'] == source and value['status'] == 'completed' and
            value['conclusion'] == 'success' and value['path'] == workflow,
            'the recorded independent check needs a successful matching GitHub run')


def audit(frozen, output):
    manifest = json.loads((DATA / 'frozen-study-source.json').read_text())
    commit = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=frozen, text=True).strip()
    require(commit == manifest['source_commit'], 'audit the original source, not current checking code')
    github_run(manifest['qualification_run'], manifest['qualification_head_commit'],
               '.github/workflows/proof-algorithms.yml')
    # Import the original checker in a fresh process; never teach it to accept changed inputs.
    program = '''
import json, sys, tempfile
from pathlib import Path
from agent_eval import selected_local_grade as public, proof_algorithms as algorithms
from agent_eval import selected_public, algorithm_results
from agent_eval.selected_plan import require
root = Path(sys.argv[1])
for study in ('M3-algorithm-facts', 'M3-algorithm-presentation'):
    data = root / study
    summary = json.loads((data / 'summary.json').read_text())
    public.verify(summary, study)
    algorithms.validate_controls(summary['controls'], study)
    algorithm_results.decisions(summary)
    require((data / 'README.md').read_text() == algorithm_results.markdown(summary), 'original report changed')
    receipt = json.loads((data / 'independent-check-receipt.json').read_text())
    with tempfile.TemporaryDirectory() as temporary:
        copied = Path(temporary)
        (copied / 'summary.json').write_bytes((data / 'summary.json').read_bytes())
        selected_public.accept_checks(copied, data / 'independent-checks.json', receipt['url'])
        require((copied / 'summary.json').read_bytes() == (data / 'summary.json').read_bytes(),
                'public outcomes disagree with the original independent verdicts')
print('Frozen source, controls, reports and all twenty outcomes: passed')
'''
    environment = dict(os.environ, PYTHONPATH=os.pathsep.join(
        [str(frozen / 'tools'), str(frozen / 'sdk/python/src')]), PYTHONDONTWRITEBYTECODE='1')
    subprocess.run([sys.executable, '-B', '-c', program, str(DATA)], cwd=frozen,
                   env=environment, check=True, timeout=60)
    counts = {}
    for study in manifest['studies']:
        data = DATA / study
        receipt = json.loads((data / 'independent-check-receipt.json').read_text())
        require(common.digest(data / 'independent-checks.json') == receipt['checks_sha256'] and
                receipt['source_commit'] == manifest['grading_source_commit'] and
                receipt['control_run'] == manifest['qualification_run'] and
                receipt['url'] == 'https://github.com/e6qu/fun-refactor/actions/runs/' + receipt['run'],
                'preserve authenticated original check bytes and source')
        github_run(receipt['run'], receipt['source_commit'], receipt['workflow'])
        summary = json.loads((data / 'summary.json').read_text())
        require(summary['controls']['source_commit'] == manifest['qualification_source_commit'],
                'preserve the qualification checkout, separately from its PR head')
        for row in summary['attempts']:
            if row['patch_file'] is not None:
                patch = data / row['patch_file']
                require(row['patch_file'] == 'patches/' + row['id'] + '.patch' and
                        patch.is_file() and not patch.is_symlink() and
                        common.digest(patch) == row['patch_sha256'], 'original submitted patch changed')
        counts[study] = {'attempts': len(summary['attempts']),
                         'accepted': sum(row['independent_outcome'] == 'passed' for row in summary['attempts'])}
    result = {'passed': True, 'live_model_calls': 0, 'frozen_source': commit, 'studies': counts}
    common.save(output, result)
    return result


def replay(binary, qualification, output):
    require(os.environ.get('GITHUB_ACTIONS') == 'true', 'replay all retained patches on GitHub')
    output.mkdir(parents=True, exist_ok=False)
    controls = json.loads((qualification / 'controls.json').read_text())
    algorithms.validate_controls(controls, 'M3-algorithm-facts')
    require(common.digest(binary) == controls['fr_sha256'], 'replay the currently qualified executable')
    original = output / 'original'
    prepared = algorithms.prepare(binary, original, output / 'inputs', ROOT / algorithms.DIRECTORY / 'fixtures/Euclid.lean')
    image = controls['private_image']
    rust = Path(subprocess.check_output(['rustc', '--print', 'sysroot'], text=True).strip())
    rows = []
    for study in ('M3-algorithm-facts', 'M3-algorithm-presentation'):
        summary = json.loads((DATA / study / 'summary.json').read_text())
        require(hashlib.sha256(encoded(prepared['original'])).hexdigest() == summary['task_source_sha256']['euclid'],
                'replay the unchanged algorithm and claims')
        rows += [(study, row, DATA / study / row['patch_file']) for row in summary['attempts']
                 if row['patch_file'] is not None]
    require(len(rows) == 8, 'retain exactly eight canonical delivered patches')
    misplaced = json.loads((DATA / 'misplaced-submission/summary.json').read_text())
    rows.append(('misplaced diagnostic only',
                 {'id': misplaced['attempt'], 'patch_sha256': misplaced['patch_sha256']},
                 DATA / 'misplaced-submission' / misplaced['patch_file']))
    checked = []
    for study, row, patch in rows:
        require(patch.is_file() and not patch.is_symlink() and common.digest(patch) == row['patch_sha256'],
                'the retained original patch changed')
        verdict = grading.grade(image, 'euclid', original, prepared, binary, rust, patch,
                                output / row['id'], milestone='M3-algorithm-facts', tools={})
        checked.append({'id': row['id'], 'study': study, 'patch_sha256': row['patch_sha256'],
                        'passed': verdict['passed'], 'stage': verdict['verdict']['stage']})
    result = {'passed': all(row['passed'] for row in checked), 'live_model_calls': 0,
              'source_commit': os.environ['GITHUB_SHA'], 'run': os.environ['GITHUB_RUN_ID'],
              'fr_sha256': common.digest(binary), 'canonical_patches': 8, 'misplaced_diagnostics': 1,
              'original_outcomes_changed': False, 'checks': checked,
              'scope': 'Current checker replay only. The misplaced diff remains a failed original delivery; no new model success or timing result.'}
    common.save(output / 'summary.json', result)
    require(result['passed'], 'fix a current checker rejection without changing original outcomes')
    return result


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    frozen = commands.add_parser('audit')
    frozen.add_argument('--frozen-root', type=Path, required=True)
    frozen.add_argument('--output', type=Path, required=True)
    current = commands.add_parser('replay')
    for name in ('fr', 'qualification', 'output'):
        current.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args()
    print(json.dumps(audit(args.frozen_root.resolve(), args.output.resolve()) if args.command == 'audit' else
                     replay(args.fr.resolve(), args.qualification.resolve(), args.output.resolve())))
