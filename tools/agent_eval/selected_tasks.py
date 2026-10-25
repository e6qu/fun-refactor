"""Prepare identical M1 projects and independently check delivered changes."""
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile

from .selected_plan import DIRECTORY, ROOT, require
from . import comparison_grade

sys.path.insert(0, str(ROOT / 'tools'))
from project_change_sources import unpack

TASKS = json.loads((ROOT / DIRECTORY / 'tasks.json').read_text())['tasks']
PROOF_PATH = 'specs/FrSpecs/SrcLibRsAllowed.lean'
OBLIGATION = 'allowedModel_access_rule'
PROOF_TARGET = PROOF_PATH + '::' + OBLIGATION
UNRELATED = 'unrelated.py'
IGNORED = {'.git', '.fr', '.fr-history', '.lake', 'target', '__pycache__', '.pytest_cache', '.agents', '.claude', '.opencode', 'artifacts'}
APPROVED_AXIOMS = {'propext', 'Classical.choice', 'Quot.sound'}


def digest(path):
    with path.open('rb') as source:
        return hashlib.file_digest(source, 'sha256').hexdigest()


def save(path, value):
    path.write_text(json.dumps(value, indent=2, ensure_ascii=False, allow_nan=False) + '\n')


def command(argv, root, output):
    result = subprocess.run(argv, cwd=root, capture_output=True, text=True, timeout=120,
        env={**os.environ, 'PYTHONDONTWRITEBYTECODE': '1', 'CARGO_NET_OFFLINE': 'true',
             'CARGO_BUILD_JOBS': '1', 'CARGO_INCREMENTAL': '0', 'CARGO_PROFILE_DEV_DEBUG': '0',
             'CARGO_PROFILE_TEST_DEBUG': '0', 'RUST_TEST_THREADS': '1'})
    require(len(result.stdout.encode()) + len(result.stderr.encode()) <= 1024**2, 'check output exceeds 1 MiB')
    value = {'argv': list(map(str, argv)), 'returncode': result.returncode,
             'stdout': result.stdout, 'stderr': result.stderr}
    save(output, value)
    return value


def fr(binary, project, arguments, output):
    result = command([str(binary), '-C', str(project), '--json', *arguments], project, output)
    require(result['returncode'] == 0, 'fr refused setup or a prescribed control; inspect ' + str(output))
    return json.loads(result['stdout'])


def sources(root):
    result = {}
    for directory, folders, files in os.walk(root, followlinks=False):
        folders[:] = [name for name in folders if name not in IGNORED]
        require(all(not (Path(directory) / name).is_symlink() for name in folders),
                'source directories must not be symlinks')
        for name in files:
            path = Path(directory) / name
            if name == 'change.patch':
                continue
            require(not path.is_symlink() and path.is_file(), 'source must be a regular file')
            result[str(path.relative_to(root))] = {'sha256': digest(path), 'mode': path.stat().st_mode & 0o777}
    require(len(result) <= 4096, 'project source file count exceeds its declared scope')
    return result


def masked_proof(text):
    begin, end = '-- fr:proof-begin ' + OBLIGATION, '-- fr:proof-end ' + OBLIGATION
    require(text.count(begin) == text.count(end) == 1, 'proof needs its one original marker pair')
    start = text.index('\n', text.index(begin)) + 1
    finish = text.rfind('\n', 0, text.index(end)) + 1
    require(start < finish, 'proof region is not ordered')
    return text[:start] + '<M1 proof steps>\n' + text[finish:]


def prepare(task, project, binary, output):
    require(task in TASKS and not project.exists(), 'prepare one named task in a new directory')
    output.mkdir(parents=True, exist_ok=False)
    if task == 'boltons-rename':
        archive = ROOT / TASKS[task]['archive']
        require(digest(archive) == TASKS[task]['archive_sha256'], 'Boltons archive changed')
        unpack(archive, project, {'archive_bytes': 8388608, 'expanded_bytes': 33554432, 'archive_members': 4096})
        require(not (project / UNRELATED).exists(), 'unrelated-name fixture collides with upstream source')
        (project / UNRELATED).write_text('def clamp(value):\n    return value + 101\n\ndef example():\n    return clamp(3)\n')
    elif task == 'unicode-dice':
        comparison_grade.BASE.unpack(project, task)
        generated = command(['cargo', 'generate-lockfile', '--offline'], project, output / 'lockfile.json')
        require(generated['returncode'] == 0, 'could not freeze the original dependency graph')
    else:
        project.mkdir(parents=True)
        (project / 'src').mkdir()
        (project / 'src/lib.rs').write_text(TASKS[task]['source'])
        (project / 'Cargo.toml').write_text('[package]\nname="m1_access_rule"\nversion="0.0.0"\nedition="2024"\n\n[workspace]\n')
        fr(binary, project, ['spec', 'init', 'specs', '--write'], output / 'init.json')
        shape = fr(binary, project, ['spec', 'property-task', 'src/lib.rs::allowed', '--token-limit', '4096'], output / 'property-task.json')
        variable = lambda name: {'kind': 'variable', 'name': name}
        property_ = {'schema': 'fr-formal-property-1', 'task_digest': shape['object_digest'], 'name': 'access_rule',
            'parameters': [{'name': name, 'lean_type': 'Bool'} for name in ('staff', 'owner')],
            'proposition': {'kind': 'equals',
                'left': {'kind': 'model', 'arguments': [variable('staff'), variable('owner')]},
                'right': {'kind': 'binary', 'operator': 'or', 'left': variable('staff'), 'right': variable('owner')}}}
        path = output / 'property.json'
        save(path, property_)
        planned = fr(binary, project, ['spec', 'plan', 'src/lib.rs::allowed', '--property-from', str(path)], output / 'plan.json')
        plan_path = output / 'formal-plan.json'
        save(plan_path, planned)
        fr(binary, project, ['spec', 'scaffold', '--from', str(plan_path), '--package', 'specs', '--write'], output / 'scaffold.json')
    comparison_grade.BASE.git(project, 'init', '-q', '-b', 'main')
    (project / '.git/info/exclude').write_text('\n'.join(sorted((IGNORED - {'.git'}) | {'change.patch'})) + '\n')
    comparison_grade.BASE.initialize(project)
    before = sources(project)
    result = {'task': task, 'original': before, 'requirement': TASKS[task],
              'rename_paths': [path for path in before if path != UNRELATED and
                  Path(path).suffix in {'.py', '.rst', '.md'} and
                  re.search(r'\bclamp\b', (project / path).read_text()) is not None]
                  if task == 'boltons-rename' else [],
              'protected_proof': masked_proof((project / PROOF_PATH).read_text()) if task == 'access-proof' else None,
              'scope': 'Identical source/model/claim for both setups; no proof steps or repair supplied to a live attempt.'}
    save(output / 'project.json', result)
    return result


def unchanged_boundaries(task, project, prepared):
    current, original = sources(project), prepared['original']
    changed = {path for path in current.keys() | original.keys() if current.get(path) != original.get(path)}
    if task == 'unicode-dice':
        require(changed <= {'src/lib.rs'}, 'Unicode task changed source outside its declared implementation')
    elif task == 'access-proof':
        require(changed <= {PROOF_PATH}, 'proof task changed unrelated source or package settings')
        require((project / PROOF_PATH).is_file(), 'proof file is missing')
        require(masked_proof((project / PROOF_PATH).read_text()) == prepared['protected_proof'],
                'proof changed its definitions, claim, assumptions or unrelated regions')
    else:
        require(UNRELATED not in changed, 'rename changed the unrelated clamp declaration or caller')
        permitted = set(prepared['rename_paths'])
        require(changed <= permitted, 'rename changed files outside its declared consumers: ' +
                ', '.join(sorted(changed - permitted)))
    return sorted(changed)


def check(task, project, prepared, binary, output):
    output.mkdir(parents=True, exist_ok=False)
    try:
        changed = unchanged_boundaries(task, project, prepared)
    except ValueError as error:
        return {'passed': False, 'stage': 'source boundaries', 'reason': str(error)}
    if task == 'unicode-dice':
        result = comparison_grade.grade(project, task)
        save(output / 'result.json', result)
        return {**result, 'changed_paths': changed}
    if task == 'boltons-rename':
        oracle = ROOT / 'tests/agent-eval/project-changes/oracle.py'
        commands = [[sys.executable, '-B', str(oracle), 'behavior', 'boltons', str(project), '--name', 'clamp_value'],
                    [sys.executable, '-B', str(oracle), 'documentation', 'boltons', str(project)],
                    [sys.executable, '-B', '-m', 'doctest', 'boltons/mathutils.py'],
                    [sys.executable, '-B', '-m', 'pytest', '-q', '-p', 'no:cacheprovider', 'tests/test_mathutils.py']]
        checks = [command(argv, project, output / f'check-{index}.json') for index, argv in enumerate(commands)]
        # Run a private copy of the original tests so deleting or weakening candidate tests cannot pass.
        archive = ROOT / TASKS[task]['archive']
        with tempfile.TemporaryDirectory(prefix='m1-original-tests-') as temporary:
            original = Path(temporary) / 'source'
            unpack(archive, original, {'archive_bytes': 8388608, 'expanded_bytes': 33554432, 'archive_members': 4096})
            test = Path(temporary) / 'test_mathutils_original.py'
            test.write_text(re.sub(r'\bclamp\b', 'clamp_value', (original / 'tests/test_mathutils.py').read_text()))
            driver = ('import sys,pytest;sys.path.insert(0,sys.argv[1]);'
                      'raise SystemExit(pytest.main(["-q","-p","no:cacheprovider",sys.argv[2]]))')
            checks.append(command([sys.executable, '-B', '-c', driver, str(project), str(test)],
                                  project, output / 'original-tests.json'))
        return {'passed': all(row['returncode'] == 0 for row in checks), 'checks': checks,
                'changed_paths': changed, 'unrelated_name_unchanged': True}
    table = output / 'truth_table.rs'
    table.write_text((project / 'src/lib.rs').read_text() + '\nfn main() {\n'
        '    for (staff, owner, expected) in [(false,false,false), (false,true,true),\n'
        '                                    (true,false,true), (true,true,true)] {\n'
        '        assert_eq!(allowed(staff, owner), expected);\n'
        '    }\n}\n')
    executable = output / 'truth_table'
    compiled = command(['rustc', '--edition=2024', str(table), '-o', str(executable)],
                       project, output / 'truth-table-compile.json')
    require(compiled['returncode'] == 0, 'could not compile the independent source truth-table check')
    truth = command([str(executable)], project, output / 'truth-table.json')
    require(truth['returncode'] == 0, 'source access rule disagrees with its four required input cases')
    strict = command([str(binary), '-C', str(project), '--json', 'spec', 'check', 'specs', '--strict', '--max-debt', '0'],
                     project, output / 'strict.json')
    if strict['returncode'] != 0:
        return {'passed': False, 'stage': 'strict proof check', 'checks': [strict]}
    evidence = command([str(binary), '-C', str(project), '--json', 'spec', 'evidence', 'specs'],
                       project, output / 'evidence.json')
    if evidence['returncode'] != 0:
        return {'passed': False, 'stage': 'Lean package check', 'checks': [strict, evidence]}
    proof = json.loads(evidence['stdout'])
    require(proof['correspondence']['proved_implementation_model'] is False, 'model proof became a source implementation claim')
    accepted = any(row['name'].endswith(OBLIGATION) and row['status'] == 'checked_by_lean' for row in proof['properties'])
    audit = output / 'Axioms.lean'
    audit.write_text('import FrSpecs.SrcLibRsAllowed\n#print axioms FrSpecs.' + OBLIGATION + '\n')
    axioms = command(['lake', 'env', 'lean', str(audit)], project / 'specs', output / 'axioms.json')
    names = []
    if 'does not depend on any axioms' not in axioms['stdout']:
        matched = re.search(r'depends on axioms:\s*\[([^]]*)\]', axioms['stdout'])
        require(matched is not None, 'could not interpret Lean axiom dependencies')
        names = [name.strip() for name in matched[1].split(',') if name.strip()]
    return {'passed': accepted and axioms['returncode'] == 0 and set(names) <= APPROVED_AXIOMS,
            'changed_paths': changed, 'model_proof_checked': accepted, 'axioms': names,
            'source_implementation_proved': False, 'checks': [compiled, truth, strict, evidence, axioms]}


def prescribed_change(task, variant, project, binary, output):
    """Controller-supplied answers for controls; never used to prompt a live agent."""
    if task == 'access-proof':
        tactics = output / 'tactics.lean'
        tactics.write_text('rfl\n' if variant == 'reference' else 'cases staff\n')
        before = sources(project)
        checked = fr(binary, project, ['spec', 'proof-check', PROOF_TARGET, '--from', str(tactics)],
                     output / 'proof-check.json')
        require(checked['passed'] == (variant == 'reference'), 'unexpected prescribed tactic outcome')
        require(sources(project) == before, 'proof-check changed project source')
        if variant == 'reference':
            fr(binary, project, ['spec', 'prove', PROOF_TARGET, '--from', str(tactics)], output / 'proof-preview.json')
            fr(binary, project, ['spec', 'prove', PROOF_TARGET, '--from', str(tactics), '--write'], output / 'proof-write.json')
        return
    if task == 'boltons-rename':
        name, path = 'clamp', 'boltons/mathutils.py'
    else:
        name, path = 'sorensen_dice', 'src/lib.rs'
    found = fr(binary, project, ['project', 'find', name, '--in', path, '--source', '--bytes', '65536'],
               output / 'discovery.json')
    require(found['page']['next'] is None, 'control discovery is incomplete')
    rows = [dict(zip(found['columns'], row)) for row in found['rows']]
    require(len(rows) == 1 and rows[0]['source']['next_offset'] is None, 'control needs one complete declaration')
    fragment = output / 'fragment.txt'
    if task == 'boltons-rename':
        fragment.write_text(json.dumps({'operation': 'rename', 'name': 'clamp_value', 'documentation': 'update'}))
        operation = 'refactor'
    else:
        _, _, old, new = comparison_grade.replacement(task, project)[0]
        if variant == 'incomplete':
            new = old.replace('a.len()', 'a.chars().count()')
        fragment.write_text(new[new.index('{'):])
        operation = 'replace-body'
    manifest = output / 'change.json'
    save(manifest, {'operations': [{'op': operation, 'handle': rows[0]['handle'], 'from': str(fragment)}]})
    preview = fr(binary, project, ['author', 'batch', '--from', str(manifest)], output / 'preview.json')
    saved = fr(binary, project, ['author', 'batch', '--from', str(manifest), '--save-plan',
        '--plan-basis', preview['plan_context_basis']], output / 'saved.json')
    fr(binary, project, ['history', 'apply', str(saved['transaction']), '--write', '--no-diff',
        '--context-basis', saved['transaction_context_basis']], output / 'applied.json')
    if task == 'boltons-rename' and variant == 'reference':
        doc = fr(binary, project, ['file', 'replace', 'docs/mathutils.rst',
            '--before', '.. autofunction:: boltons.mathutils.clamp',
            '--after', '.. autofunction:: boltons.mathutils.clamp_value', '--matches', '1', '--save-plan'],
            output / 'documentation-preview.json')
        fr(binary, project, ['history', 'apply', str(doc['transaction']), '--write'], output / 'documentation-applied.json')


def controls(task, binary, output):
    require(os.environ.get('GITHUB_ACTIONS') == 'true', 'run task controls on GitHub under the workstation rules')
    output.mkdir(parents=True, exist_ok=False)
    original = output / 'original'
    prepared = prepare(task, original, binary, output / 'preparation')
    results = {}
    try:
        for variant in ('unchanged', 'incomplete', 'reference'):
            project = output / variant
            shutil.copytree(original, project)
            actions = output / (variant + '-actions')
            actions.mkdir()
            if variant != 'unchanged':
                prescribed_change(task, variant, project, binary, actions)
            results[variant] = check(task, project, prepared, binary, output / (variant + '-checks'))
            save(output / (variant + '.json'), results[variant])
            require(results[variant]['passed'] == (variant == 'reference'), f'unexpected {variant} grading outcome')
            if task == 'unicode-dice' and variant != 'reference':
                require(all(row['exit_code'] == 0 for row in results[variant]['checks']), 'negative control failed upstream checks')
                require(results[variant]['oracle'].get('stage') == 2, 'negative control did not reach independent behavior checks')
                if variant == 'incomplete':
                    require('a="aa" b="aaé": 0.5 != 0.6666666666666666' in results[variant]['oracle']['detail'],
                            'incomplete control failed for an unexpected reason')
            if task == 'boltons-rename' and variant == 'incomplete':
                require(results[variant]['checks'][0]['returncode'] == 0 and
                        results[variant]['checks'][1]['returncode'] != 0,
                        'incomplete rename must retain behavior but fail documentation resolution')
            if task == 'access-proof' and variant != 'reference':
                require(results[variant]['stage'] == 'strict proof check', 'unfinished proof failed for an unexpected reason')
        reference = output / 'reference'
        patch = comparison_grade.BASE.git(reference, 'diff', '--binary', 'HEAD').stdout
        require(patch, 'reference produced no deliverable change')
        (output / 'change.patch').write_bytes(patch)
        receiver = output / 'receiver'
        shutil.copytree(original, receiver)
        comparison_grade.BASE.git(receiver, 'apply', '--check', '-', data=patch)
        comparison_grade.BASE.git(receiver, 'apply', '-', data=patch)
        require(sources(receiver) == sources(reference), 'receiver differs from the delivered source')
        results['receiver'] = check(task, receiver, prepared, binary, output / 'receiver-checks')
        require(results['receiver']['passed'], 'delivered patch failed independent checks in a new checkout')
        delivered = sources(receiver)
        comparison_grade.BASE.git(receiver, 'apply', '--reverse', '-', data=patch)
        require(sources(receiver) == prepared['original'], 'patch undo did not restore exact original bytes and modes')
        comparison_grade.BASE.git(receiver, 'apply', '-', data=patch)
        require(sources(receiver) == delivered, 'patch redo did not restore exact delivered bytes and modes')
        if task == 'access-proof':
            path = receiver / PROOF_PATH
            text = path.read_text()
            declaration, theorem = text.split('theorem ', 1)
            require(declaration.count('||') == 1, 'false-model control needs one model disjunction')
            path.write_text(declaration.replace('||', '&&') + 'theorem ' + theorem)
            false_model = command(['lake', 'build'], receiver / 'specs', output / 'false-model-lean.json')
            require(false_model['returncode'] != 0 and 'type mismatch' in
                    (false_model['stdout'] + false_model['stderr']), 'Lean did not reject the false model for the fixed property')
            results['false_model'] = check(task, receiver, prepared, binary, output / 'false-model-checks')
            require(results['false_model']['stage'] == 'source boundaries', 'grader accepted changed model definitions')
        result = {'task': task, 'passed': True, 'live_model_calls': 0, 'controls': results,
                  'patch_sha256': digest(output / 'change.patch'), 'undo_exact': True, 'redo_exact': True,
                  'scope': 'Supplied control answers on familiar tasks. These are not live agent outcomes.'}
        save(output / 'result.json', result)
        return result
    finally:
        save(output / 'completed-controls.json', results)
        for name in ('original', 'unchanged', 'incomplete', 'reference', 'receiver'):
            shutil.rmtree(output / name, ignore_errors=True)


if __name__ == '__main__':
    import argparse
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--task', choices=list(TASKS), required=True)
    parser.add_argument('--fr', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    print(json.dumps(controls(args.task, args.fr.resolve(), args.output.resolve())))
