"""Reproduce division mistakes against the complete fixed page-count claim."""
import argparse
import os
from pathlib import Path

from . import proof_writing_tasks as proof
from . import selected_tasks as common
from .selected_plan import ROOT, require


REFERENCE = ROOT / proof.DIRECTORY / 'controls/page-count.lean'
MUTATIONS = {
    'integer-instead-of-proof': ('Int.tdiv_nonneg hi0 (by omega : 0 ≤ size)',
                               'Int.tdiv_nonneg items size'),
    'upper-bound-instead-of-lower-bound': ('Int.tdiv_nonneg hi0 (by omega : 0 ≤ size)',
                                         'Int.tdiv_nonneg _himax (by omega : 0 ≤ size)'),
    'division-fact-missing': ('have hq0 := Int.tdiv_nonneg hi0 (by omega : 0 ≤ size)',
                              'have hq0 : 0 ≤ Int.tdiv items size := by omega'),
    'wrong-division-family': ('Int.tdiv_nonneg hi0 (by omega : 0 ≤ size)',
                              'Int.ediv_nonneg hi0 (by omega : 0 ≤ size)'),
    'conversion-missing': ('        rw [Int.tdiv_eq_ediv_of_nonneg hi0]\n', ''),
    'alternatives-treated-as-pair': ('rcases invalid with hi | hs', 'rcases invalid with ⟨hi, hs⟩'),
    'nonlinear-relationship-missing': ('exact Int.ediv_lt_self_of_pos_of_ne_one hipos hsne', 'omega'),
}


def drafts(reference):
    result = {'reference': reference}
    for name, (before, after) in MUTATIONS.items():
        require(reference.count(before) == 1, 'control must change exactly one proof step: ' + name)
        result[name] = reference.replace(before, after, 1)
    result['reference-again'] = reference
    return result


def validate(name, value):
    require(name in {'reference', 'reference-again', *MUTATIONS}, 'unknown diagnostic control')
    expected = name in {'reference', 'reference-again'}
    require(type(value.get('passed')) is bool and value['passed'] is expected,
            'unexpected page-count control result: ' + name)
    require((value.get('receipt') is not None) is expected,
            'only a checked complete reference may have a receipt: ' + name)
    if not expected:
        require(any(row.get('draft_location') and row.get('message')
                    for row in value.get('diagnostics', [])),
                'a rejection needs a Lean error mapped to the changed draft: ' + name)


def run(binary, output):
    require(os.environ.get('GITHUB_ACTIONS') == 'true', 'run this collection on GitHub')
    output.mkdir(parents=True, exist_ok=False)
    project = output / 'project'
    prepared = proof.prepare('page-count', binary, project, output / 'inputs', [])
    baseline = common.sources(project)
    report = {'schema': 'fr-page-count-diagnosis-1', 'passed': False, 'live_model_calls': 0,
              'source_commit': os.environ['GITHUB_SHA'], 'fr_sha256': common.digest(binary),
              'reference_sha256': common.digest(REFERENCE), 'property': prepared['property'],
              'model': prepared['model'], 'arithmetic': prepared['arithmetic'], 'controls': {}}
    common.save(output / 'summary.json', report)
    for name, text in drafts(REFERENCE.read_text()).items():
        draft = output / (name + '.lean')
        draft.write_text(text)
        value = common.fr(binary, project, ['spec', 'proof-check', prepared['target'],
            '--from', str(draft), '--token-limit', '4096'], output / (name + '-check.json'))
        validate(name, value)
        require(common.sources(project) == baseline, 'a diagnostic changed the prepared project')
        report['controls'][name] = {'accepted': value['passed'], 'draft_sha256': common.digest(draft),
                                   'diagnostics': value['diagnostics'],
                                   'diagnostics_omitted': value['diagnostics_omitted']}
        common.save(output / 'summary.json', report)
    report['passed'] = True
    common.save(output / 'summary.json', report)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--fr', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    run(args.fr.resolve(), args.output.resolve())
