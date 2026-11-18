"""Keep every product-comparison slot and apply only the frozen within-round rule."""
import math
import re

from . import product_plan as plan
from .selected_plan import require


def finite(value):
    return type(value) in (int, float) and math.isfinite(value) and value >= 0


def validate_grading(summary, grading):
    """Bind each independent verdict to the exact submitted patch (or its absence)."""
    require(grading.get('schema') == 'fr-product-grading-1' and
            grading.get('round') == summary['round'] and
            isinstance(grading.get('run'), str) and grading['run'].isdigit() and
            isinstance(grading.get('source_commit'), str) and re.fullmatch(r'[0-9a-f]{40}', grading['source_commit']) and
            grading.get('checker_bindings'), 'retain the hosted independent grading identity')
    attempts = summary['attempts']
    expected = {row['id'] for row in attempts}
    receipts = grading.get('attempts', {})
    require(set(receipts) == expected, 'independently grade every scheduled slot exactly once')
    for row in attempts:
        receipt = receipts[row['id']]
        require(type(receipt.get('passed')) is bool and
                receipt.get('patch_sha256') == row.get('patch_sha256') and
                row.get('independent_outcome') == ('passed' if receipt['passed'] else 'failed'),
                'independent verdict differs from the submitted patch or published outcome')
        require(not receipt['passed'] or isinstance(row.get('patch_sha256'), str) and
                re.fullmatch(r'[0-9a-f]{64}', row['patch_sha256']), 'a missing patch cannot pass independent grading')
    return grading


def complete(row):
    """A late valid patch is an artifact success, but not a complete attempt."""
    return (row['status'] == 'completed' and row.get('independent_outcome') == 'passed' and
            row.get('patch_delivered') is True and row.get('checking_inputs_preserved') is True and
            row.get('accounting_complete') is True and row.get('resource_stop') is None and
            finite(row.get('elapsed_seconds')) and row['elapsed_seconds'] <= plan.LIMITS['wall_seconds'])


def summary(root, phase, observations, grading=None):
    require(phase in plan.TASKS, 'choose the comparison round explicitly')
    campaign = plan.campaign(root)
    scheduled = [row for row in campaign['attempts'] if row['round'] == phase]
    expected = {row['id'] for row in scheduled}
    require(set(observations) <= expected, 'unknown or cross-round result')
    rows = []
    immutable = {'id', 'round', 'profile', 'task', 'family', 'setup', 'repetition', 'position'}
    fields = {'status', 'independent_outcome', 'patch_delivered', 'patch_sha256', 'checking_inputs_preserved',
              'accounting_complete', 'resource_stop', 'elapsed_seconds', 'usage', 'cost', 'fr_contribution',
              'failure_summary'}
    for planned in scheduled:
        observed = observations.get(planned['id'], {})
        require(set(observed) <= fields | immutable and
                all(observed[key] == planned[key] for key in immutable & observed.keys()),
                'retain scheduled identity and only reviewed public fields')
        row = {**planned, 'independent_outcome': None, 'patch_delivered': False,
               'patch_sha256': None, 'checking_inputs_preserved': None, 'accounting_complete': None,
               'resource_stop': None, 'elapsed_seconds': None, 'failure_summary': None, **observed}
        require(row['status'] in {'unstarted', 'completed', 'failed'}, 'retain a terminal or unstarted slot')
        require(row['elapsed_seconds'] is None or finite(row['elapsed_seconds']), 'invalid complete-attempt time')
        require(row['cost'] is None or finite(row['cost']), 'invalid recorded cost')
        if row['status'] == 'unstarted':
            require(all(row[key] is None for key in ('elapsed_seconds', 'usage', 'cost', 'independent_outcome')) and
                    row['patch_delivered'] is False, 'unstarted time, usage and cost must stay unknown')
        row['complete_accepted'] = complete(row) if grading is not None else None
        rows.append(row)
    result = {'schema': 'fr-product-round-summary-1', 'round': phase, 'attempts': rows,
              'independently_graded': grading is not None,
              'scope': 'Within-round comparison on four fixed tasks; different rounds do not isolate implementation effects.'}
    if grading is not None:
        validate_grading(result, grading)
    by_profile = {}
    for profile in campaign['profiles']:
        chosen = [row for row in rows if row['profile'] == profile]
        accepted = {setup: sum(row['complete_accepted'] is True for row in chosen if row['setup'] == setup)
                    for setup in ('files', 'fr')}
        matched = []
        for family in plan.FAMILIES:
            group = [row for row in chosen if row['family'] == family]
            if len(group) == 4 and all(row['complete_accepted'] is True for row in group):
                times = {setup: sum(row['elapsed_seconds'] for row in group if row['setup'] == setup)
                         for setup in ('files', 'fr')}
                reduction = 1 - times['fr'] / times['files'] if times['files'] > 0 else None
                matched.append({'family': family, 'complete_seconds': times, 'reduction': reduction})
        finished = grading is not None and all(row['status'] in {'completed', 'failed'} for row in chosen)
        totals = {setup: sum(row['complete_seconds'][setup] for row in matched) for setup in ('files', 'fr')}
        reduction = 1 - totals['fr'] / totals['files'] if totals['files'] > 0 else None
        benefit = (accepted['fr'] >= accepted['files'] and len(matched) >= 2 and
                   reduction is not None and reduction >= 0.1 - 1e-12)
        by_profile[profile] = {'complete_accepted': accepted if grading is not None else None,
                              'matched_families': matched, 'combined_reduction': reduction,
                              'benefit_demonstrated': benefit if finished else None}
    result['profiles'] = by_profile
    result['billing'] = {'known_attempts': sum(row['cost'] is not None for row in rows),
                         'total': sum(row['cost'] for row in rows) if all(row['cost'] is not None for row in rows) else None}
    return result
