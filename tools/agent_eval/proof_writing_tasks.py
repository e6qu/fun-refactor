"""Prepare fixed proof claims without supplying tactics to the selected agents."""
import argparse
import json
from pathlib import Path
import re

from . import selected_tasks as common
from .selected_plan import ROOT, require

DIRECTORY = Path('tests/agent-eval/proof-writing')
CASES = json.loads((ROOT / DIRECTORY / 'cases.json').read_text())
I64_MAX = 9223372036854775807
U64_MAX = 18446744073709551615


def variable(name):
    return {'kind': 'variable', 'name': name}


def integer(value, lean_type):
    return {'kind': 'integer', 'value': value, 'lean_type': lean_type}


def model(*arguments):
    return {'kind': 'model', 'arguments': list(arguments)}


def binary(operator, left, right):
    return {'kind': 'binary', 'operator': operator, 'left': left, 'right': right}


def relation(kind, left, right):
    return {'kind': kind, 'left': left, 'right': right}


def conjunction(*propositions):
    return {'kind': 'and', 'propositions': list(propositions)}


def implies(premise, conclusion):
    return {'kind': 'implies', 'premise': premise, 'conclusion': conclusion}


def property_for(case, task):
    """The experiment owns the immutable claim; agents own only its proof steps."""
    require(case in CASES, 'select one explicit M3 case')
    if case.startswith('access-'):
        parameters = [{'name': name, 'lean_type': 'Bool'} for name in ('staff', 'owner')]
        staff, owner = variable('staff'), variable('owner')
        proposition = relation('equals', model(staff, owner), binary('or', staff, owner))
    elif case == 'page-count':
        parameters = [{'name': name, 'lean_type': 'Int'} for name in ('items', 'size')]
        items, size = variable('items'), variable('size')
        zero, one, maximum = [integer(value, 'Int') for value in (0, 1, I64_MAX)]
        invalid = {'kind': 'or', 'propositions': [relation('less-than', items, zero), relation('less-or-equal', size, zero)]}
        valid = conjunction(relation('less-or-equal', zero, items), relation('less-or-equal', items, maximum),
                            relation('less-or-equal', one, size), relation('less-or-equal', size, maximum))
        bounds = conjunction(relation('less-or-equal', zero, model(items, size)),
                             relation('less-or-equal', model(items, size), items))
        proposition = conjunction(implies(invalid, relation('equals', model(items, size), integer(-1, 'Int'))),
            implies(valid, bounds), relation('equals', model(integer(11, 'Int'), integer(5, 'Int')), integer(3, 'Int')))
    else:
        parameters = [{'name': name, 'lean_type': 'Nat'} for name in ('offset', 'count', 'total')]
        offset, count, total = [variable(name) for name in ('offset', 'count', 'total')]
        accepted = {'kind': 'holds', 'term': model(offset, count, total)}
        within = relation('less-or-equal', binary('add', offset, count), total)
        proposition = conjunction(implies(accepted, within), implies(within, accepted))
    return {'schema': 'fr-formal-property-1', 'task_digest': task['object_digest'],
            'name': CASES[case]['property'], 'parameters': parameters, 'proposition': proposition}


def masked_proof(text, obligation):
    begin, end = '-- fr:proof-begin ' + obligation, '-- fr:proof-end ' + obligation
    require(text.count(begin) == text.count(end) == 1, 'keep the one original proof marker pair')
    start = text.index('\n', text.index(begin)) + 1
    finish = text.rfind('\n', 0, text.index(end)) + 1
    require(start < finish, 'proof markers are not ordered')
    return text[:start] + '<M3 agent proof steps>\n' + text[finish:]


def prepare(case, binary, project, output):
    require(case in CASES and not project.exists(), 'prepare one named proof case in a new folder')
    output.mkdir(parents=True, exist_ok=False)
    entry = CASES[case]
    project.mkdir(parents=True)
    source = project / entry['path']
    source.parent.mkdir(parents=True)
    source.write_bytes((ROOT / DIRECTORY / 'fixtures' / entry['path']).read_bytes())
    (project / 'Cargo.toml').write_text('[package]\nname="m3_proof"\nversion="0.0.0"\nedition="2024"\n\n[workspace]\n')
    common.fr(binary, project, ['spec', 'init', 'specs', '--write'], output / 'init.json')
    target = entry['path'] + '::' + entry['symbol']
    task = common.fr(binary, project, ['spec', 'property-task', target, '--token-limit', '4096'], output / 'property-task.json')
    property_ = property_for(case, task)
    common.save(output / 'property.json', property_)
    plan = common.fr(binary, project, ['spec', 'plan', target, '--property-from', str(output / 'property.json')], output / 'plan.json')
    common.fr(binary, project, ['spec', 'scaffold', '--from', str(output / 'plan.json'), '--package', 'specs', '--write'], output / 'scaffold.json')
    spec = 'specs/FrSpecs/' + plan['kernel']['module'] + '.lean'
    obligation = plan['properties'][0]['name']
    built = common.command(['lake', 'build'], project / 'specs', output / 'initial-package.json')
    require(built['returncode'] == 0, 'the prepared Lean package must build before a live attempt')
    catalog = common.fr(binary, project, ['spec', 'goals', 'specs'], output / 'goals.json')
    require(len(catalog['catalog']) == 1 and catalog['omitted'] == 0, 'disclose exactly the one fixed proof goal')
    goal = catalog['catalog'][0]
    common.fr(binary, project, ['spec', 'goals', 'specs', '--goal', goal['id']], output / 'goal.json')
    common.fr(binary, project, ['spec', 'proof-task', spec + '::' + obligation], output / 'proof-task.json')
    for name in common.sources(project):
        (project / name).chmod(0o644)
    common.comparison_grade.BASE.git(project, 'init', '-q', '-b', 'main')
    (project / '.git/info/exclude').write_text('\n'.join(sorted((common.IGNORED - {'.git'}) | {'change.patch'})) + '\n')
    common.comparison_grade.BASE.initialize(project)
    result = {'task': case, 'language': entry['language'], 'source': entry['path'], 'spec': spec,
        'obligation': obligation, 'target': spec + '::' + obligation,
        'original': common.sources(project), 'protected_proof': masked_proof((project / spec).read_text(), obligation),
        'plan_digest': plan['object_digest'], 'goal_digest': goal['id'],
        'model': plan['kernel']['lean_definition'], 'property': property_, 'assumptions': plan['assumptions'],
        'arithmetic': {'source': 'Boolean' if case.startswith('access-') else 'signed-64' if case == 'page-count' else 'unsigned-64',
                       'model': 'Bool' if case.startswith('access-') else 'Lean mathematical Int with truncating division' if case == 'page-count' else 'Lean mathematical Nat',
                       'source_implementation_proved': False},
        'scope': 'Source, model, fixed property and package are identical for both setups; no proof steps are supplied.'}
    common.save(output / 'project.json', result)
    common.retain_seed(project, result, output)
    return result


def boundaries(project, prepared):
    current, original = common.sources(project), prepared['original']
    changed = {name for name in current.keys() | original.keys() if current.get(name) != original.get(name)}
    require(changed <= {prepared['spec']}, 'change only the one declared proof region')
    require(masked_proof((project / prepared['spec']).read_text(), prepared['obligation']) == prepared['protected_proof'],
            'keep source, model, property, assumptions and unrelated proof regions unchanged')
    return sorted(changed)


def check(project, prepared, binary, output):
    output.mkdir(parents=True, exist_ok=False)
    try:
        changed = boundaries(project, prepared)
    except ValueError as error:
        return {'passed': False, 'stage': 'protected source', 'reason': str(error)}
    strict = common.command([str(binary), '-C', str(project), '--json', 'spec', 'check', 'specs', '--strict', '--max-debt', '0'], project, output / 'strict.json')
    if strict['returncode'] != 0:
        return {'passed': False, 'stage': 'unfinished or stale proof', 'checks': [strict]}
    checked = common.command([str(binary), '-C', str(project), '--json', 'spec', 'evidence', 'specs'], project, output / 'evidence.json')
    if checked['returncode'] != 0:
        return {'passed': False, 'stage': 'Lean package', 'checks': [strict, checked]}
    evidence = json.loads(checked['stdout'])
    require(evidence['correspondence']['proved_implementation_model'] is False, 'a model proof cannot establish source implementation equivalence')
    accepted = any(row['name'].endswith(prepared['obligation']) and row['status'] == 'checked_by_lean' for row in evidence['properties'])
    audit = output / 'Axioms.lean'
    audit.write_text('import FrSpecs.' + Path(prepared['spec']).stem + '\n#print axioms FrSpecs.' + prepared['obligation'] + '\n')
    axioms = common.command(['lake', 'env', 'lean', str(audit.resolve())], project / 'specs', output / 'axioms.json')
    names = []
    if 'does not depend on any axioms' not in axioms['stdout']:
        matched = re.search(r'depends on axioms:\s*\[([^]]*)\]', axioms['stdout'])
        require(matched is not None, 'could not read Lean axiom dependencies')
        names = [name.strip() for name in matched[1].split(',') if name.strip()]
    return {'passed': accepted and axioms['returncode'] == 0 and set(names) <= common.APPROVED_AXIOMS,
            'changed_paths': changed, 'model_proof_checked': accepted, 'axioms': names,
            'source_implementation_proved': False, 'checks': [strict, checked, axioms]}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--case', choices=CASES, required=True)
    parser.add_argument('--fr', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    arguments = parser.parse_args()
    result = prepare(arguments.case, arguments.fr.resolve(), arguments.output / 'original', arguments.output / 'inputs')
    print(json.dumps({'case': arguments.case, 'target': result['target'], 'goal_digest': result['goal_digest'],
                      'source_implementation_proved': False, 'live_model_calls': 0}))
