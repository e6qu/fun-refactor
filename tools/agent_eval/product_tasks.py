"""Prepare reserved product tasks and grade changes with independent oracles.

Reference answers and private observations stay beside, never inside, live seeds.
Run preparation and qualification on a hosted runner. Live grading must invoke
candidate Python/Lean only inside the existing isolated checker environment.
"""
import argparse
import ast
import copy
import hashlib
import json
from pathlib import Path
import re
import shutil
import sys

from . import product_plan as plan, selected_tasks as common
from .selected_plan import ROOT, require
from project_change_sources import download

PROOF = 'specs/FrSpecs/Clamp.lean'
ARCHIVE_LIMITS = {'archive_bytes': 8 * 1024**2, 'expanded_bytes': 32 * 1024**2,
                  'archive_members': 4096}


def masked(text):
    markers = list(re.finditer(r'^  -- fr:proof-(begin|end) clamp_bounds\n', text, re.M))
    require([m[1] for m in markers] == ['begin', 'end'] and text.count('fr:proof-') == 2,
            'preserve the original proof markers')
    begin, end = markers
    require(begin.end() < end.start(), 'keep an ordered, nonempty proof region')
    return text[:begin.end()] + '<proof>\n' + text[end.start():]


def graph_answer(project):
    """Read Python AST locations and reviewed source candidates, without fr."""
    entry = project / 'sqlparse/__init__.py'
    source = entry.read_text()
    tree = ast.parse(source)
    names = ('parse', 'parsestream', 'format', 'split')
    functions = {node.name: node for node in tree.body
                 if isinstance(node, ast.FunctionDef) and node.name in names}
    require(set(functions) == set(names), 'the reserved entry points changed')

    def target(path, name):
        body = ast.parse((project / path).read_text()).body
        found = None
        for part in name.split('.'):
            matches = [node for node in body if isinstance(node, (ast.FunctionDef, ast.ClassDef))
                       and node.name == part]
            require(len(matches) == 1, 'source target is missing or ambiguous: ' + name)
            found, = matches
            body = found.body
        return {'path': path, 'name': name, 'line': found.lineno}

    targets = {
        'parsestream': target('sqlparse/__init__.py', 'parsestream'),
        'engine.FilterStack': target('sqlparse/engine/filter_stack.py', 'FilterStack'),
        'stack.enable_grouping': target('sqlparse/engine/filter_stack.py', 'FilterStack.enable_grouping'),
        'stack.run': target('sqlparse/engine/filter_stack.py', 'FilterStack.run'),
        'formatter.validate_options': target('sqlparse/formatter.py', 'validate_options'),
        'formatter.build_filter_stack': target('sqlparse/formatter.py', 'build_filter_stack'),
        'filters.SerializerUnicode': target('sqlparse/filters/others.py', 'SerializerUnicode'),
    }
    rows = []
    observed = set()
    for name, node in functions.items():
        for child in ast.walk(node):
            if isinstance(child, ast.Call):
                callee = ast.get_source_segment(source, child.func)
                observed.add(callee)
                rows.append({'caller': name, 'callee': callee, 'line': child.lineno,
                             'column': child.col_offset, 'source_target': targets.get(callee)})
    require(set(targets) <= observed, 'reviewed source candidates no longer match the reserved calls')
    return {'definitions': [{'name': name, 'line': functions[name].lineno} for name in sorted(names)],
            'calls': sorted(rows, key=lambda row: (row['caller'], row['line'], row['column'])),
            'runtime_dispatch_complete': False}


def private(operation, project, output, *arguments):
    result = common.command([sys.executable, '-B', str(ROOT / 'tools/agent_eval/product_private.py'),
                             operation, str(project.resolve()), *arguments], project, output)
    require(result['returncode'] == 0, 'independent behavior check failed: ' + str(output))
    return json.loads(result['stdout'])


def prepare(task, output):
    tasks = plan.prepare(ROOT, 'P5-initial')['requirements']['tasks']
    require(task in tasks and not output.exists(), 'prepare one initial task in a fresh output directory')
    output.mkdir(parents=True)
    requirement = tasks[task]
    project = output / 'project'
    if requirement['family'] != 'proof':
        original = download(requirement, output / 'upstream', ARCHIVE_LIMITS)
        shutil.copytree(original, project)
    else:
        (project / 'specs/FrSpecs').mkdir(parents=True)
        (project / PROOF).write_bytes((ROOT / plan.DIRECTORY / 'initial/fixtures/Clamp.lean').read_bytes())
        fixtures = ROOT / 'tests/agent-eval/proof-algorithms/fixtures'
        for name in ('lakefile.toml', 'lake-manifest.json', 'lean-toolchain'):
            (project / 'specs' / name).write_bytes((fixtures / name).read_bytes())
        (project / 'specs/FrSpecs.lean').write_text('import FrSpecs.Clamp\n')
    if task == 'utils-rename':
        require(not (project / 'unrelated.py').exists(), 'sentinel collides with original source')
        (project / 'unrelated.py').write_text("def to_int(value):\n    return 'unrelated:' + str(value)\n")
    elif task == 'sqlparse-graph':
        require(not (project / 'analysis.json').exists(), 'answer path collides with original source')
        common.save(project / 'analysis.json', {'definitions': [], 'calls': [], 'runtime_dispatch_complete': False})
    common.comparison_grade.BASE.git(project, 'init', '-q', '-b', 'main')
    (project / '.git/info/exclude').write_text('\n'.join(sorted((common.IGNORED - {'.git'}) | {'change.patch'})) + '\n')
    common.comparison_grade.BASE.initialize(project)
    record = {'task': task, 'requirement': requirement, 'original': common.sources(project)}
    if task == 'utils-rename':
        record['behavior'] = private('conversion', project, output / 'original-behavior.json', '--name', 'to_int')
        record['renamed'] = {}
        for path in record['original']:
            if path != 'unrelated.py' and Path(path).suffix in {'.py', '.rst', '.md'}:
                original = (project / path).read_bytes()
                renamed = re.sub(rb'\bto_int\b', b'parse_int', original)
                if renamed != original:
                    record['renamed'][path] = hashlib.sha256(renamed).hexdigest()
        require(bool(record['renamed']), 'the reserved API name was not found')
    elif task == 'sqlparse-graph':
        record['answer'] = graph_answer(project)
    elif task == 'slugify-bytes':
        record['behavior'] = private('slug-baseline', project, output / 'original-behavior.json')
        source = ast.parse((project / 'slugify/slugify.py').read_text())
        record['smart_truncate'] = [ast.dump(node) for node in source.body
                                   if isinstance(node, ast.ImportFrom) and any(a.name == 'smart_truncate' for a in node.names)]
    else:
        record['protected_source'] = masked((project / PROOF).read_text())
        built = common.command(['lake', 'build'], project / 'specs', output / 'original-build.json')
        require(built['returncode'] == 0, 'the unchanged executable proof fixture must build')
    common.save(output / 'project.json', record)
    common.retain_seed(project, record, output)
    return project, record


def boundaries(project, prepared):
    current, original = common.sources(project), prepared['original']
    changed = {path for path in current.keys() | original.keys() if current.get(path) != original.get(path)}
    require(all(path in current and current[path]['mode'] == entry['mode'] for path, entry in original.items()),
            'preserve original files and modes')
    task = prepared['task']
    if task == 'utils-rename':
        require(current.keys() == original.keys() and changed == prepared['renamed'].keys(),
                'update every API reference without changing unrelated files')
        require(all(current[path]['sha256'] == value for path, value in prepared['renamed'].items()),
                'preserve behavior and all bytes outside the requested name')
    elif task == 'sqlparse-graph':
        require(changed <= {'analysis.json'}, 'graph analysis must preserve all project source')
    elif task == 'slugify-bytes':
        allowed = {name for name in changed if name == 'slugify/slugify.py' or
                   (name not in original and (name.startswith('tests/') or name.startswith('test_')) and name.endswith('.py')) or
                   Path(name).suffix in {'.md', '.rst'}}
        require(changed == allowed, 'preserve legacy code, existing tests, dependencies and unrelated source')
        require('slugify/slugify.py' in changed and any(name.endswith(('.md', '.rst')) for name in changed) and
                any(name not in original and name.endswith('.py') for name in changed),
                'include implementation, API documentation and new tests')
        tree = ast.parse((project / 'slugify/slugify.py').read_text())
        imports = [ast.dump(node) for node in tree.body if isinstance(node, ast.ImportFrom) and
                   any(alias.name == 'smart_truncate' for alias in node.names)]
        require(imports == prepared['smart_truncate'], 'keep the public smart_truncate binding')
        functions = [node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == 'slugify']
        require(len(functions) == 1 and 'max_bytes' in [arg.arg for arg in functions[0].args.kwonlyargs],
                'max_bytes must be keyword-only')
    else:
        require(changed <= {PROOF} and masked((project / PROOF).read_text()) == prepared['protected_source'],
                'change only the marked proof; preserve its program, claim and settings')
    return sorted(changed)


def check(project, prepared, output):
    output.mkdir(parents=True, exist_ok=False)
    try:
        changed = boundaries(project, prepared)
        task = prepared['task']
        if task == 'utils-rename':
            require(private('conversion', project, output / 'behavior.json', '--name', 'parse_int') == prepared['behavior'],
                    'renamed API changed behavior')
        elif task == 'sqlparse-graph':
            actual = json.loads((project / 'analysis.json').read_text())
            # Canonical bytes also reject bools masquerading as integer offsets.
            require(plan.encoded(actual) == plan.encoded(prepared['answer']), 'incomplete or inaccurate source call inventory')
        elif task == 'slugify-bytes':
            baseline = output / 'baseline.json'
            common.save(baseline, prepared['behavior'])
            private('slug-check', project, output / 'behavior.json', '--baseline', str(baseline.resolve()))
        else:
            built = common.command(['lake', 'build', '--wfail'], project / 'specs', output / 'build.json')
            require(built['returncode'] == 0, 'Lean rejected the complete proof')
            audit = output / 'Audit.lean'
            audit.write_text('import FrSpecs.Clamp\n#print axioms Product.clamp\n#print axioms Product.clamp_bounds\n')
            checked = common.command(['lake', 'env', 'lean', '-j1', '-DwarningAsError=true', str(audit.resolve())],
                                     project / 'specs', output / 'axioms.json')
            require(checked['returncode'] == 0, 'Lean axiom audit failed')
            matches = re.findall(r"'Product\.(clamp|clamp_bounds)' (?:does not depend on any axioms|depends on axioms:\s*\[([^]]*)\])", checked['stdout'])
            require(len(matches) == 2 and {name for name, _ in matches} == {'clamp', 'clamp_bounds'} and
                    all({v.strip() for v in values.split(',') if v.strip()} <= common.APPROVED_AXIOMS for _, values in matches),
                    'proof depends on unapproved axioms or lacks a complete audit')
        return {'passed': True, 'changed_paths': changed}
    except (ValueError, AssertionError, SyntaxError, json.JSONDecodeError) as error:
        return {'passed': False, 'reason': str(error)}


def reference(project, prepared):
    """Qualification controls only; never call this when preparing a live seed."""
    task = prepared['task']
    if task == 'utils-rename':
        for name in prepared['renamed']:
            path = project / name
            path.write_bytes(re.sub(rb'\bto_int\b', b'parse_int', path.read_bytes()))
    elif task == 'sqlparse-graph':
        common.save(project / 'analysis.json', prepared['answer'])
    elif task == 'slugify-bytes':
        path = project / 'slugify/slugify.py'
        source = path.read_text()
        source = source.replace("    algorithm: Algorithm = 'legacy',\n", "    algorithm: Algorithm = 'legacy',\n    max_bytes: int = 0,\n")
        source = source.replace("    if algorithm not in ('legacy', 'modern'):\n",
            "    if isinstance(max_bytes, bool) or not isinstance(max_bytes, int):\n        raise TypeError('max_bytes must be an integer')\n"
            "    if max_bytes < 0 or (algorithm == 'legacy' and max_bytes):\n        raise ValueError('max_bytes requires modern mode and a nonnegative limit')\n"
            "    if algorithm not in ('legacy', 'modern'):\n")
        require(source.count('    return _modern_slugify(') == 1, 'reserved modern dispatch changed')
        source = source.replace('    return _modern_slugify(', '    result = _modern_slugify(')
        source += "    return result if max_bytes == 0 else result.encode('utf-8')[:max_bytes].decode('utf-8', errors='ignore')\n"
        path.write_text(source)
        (project / 'MAX_BYTES.md').write_text('In modern mode, max_bytes limits the final UTF-8 result. Zero disables it; legacy rejects nonzero limits.\n')
        (project / 'test_max_bytes.py').write_text("from slugify import slugify\n\ndef test_max_bytes():\n    assert slugify('日本', algorithm='modern', allow_unicode=True, max_bytes=4) == '日'\n")
    else:
        path = project / PROOF
        text = path.read_text()
        text = text.replace('  sorry\n',
            '  by_cases below : value < lower\n'
            '  · simp [clamp, below, Nat.not_lt_of_ge ordered, ordered]\n'
            '  · by_cases above : upper < value\n'
            '    · simp [clamp, below, above, Nat.not_lt_of_ge ordered, ordered]\n'
            '    · have lo : lower ≤ value := by omega\n'
            '      have hi : value ≤ upper := by omega\n'
            '      simp [clamp, below, above, lo, hi]\n')
        path.write_text(text)


def qualify(task, output):
    project, prepared = prepare(task, output)
    original = check(project, prepared, output / 'unchanged')
    require(not original['passed'], 'an unchanged task incorrectly passed')
    candidate = output / 'reference'
    common.checkout(project, candidate)
    reference(candidate, prepared)
    accepted = check(candidate, prepared, output / 'accepted')
    require(accepted['passed'], 'reference answer failed: ' + str(accepted))
    # Deliver exactly the candidate bytes to a fresh receiver, then reverse the
    # patch. This tests patch completeness independently of working-directory state.
    git = common.comparison_grade.BASE.git
    git(candidate, 'add', '-A')
    patch = git(candidate, 'diff', '--cached', '--binary')
    patch_path = output / 'reference.patch'
    patch_path.write_bytes(patch.stdout)
    receiver = output / 'receiver'
    common.checkout(project, receiver)
    git(receiver, 'apply', str(patch_path.resolve()))
    replayed = check(receiver, prepared, output / 'replayed')
    require(replayed['passed'] and common.sources(receiver) == common.sources(candidate), 'delivered patch differs')
    git(receiver, 'apply', '--reverse', str(patch_path.resolve()))
    require(common.sources(receiver) == prepared['original'], 'reverse patch changed original source')
    git(receiver, 'apply', str(patch_path.resolve()))
    require(common.sources(receiver) == common.sources(candidate), 'patch replay is not reproducible')
    controls = {'unchanged': original, 'reference': accepted, 'fresh_receiver': replayed}
    mutations = {}
    if task == 'utils-rename':
        mutations = {'compatibility-alias': ('python_utils/converters.py', lambda text: text + '\nto_int = parse_int\n'),
                     'unrelated-name': ('unrelated.py', lambda text: text.replace('to_int', 'parse_int')),
                     'missed-public-export': ('python_utils/__init__.py', lambda text: text.replace('parse_int', 'to_int'))}
    elif task == 'sqlparse-graph':
        for kind in ('missing-call', 'wrong-target', 'wrong-column', 'runtime-overclaim'):
            answer = copy.deepcopy(prepared['answer'])
            if kind == 'missing-call':
                answer['calls'].pop()
            elif kind == 'wrong-target':
                next(row for row in answer['calls'] if row['source_target'])['source_target'] = None
            elif kind == 'wrong-column':
                answer['calls'][0]['column'] += 1
            else:
                answer['runtime_dispatch_complete'] = True
            mutations[kind] = ('analysis.json', lambda _, answer=answer: json.dumps(answer))
    elif task == 'slugify-bytes':
        mutations = {'character-budget': ('slugify/slugify.py', lambda text: text.replace("result.encode('utf-8')[:max_bytes].decode('utf-8', errors='ignore')", 'result[:max_bytes]')),
                     'accept-bool': ('slugify/slugify.py', lambda text: text.replace('isinstance(max_bytes, bool) or ', '')),
                     'legacy-drift': ('slugify/_legacy.py', lambda text: text + '\n# forbidden legacy edit\n')}
    else:
        mutations = {'unfinished': (PROOF, lambda _: (project / PROOF).read_text()),
                     'changed-program': (PROOF, lambda text: text.replace('then lower else', 'then upper else')),
                     'changed-claim': (PROOF, lambda text: text.replace('lower ≤ clamp lower upper value', '0 ≤ clamp lower upper value'))}
    for name, (path, transform) in mutations.items():
        bad = output / name
        common.checkout(project, bad)
        reference(bad, prepared)
        source = bad / path
        before = source.read_text()
        after = transform(before)
        require(after != before, 'negative control did not change its intended input')
        source.write_text(after)
        verdict = check(bad, prepared, output / (name + '-check'))
        require(not verdict['passed'], 'negative control passed: ' + name)
        controls[name] = verdict
        git(bad, 'add', '-A')
        (output / (name + '.patch')).write_bytes(git(bad, 'diff', '--cached', '--binary').stdout)
    result = {'schema': 'fr-product-task-controls-1', 'task': task, 'passed': True, 'live_calls': 0,
              'seed_sha256': common.digest(output / 'original.tar.gz'), 'controls': controls}
    common.save(output / 'controls.json', result)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('operation', choices=('prepare', 'qualify'))
    parser.add_argument('task', choices=plan.TASKS['initial'])
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    if args.operation == 'qualify':
        print(json.dumps(qualify(args.task, args.output.resolve()), indent=2))
    else:
        prepare(args.task, args.output.resolve())


if __name__ == '__main__':
    main()
