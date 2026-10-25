"""Prepare M2's frozen projects and check their changes with separate oracles."""
import ast
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile

from . import regex_escape_len, regex_workspace
from . import selected_tasks as common
from .selected_plan import ROOT, require

DIRECTORY = Path('tests/agent-eval/project-edits')
TASKS = json.loads((ROOT / DIRECTORY / 'tasks.json').read_text())['tasks']


def digest_bytes(value):
    return hashlib.sha256(value).hexdigest()


def rust_arguments(text):
    return re.sub(r'(\btotal\()([^()]*)\)', r'\1<M2 argument list>)', text)


def python_shape(text):
    tree = ast.parse(text)
    targets = [node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == 'invoice_total']
    require(len(targets) == 1, 'keep one public invoice_total function')
    return [{'kind': type(node).__name__, 'name': node.name if isinstance(node,
             (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)) else None} for node in tree.body]


def test_tail(text):
    start = re.search(r'#\[cfg\(test\)\]\s*(?:pub\s+)?mod\s+tests\b', text)
    return text[start.start():] if start else None


def copy_fixture(name, project):
    source = ROOT / DIRECTORY / 'fixtures' / name
    for path, entry in common.sources(source).items():
        destination = project / path
        require(not destination.exists(), 'fixture overlay collides with the pinned source: ' + path)
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes((source / path).read_bytes())
        destination.chmod(entry['mode'])


def prepare(task, project, output):
    require(task in TASKS and not project.exists(), 'prepare one explicit M2 task in a fresh directory')
    output.mkdir(parents=True, exist_ok=False)
    if task == 'regex-escape-len':
        require(common.digest(ROOT / TASKS[task]['archive']) == TASKS[task]['archive_sha256'] and
                common.digest(ROOT / TASKS[task]['lock']) == TASKS[task]['lock_sha256'],
                'M2 regex source or dependency lock differs from its frozen requirement')
        regex_workspace.unpack(project)
        copy_fixture(TASKS[task]['fixture_overlay'], project)
    else:
        project.mkdir(parents=True)
        copy_fixture(TASKS[task]['fixture'], project)
    common.comparison_grade.BASE.git(project, 'init', '-q', '-b', 'main')
    (project / '.git/info/exclude').write_text('\n'.join(sorted((common.IGNORED - {'.git'}) | {'change.patch'})) + '\n')
    common.comparison_grade.BASE.initialize(project)
    original = common.sources(project)
    protected = {}
    if task == 'rust-parameter':
        protected = {path: digest_bytes(rust_arguments((project / path).read_text()).encode())
                     for path in original if path.endswith('.rs')}
    elif task == 'typescript-alias':
        replacements = {'src/pricing.ts': ('function price(', 'function quote('),
                        'src/checkout.ts': ('price as displayPrice', 'quote as displayPrice')}
        for path, (before, after) in replacements.items():
            text = (project / path).read_text()
            require(text.count(before) == 1, 'TypeScript fixture needs one intended rename site')
            protected[path] = digest_bytes(text.replace(before, after).encode())
    elif task == 'python-invoice':
        text = (project / 'invoice.py').read_text()
        before, body, after = text.partition('    return amount\n')
        require(body and text.count(body) == 1, 'invoice fixture needs its one original return')
        protected = {'prefix': before, 'suffix': after, 'shape': python_shape(text)}
    else:
        for path in regex_escape_len.PATHS:
            tail = test_tail((project / path).read_text())
            if tail is not None:
                protected[path] = tail
    record = {'task': task, 'original': original, 'protected': protected, 'requirement': TASKS[task]}
    common.save(output / 'project.json', record)
    common.retain_seed(project, record, output)
    return record


def boundaries(task, project, prepared):
    original, current = prepared['original'], common.sources(project)
    changed = {path for path in original.keys() | current.keys() if original.get(path) != current.get(path)}
    require(all(path in current and current[path]['mode'] == entry['mode'] for path, entry in original.items()),
            'preserve original files and executable modes')
    if task == 'rust-parameter':
        require(current.keys() == original.keys() and all(path.endswith('.rs') for path in changed),
                'parameter removal changed configuration or unrelated files')
        require(all(digest_bytes(rust_arguments((project / path).read_text()).encode()) == expected
                    for path, expected in prepared['protected'].items()),
                'preserve exact bytes outside the parameter and argument lists')
    elif task == 'typescript-alias':
        permitted = prepared['protected']
        require(current.keys() == original.keys() and changed <= permitted.keys(),
                'rename changed unrelated source or project settings')
        require(all(current[path]['sha256'] == expected for path, expected in permitted.items()),
                'rename the intended export/import names and preserve aliases and other bytes')
    elif task == 'python-invoice':
        require(current.keys() == original.keys() and changed <= {'invoice.py'},
                'invoice repair changed tests or unrelated source')
        text, expected = (project / 'invoice.py').read_text(), prepared['protected']
        require(text.startswith(expected['prefix']) and text.endswith(expected['suffix']) and
                python_shape(text) == expected['shape'], 'preserve the signature, unrelated code and surrounding bytes')
    elif task == 'regex-escape-len':
        require(all(path.endswith('.rs') and path.startswith(('src/', 'regex-syntax/src/')) for path in changed),
                'feature changed manifests, dependency locks, examples, tests or unrelated crates')
        require(all(tail in (project / path).read_text() for path, tail in prepared['protected'].items()),
                'preserve the original inline test programs')
    else:
        raise ValueError('unknown M2 task')
    return sorted(changed)


def public_checks(task, tools):
    if task == 'rust-parameter':
        return [{'name': 'project', 'argv': [tools['cargo'], 'test', '--workspace', '--locked', '--offline']}]
    if task == 'typescript-alias':
        return [{'name': 'types', 'argv': [tools['node'], tools['typescript'], '--project', 'tsconfig.json', '--pretty', 'false']},
                {'name': 'behavior', 'argv': [tools['node'], 'check.cjs']}]
    if task == 'python-invoice':
        return [{'name': 'project', 'argv': [tools['python'], '-B', '-m', 'pytest', '-q', 'test_invoice.py', '-p', 'no:cacheprovider']}]
    require(task == 'regex-escape-len', 'choose an explicit M2 task for public checks')
    return [{'name': 'api-examples', 'argv': [tools['cargo'], 'check', '-p', 'regex', '--example',
            'required_escape_len', '--locked', '--offline']}]


RUST_ORACLE = '''
fn main() {
    for amount in -50i64..=50 {
        for fee in -9i64..=9 {
            assert_eq!(pricing::total(amount, fee), amount + fee);
        }
        assert_eq!(pricing::same_crate(amount), amount + 2);
        assert_eq!(pricing::unrelated(amount), amount + 101);
        assert_eq!(checkout::charge(amount), amount + 3);
        assert_eq!(checkout::combined(amount), 2 * amount + 6);
    }
    println!("independent Rust API and caller results passed");
}
'''

PYTHON_ORACLE = '''
import runpy, sys
module = runpy.run_path(sys.argv[1])
for amount in range(-50, 51):
    for fee in range(-9, 10):
        assert module['invoice_total'](amount, fee) == amount + fee, (amount, fee)
    assert module['label'](amount) == f'invoice:{amount}'
print('independent invoice results passed')
'''

TYPESCRIPT_ORACLE = '''
const assert = require('node:assert/strict');
const path = require('node:path');
const root = process.argv[2];
const pricing = require(path.join(root, 'target/js/pricing.js'));
const checkout = require(path.join(root, 'target/js/checkout.js'));
const unrelated = require(path.join(root, 'target/js/unrelated.js'));
assert.equal(Object.hasOwn(pricing, 'price'), false);
assert.equal(typeof pricing.quote, 'function');
for (let amount = -50; amount <= 50; amount++) {
    assert.equal(pricing.quote(amount), amount + 2);
    assert.equal(checkout.receipt(amount), amount + 2);
    assert.equal(unrelated.price(amount), amount + 100);
    assert.equal(unrelated.localReceipt(amount), amount + 100);
}
console.log('independent TypeScript export and alias results passed');
'''


def check(task, project, prepared, tools, output):
    output.mkdir(parents=True, exist_ok=False)
    result = {'task': task, 'passed': False, 'stage': 'source boundaries', 'checks': []}
    try:
        result['changed'] = boundaries(task, project, prepared)
        before_checks = common.sources(project)
        with tempfile.TemporaryDirectory(prefix='fr-m2-oracle-') as temporary:
            temporary = Path(temporary)
            commands = []
            if task == 'rust-parameter':
                commands += [[tools['cargo'], 'build', '--workspace', '--locked', '--offline'],
                             public_checks(task, tools)[0]['argv']]
                source, binary = temporary / 'results.rs', temporary / 'results'
                source.write_text(RUST_ORACLE)
                commands += [[tools['rustc'], '--edition=2024', str(source),
                    '--extern', 'pricing=' + str(project / 'target/debug/libpricing.rlib'),
                    '--extern', 'checkout=' + str(project / 'target/debug/libcheckout.rlib'),
                    '-L', 'dependency=' + str(project / 'target/debug/deps'), '-o', str(binary)], [str(binary)]]
            elif task == 'typescript-alias':
                source = temporary / 'results.cjs'
                source.write_text(TYPESCRIPT_ORACLE)
                commands = [row['argv'] for row in public_checks(task, tools)] + [[tools['node'], str(source), str(project)]]
            elif task == 'python-invoice':
                source = temporary / 'results.py'
                source.write_text(PYTHON_ORACLE)
                commands = [[tools['python'], '-I', '-B', str(source), str(project / 'invoice.py')],
                            public_checks(task, tools)[0]['argv']]
            elif task == 'regex-escape-len':
                result['stage'] = 'independent byte and allocation oracle'
                checked = regex_escape_len.verify(project, cargo=tools['cargo'], rustc=tools['rustc'],
                    cargo_home=ROOT / 'target/cargo-home')
                result['checks'].append(checked)
                if not checked['passed']:
                    return result
                commands = [public_checks(task, tools)[0]['argv'],
                    [tools['cargo'], 'test', '-p', 'regex', '-p', 'regex-syntax', '--lib', '--locked', '--offline'],
                    [tools['cargo'], 'check', '-p', 'regex', '--no-default-features', '--locked', '--offline'],
                    [tools['cargo'], 'check', '-p', 'regex-syntax', '--no-default-features', '--locked', '--offline']]
            else:
                raise ValueError('unknown M2 task')
            result['stage'] = 'compiler and behavior checks'
            for index, argv in enumerate(commands):
                checked = common.command(argv, project, output / f'command-{index}.json')
                result['checks'].append({'argv': list(map(str, argv)), 'returncode': checked['returncode']})
                if checked['returncode'] != 0:
                    return result
        require(common.sources(project) == before_checks, 'a check changed source during validation')
        result.update(passed=True, stage='complete')
        return result
    except (ValueError, SyntaxError, UnicodeError, subprocess.TimeoutExpired) as error:
        result['reason'] = type(error).__name__ + ': ' + str(error)
        return result
    finally:
        common.save(output / 'result.json', result)


def prescribed_change(task, variant, project):
    require(task in TASKS and variant in {'reference', 'incomplete'}, 'choose one explicit supplied M2 control')
    if task == 'rust-parameter':
        for path in project.rglob('*.rs'):
            if 'target' in path.relative_to(project).parts:
                continue
            text = path.read_text().replace(', _verbose: bool', '')
            if variant == 'reference':
                text = text.replace(', true, ', ', ').replace(', false, ', ', ')
            path.write_text(text)
    elif task == 'typescript-alias':
        path = project / 'src/pricing.ts'
        path.write_text(path.read_text().replace('function price(', 'function quote('))
        if variant == 'reference':
            path = project / 'src/checkout.ts'
            path.write_text(path.read_text().replace('price as displayPrice', 'quote as displayPrice'))
    elif task == 'python-invoice':
        path = project / 'invoice.py'
        path.write_text(path.read_text().replace('return amount\n',
            'return amount + fee\n' if variant == 'reference' else 'return 7\n'))
    else:
        path = project / 'regex-syntax/src/lib.rs'
        text = path.read_text()
        old = 'pub fn escape(text: &str) -> String {\n    let mut quoted = String::new();'
        require(text.count(old) == 1, 'reference control needs the original escape implementation')
        text = text.replace(old, 'pub fn escape(text: &str) -> String {\n    let mut quoted = String::with_capacity(escape_len(text));')
        expression = ('text.chars().map(|c| c.len_utf8() + usize::from(is_meta_character(c))).sum()'
                      if variant == 'reference' else 'text.len()')
        path.write_text(text + '\n/// Returns the escaped UTF-8 byte length without allocating.\n'
            'pub fn escape_len(text: &str) -> usize {\n    ' + expression + '\n}\n')
        path = project / 'src/lib.rs'
        path.write_text(path.read_text() + '\n/// Returns the escaped UTF-8 byte length without allocating.\n'
            'pub fn escape_len(text: &str) -> usize { regex_syntax::escape_len(text) }\n')


def controls(task, inputs, tools, output):
    require(os.environ.get('GITHUB_ACTIONS') == 'true', 'run compiled M2 acceptance controls on GitHub')
    output.mkdir(parents=True, exist_ok=False)
    original = inputs / 'original'
    prepared = json.loads((inputs / 'preparation/project.json').read_text())
    require(prepared['task'] == task and prepared['requirement'] == TASKS[task] and
            common.sources(original) == prepared['original'], 'control inputs differ from the prepared task')
    summary = {'passed': False, 'task': task, 'live_model_calls': 0, 'variants': {},
               'scope': 'Supplied correct and incorrect changes, not agent outcomes.'}
    try:
        for variant in ('unchanged', 'incomplete', 'reference'):
            with tempfile.TemporaryDirectory(prefix='fr-m2-control-') as temporary:
                project = Path(temporary) / 'project'
                common.checkout(original, project)
                if variant != 'unchanged':
                    prescribed_change(task, variant, project)
                verdict = check(task, project, prepared, tools, output / variant)
                summary['variants'][variant] = {'passed': verdict['passed'], 'stage': verdict['stage']}
                if task == 'regex-escape-len' and variant == 'unchanged':
                    require(verdict['checks'] and verdict['checks'][0]['stage'] == 1,
                            'unchanged regex must fail only because the requested APIs are absent')
                require(verdict['passed'] == (variant == 'reference'),
                        'incorrect M2 oracle outcome: ' + task + '/' + variant)
                if variant != 'reference':
                    continue
                patch = common.comparison_grade.BASE.git(project, 'diff', '--binary', 'HEAD').stdout
                (output / 'reference.patch').write_bytes(patch)
                receiver = Path(temporary) / 'receiver'
                common.checkout(original, receiver)
                common.comparison_grade.BASE.git(receiver, 'apply', '--check', '-', data=patch)
                common.comparison_grade.BASE.git(receiver, 'apply', '-', data=patch)
                delivered = common.sources(receiver)
                require(delivered == common.sources(project), 'reference patch differs in its receiver')
                received = check(task, receiver, prepared, tools, output / 'receiver')
                require(received['passed'], 'reference change failed in a fresh receiver')
                common.comparison_grade.BASE.git(receiver, 'apply', '--reverse', '-', data=patch)
                require(common.sources(receiver) == prepared['original'], 'undo did not restore exact original bytes and modes')
                common.comparison_grade.BASE.git(receiver, 'apply', '-', data=patch)
                require(common.sources(receiver) == delivered, 'redo did not restore the delivered bytes and modes')
                summary.update(receiver_passed=True, undo_exact=True, redo_exact=True,
                               reference_patch_sha256=digest_bytes(patch))
        summary['passed'] = True
        return summary
    finally:
        common.save(output / 'summary.json', summary)


if __name__ == '__main__':
    import argparse
    parser = argparse.ArgumentParser(description=__doc__)
    operations = parser.add_subparsers(dest='operation', required=True)
    preparing = operations.add_parser('prepare')
    preparing.add_argument('--task', choices=list(TASKS), required=True)
    preparing.add_argument('--output', type=Path, required=True)
    controlling = operations.add_parser('controls')
    controlling.add_argument('--task', choices=list(TASKS), required=True)
    controlling.add_argument('--inputs', type=Path, required=True)
    controlling.add_argument('--output', type=Path, required=True)
    for name in ('python', 'cargo', 'rustc', 'node', 'typescript'):
        controlling.add_argument('--' + name, required=True)
    arguments = parser.parse_args()
    if arguments.operation == 'prepare':
        result = prepare(arguments.task, arguments.output / 'original', arguments.output / 'preparation')
        print(json.dumps({'task': arguments.task, 'source_files': len(result['original']), 'live_model_calls': 0}))
    else:
        tools = {name: getattr(arguments, name) for name in ('python', 'cargo', 'rustc', 'node', 'typescript')}
        print(json.dumps(controls(arguments.task, arguments.inputs, tools, arguments.output)))
