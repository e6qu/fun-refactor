"""Exercise real fr previews, refusal, checks, patch delivery and reversal for M2."""
import argparse
import json
import os
from pathlib import Path
import tempfile

from . import project_edits_tasks as cases
from . import selected_tasks as common
from .selected_plan import require


def invoke(binary, project, arguments, output):
    result = common.command([str(binary), '-C', str(project), '--json', *arguments], project, output)
    return result['returncode'], json.loads(result['stdout'])


def declaration(binary, project, name, path, output):
    report = common.fr(binary, project, ['project', 'find', name, '--in', path,
        '--source', '--bytes', '65536'], output)
    require(report['page']['next'] is None, 'control discovery must include every match')
    rows = [dict(zip(report['columns'], row)) for row in report['rows']]
    require(len(rows) == 1 and rows[0]['source']['next_offset'] is None,
            'control needs one complete declaration')
    return rows[0], report['root']


def plan(task, binary, project, output):
    output.mkdir(parents=True, exist_ok=False)
    operations = []

    def operation(kind, handle, text):
        fragment = output / ('fragment-' + str(len(operations)))
        fragment.write_text(text)
        operations.append({'op': kind, 'handle': handle, 'from': str(fragment)})

    if task == 'rust-parameter':
        target, _ = declaration(binary, project, 'total', 'pricing/src/lib.rs', output / 'discovery.json')
        operation('refactor', target['handle'], '{"operation":"remove-parameter","index":1}')
    elif task == 'typescript-alias':
        target, _ = declaration(binary, project, 'price', 'src/pricing.ts', output / 'discovery.json')
        operation('refactor', target['handle'], '{"operation":"rename","name":"quote","documentation":"preserve"}')
    elif task == 'python-invoice':
        target, _ = declaration(binary, project, 'invoice_total', 'invoice.py', output / 'discovery.json')
        operation('replace-body', target['handle'], 'return amount + fee\n')
    else:
        require(task == 'regex-escape-len', 'choose an explicit delivery case')
        lower, lower_file = declaration(binary, project, 'escape', 'regex-syntax/src/lib.rs', output / 'lower.json')
        _, facade_file = declaration(binary, project, 'escape', 'src/lib.rs', output / 'facade.json')
        body = lower['source']['text'].split('{', 1)[1]
        require(body.count('String::new()') == 1, 'regex control needs the original buffer creation')
        operation('replace-body', lower['handle'], '{' + body.replace('String::new()', 'String::with_capacity(escape_len(text))'))
        operation('insert-declaration', lower_file,
            '/// Returns the escaped UTF-8 byte length without allocating.\n'
            'pub fn escape_len(text: &str) -> usize {\n'
            '    text.chars().map(|c| c.len_utf8() + usize::from(is_meta_character(c))).sum()\n}\n')
        operation('insert-declaration', facade_file,
            '/// Returns the escaped UTF-8 byte length without allocating.\n'
            'pub fn escape_len(text: &str) -> usize { regex_syntax::escape_len(text) }\n')
    manifest = output / 'author-input'
    common.save(manifest, {'operations': operations})
    before = common.sources(project)
    preview = common.fr(binary, project, ['author', 'batch', '--from', str(manifest)], output / 'preview.json')
    saved = common.fr(binary, project, ['author', 'batch', '--from', str(manifest), '--save-plan',
        '--plan-basis', preview['plan_context_basis']], output / 'saved.json')
    require(common.sources(project) == before, 'preview or save changed source')
    return preview, saved


def check_unselected_bytes(project, original, preview):
    ranges = {}
    for step in preview['steps']:
        edits = step['edits'] if step['operation'] == 'refactor' else [step]
        for edit in edits:
            ranges.setdefault(edit['path'], []).append(edit)
    current = common.sources(project)
    changed = {path for path in original.keys() | current.keys()
               if path not in original or path not in current or original[path] != (project / path).read_bytes()}
    require(changed == ranges.keys(), 'changed paths differ from the previewed ranges')
    for path, edits in ranges.items():
        before, after = original[path], (project / path).read_bytes()
        old_offset = new_offset = 0
        for edit in sorted(edits, key=lambda entry: entry['before_span']['start']):
            start, end = [edit['before_span'][key] for key in ('start', 'end')]
            require(old_offset <= start <= end <= len(before), 'preview ranges overlap or leave the original file')
            length = start - old_offset
            require(before[old_offset:start] == after[new_offset:new_offset + length],
                    'bytes outside a previewed edit changed')
            old_offset, new_offset = end, new_offset + length + edit['after_bytes']
        require(before[old_offset:] == after[new_offset:], 'bytes after the final previewed edit changed')
    return ranges


def configure(project, tools, task):
    directory = project / '.fr'
    directory.mkdir(exist_ok=True)
    checks = [{**row, 'cwd': '.', 'timeout_seconds': 120, 'covers': ['public M2 project checks']}
              for row in cases.public_checks(task, tools)]
    checks.append({'name': 'forced-failure', 'argv': [tools['python'], '-I', '-B', '-c',
        'import sys;sys.stderr.write("M2 deliberately failed project check\\n");sys.exit(1)'],
        'cwd': '.', 'timeout_seconds': 10, 'covers': ['failed-check state control']})
    common.save(directory / 'checks.json', {'schema': 1, 'checks': checks})
    return [row['name'] for row in checks if row['name'] != 'forced-failure']


def workflow(binary, project, saved, names, output):
    listed = common.fr(binary, project, ['checks'], output / 'check-listing.json')
    manifest = output / 'workflow-input'
    common.save(manifest, {'schema': 1, 'transaction': saved['transaction'],
        'transaction-context-basis': saved['transaction_context_basis'],
        'checks': {'basis': listed['basis'], 'names': names},
        'exercise-reversal': True, 'patch': {'output': 'change.patch'}, 'check-output-bytes': 2048})
    preview = common.fr(binary, project, ['workflow', '--from', str(manifest)], output / 'workflow-preview.json')
    code, result = invoke(binary, project, ['workflow', '--from', str(manifest), '--write',
        '--basis', preview['workflow_basis']], output / 'workflow-write.json')
    return code, result


def controls(task, binary, inputs, tools, output):
    require(os.environ.get('GITHUB_ACTIONS') == 'true', 'run complete M2 delivery controls on GitHub')
    output.mkdir(parents=True, exist_ok=False)
    original = inputs / 'original'
    prepared = json.loads((inputs / 'preparation/project.json').read_text())
    require(prepared['task'] == task and prepared['requirement'] == cases.TASKS[task] and
            common.sources(original) == prepared['original'], 'delivery inputs differ from the frozen task')
    summary = {'passed': False, 'task': task, 'live_model_calls': 0,
               'scope': 'Supplied changes through real fr; not live agent results.'}
    try:
        with tempfile.TemporaryDirectory(prefix='fr-m2-delivery-') as temporary:
            root = Path(temporary)
            project = root / 'project'
            common.checkout(original, project)
            names = configure(project, tools, task)
            source = {path: (original / path).read_bytes() for path in prepared['original']}
            preview, saved = plan(task, binary, project, output / 'stale-plan')
            path = project / next(name for name in source if Path(name).suffix in {'.rs', '.ts', '.py'})
            path.write_bytes(path.read_bytes() + b'\n')
            competing = common.sources(project)
            code, refused = invoke(binary, project, ['history', 'apply', str(saved['transaction']), '--write'],
                output / 'stale-apply.json')
            require(code != 0 and common.sources(project) == competing,
                    'old saved preview did not refuse the competing edit before writing')
            summary['stale_refused_without_writes'] = True
            path.write_bytes(source[str(path.relative_to(project))])
            preview, saved = plan(task, binary, project, output / 'fresh-plan')
            code, delivered = workflow(binary, project, saved, names, output)
            require(code == 0 and delivered['passed'] and delivered['transaction_status'] == 'applied',
                    'fresh reviewed delivery did not pass every stage')
            ranges = check_unselected_bytes(project, source, preview)
            cases.boundaries(task, project, prepared)
            changed = common.sources(project)
            patch = project / 'change.patch'
            require(patch.is_file(), 'fr did not deliver its checked patch')
            (output / 'change.patch').write_bytes(patch.read_bytes())
            receiver = root / 'receiver'
            common.checkout(original, receiver)
            common.comparison_grade.BASE.git(receiver, 'apply', '--check', str(patch))
            common.comparison_grade.BASE.git(receiver, 'apply', str(patch))
            require(common.sources(receiver) == changed, 'patch receiver differs from the fr delivery')
            received = cases.check(task, receiver, prepared, tools, output / 'receiver')
            require(received['passed'], 'fr patch failed independent receiver checks')
            common.fr(binary, project, ['history', 'undo', str(saved['transaction']), '--write'], output / 'undo.json')
            require(common.sources(project) == prepared['original'], 'fr undo did not restore exact bytes and modes')
            common.fr(binary, project, ['history', 'redo', str(saved['transaction']), '--write'], output / 'redo.json')
            require(common.sources(project) == changed, 'fr redo did not restore exact delivered bytes and modes')
            summary.update(receiver_passed=True, undo_exact=True, redo_exact=True,
                           previewed_edits=sum(map(len, ranges.values())),
                           patch_sha256=common.digest(patch))

            failed_project = root / 'failed-project'
            common.checkout(original, failed_project)
            configure(failed_project, tools, task)
            failed_output = output / 'failed-check'
            failed_output.mkdir()
            failed_preview, failed_saved = plan(task, binary, failed_project, failed_output / 'plan')
            code, failed = workflow(binary, failed_project, failed_saved, ['forced-failure'], failed_output)
            require(code != 0 and failed['passed'] is False and failed['transaction_status'] == 'applied' and
                    'M2 deliberately failed project check' in json.dumps(failed) and
                    not (failed_project / 'change.patch').exists(), 'failed check lost its diagnostics or applied state')
            check_unselected_bytes(failed_project, source, failed_preview)
            require(common.sources(failed_project) == changed, 'failed check misreported its applied source')
            summary['failed_check_retains_diagnostics_and_applied_source'] = True

            if task == 'rust-parameter':
                effect_project = root / 'effect-project'
                common.checkout(original, effect_project)
                path = effect_project / 'pricing/src/lib.rs'
                text = path.read_text()
                require(text.count(', true, ') == 1, 'side-effect control needs its original Boolean argument')
                path.write_text(text.replace(', true, ', ', record(), ', 1) + '\nfn record() -> bool { println!("effect"); false }\n')
                before = common.sources(effect_project)
                try:
                    plan(task, binary, effect_project, output / 'side-effect')
                except ValueError:
                    refusal = json.loads((output / 'side-effect/preview.json').read_text())
                    require('literal scalar' in refusal['stderr'] and common.sources(effect_project) == before,
                            'side-effecting argument did not refuse without writes')
                else:
                    raise ValueError('side-effect control unexpectedly admitted removal')
                summary['side_effecting_argument_refused_without_writes'] = True
        summary['passed'] = True
        return summary
    finally:
        common.save(output / 'summary.json', summary)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--task', choices=list(cases.TASKS), required=True)
    parser.add_argument('--fr', type=Path, required=True)
    parser.add_argument('--inputs', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    for name in ('python', 'cargo', 'rustc', 'node', 'typescript'):
        parser.add_argument('--' + name, required=True)
    arguments = parser.parse_args()
    tools = {name: getattr(arguments, name) for name in ('python', 'cargo', 'rustc', 'node', 'typescript')}
    print(json.dumps(controls(arguments.task, arguments.fr.resolve(), arguments.inputs.resolve(), tools,
                              arguments.output.resolve())))
