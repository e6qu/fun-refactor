#!/usr/bin/env python3
"""Retain original-package answers and independent Python checks, including failures."""
import argparse
import ast
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import tomllib
import urllib.request
import zipfile

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'tools'))
sys.path.insert(0, str(ROOT / 'sdk/python/src'))
from agent_eval.bounded_host import run
from evidence_basis import native_bindings, native_source_bindings, cargo_manifest_basis, cargo_lock_basis
from python_repository_basis import file_digest as repository_digest
from fr_ir.context import DirectoryObjectStore, MemoryObjectStore
from fr_ir.flow import FlowCache
from fr_ir.runtime import FrClient

spec = importlib.util.spec_from_file_location('repository_flow', ROOT / 'tools/python-repository-flow.py')
flow = importlib.util.module_from_spec(spec)
spec.loader.exec_module(flow)
PLAN = json.loads((ROOT / 'tools/package-question-projects.json').read_text())
COLLECTOR_INPUTS = (
    'tools/package-questions.py', 'tools/package-question-projects.json',
    'tools/evidence_basis.py', 'tools/python_repository_basis.py',
    'tools/python-repository-flow.py', 'tools/compiler-profile.py',
    'tools/agent_eval/bounded_host.py', 'tests/agent-eval/python-repository-flow/cohort.json',
    'tests/agent-eval/python-repositories/boltons.tar.gz', *PLAN['previous_attempts'],
)
RULES = {'version': 'package-question', 'sources': ['emit_value'], 'sinks': ['record_value'],
         'sanitizers': {}, 'propagators': []}
LIMITS = dict(wall_seconds=20, rss_bytes=512 * 1024**2, disk_bytes=16 * 1024**2,
              cpu_limit_seconds=10, transcript_bytes=1024**2)
CACHE_STAGES = [('cold', False, True), ('restored', True, True), ('unrelated-edit', True, True),
                ('helper-replaced', False, False), ('helper-restored', True, True), ('import-replaced', False, False)]

# This reference process imports neither the SDK nor the evaluation framework.
# -I prevents project startup hooks from running before the explicit import root.
RUNTIME_SOURCE = '''import datetime, importlib.util, json, sys
from pathlib import Path
request = json.loads(Path(sys.argv[1]).read_text())
scope = Path(request['scope'])
sys.path.insert(0, str(scope))
spec = importlib.util.spec_from_file_location('package_question', scope / request['path'])
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
module.emit_value = lambda: request['value']
module.record_value = lambda value: value
module.opaque = lambda value: value
value = getattr(module, request['entry'])()
if type(value) is datetime.date:
    value = value.isoformat()
if value is not None and type(value) not in (int, float, bool, str):
    raise ValueError('runtime result is outside the declared scalar/date contract')
print(json.dumps({'value': value}))
'''


def save(path, value):
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + '\n')


def bindings():
    return {**native_bindings(ROOT, [*COLLECTOR_INPUTS, 'tests/agent-eval/package-questions/freeze.json']),
            **{path.relative_to(ROOT).as_posix(): repository_digest(path)
               for path in sorted((ROOT / 'sdk/python/src').rglob('*.py'))}}


def verify_analyzer(root, frozen):
    paths = flow.ANALYZER_PATHS
    revision = frozen['analyzer_revision']
    recorded = {path: flow.command(['git', 'rev-parse', revision + ':' + path], cwd=root).stdout.strip()
                for path in paths}
    flow.require(recorded == frozen['analyzer_trees'], 'frozen source identifiers differ from their recorded commit')
    status = flow.command(['git', 'status', '--porcelain', '--untracked-files=all', '--', *paths], cwd=root).stdout
    flow.require(not status, 'analyzer and client source must be committed before checking the freeze')
    native = native_source_bindings(root)
    sdk = {path.relative_to(root).as_posix(): repository_digest(path)
           for path in (root / 'sdk/python/src').rglob('*.py')}
    tracked = set(flow.command(['git', 'ls-files', '-z', '--', *paths], cwd=root).stdout.split('\0'))
    flow.require(set(native) | set(sdk) <= tracked, 'analyzer input is absent from the committed source inventory')
    changed = flow.command(['git', 'diff', '--name-only', '-z', revision, 'HEAD', '--', *paths], cwd=root).stdout.split('\0')
    members = tomllib.loads((root / 'Cargo.toml').read_text())['workspace']['members']
    manifests = {'Cargo.toml' if member == '.' else member + '/Cargo.toml' for member in members}
    for name in filter(None, changed):
        flow.require(name in manifests or name in ('Cargo.lock', 'sdk/python/src/fr_ir/_version.py'),
                     'frozen analyzer code changed: ' + name)
        source = subprocess.run(['git', 'show', revision + ':' + name], cwd=root, check=True,
                                capture_output=True, timeout=30).stdout
        if name in manifests:
            expected = flow.sha(cargo_manifest_basis(source))
            actual = native[name]
        elif name == 'Cargo.lock':
            expected = flow.sha(cargo_lock_basis(source))
            actual = native[name]
        else:
            with tempfile.TemporaryDirectory() as directory:
                original = Path(directory) / name
                original.parent.mkdir(parents=True)
                original.write_bytes(source)
                expected = repository_digest(original)
            actual = sdk[name]
        flow.require(actual == expected, 'frozen analyzer behavior or dependencies changed: ' + name)


def prepare(scope, project, variant, case):
    entry = 'entry' if variant == 'original' else 'renamed_entry'
    path = 'question.py' if variant == 'original' else 'moved_question/adapter.py'
    if variant != 'original':
        (scope / 'moved_question').mkdir(exist_ok=True)
        (scope / 'moved_question/__init__.py').write_text('')
    expression = project['expression'].replace('VALUE', 'emit_value()')
    body = f'return record_value({expression})'
    if case == 'overwritten':
        body = f'value = emit_value()\nvalue = {project["overwrite"]}\nreturn record_value({project["expression"].replace("VALUE", "value")})'
    elif case == 'unknown':
        body = f'return record_value(opaque({expression}))'
    source = f'from {project["module"]} import {project["function"]} as relay\n\ndef {entry}():\n'
    source += ''.join('    ' + line + '\n' for line in body.splitlines())
    (scope / path).write_text(source)
    (scope / 'rules.json').write_text(json.dumps(RULES))
    return path, entry, source


def analysis(request):
    scope = Path(request['scope'])
    client = FrClient(scope, executable=request['fr'], max_output_bytes=1048576)
    handle = client.project('find', request['entry'], '--in', request['path']).definition_target().handle
    project = request['project']
    settings = dict(standard_library=tuple(project['standard_library']), rules=scope / 'rules.json',
                    summaries=True, imports=True, context='html', depth=8, steps=1 if request['case'] == 'steps' else 4096,
                    max_bytes=1048576)
    cache = FlowCache(MemoryObjectStore())
    result = cache.analyze(client, handle, **settings)
    report = result.report.to_data()
    save(Path(request['report']), report)
    if request['case'] in ('unknown', 'steps'):
        flow.require(not report['complete'] and report['cutoffs'], 'unchecked behavior claimed a complete answer')
    elif report['complete']:
        flow.require(not report['cutoffs'] and bool(result.witnesses) == (request['case'] == 'positive'),
                     'complete answer disagrees with independently declared connection')
    print(json.dumps({'complete': report['complete'], 'witnesses': len(result.witnesses), 'cutoffs': report['cutoffs']}))


def cache_controls(request):
    scope = Path(request['scope'])
    project = request['project']
    client = FrClient(scope, executable=request['fr'], max_output_bytes=1048576)
    store = DirectoryObjectStore(request['objects'])
    manifest = request['previous_manifest']
    cache = FlowCache(store) if manifest is None else FlowCache.restore(store, manifest)
    settings = dict(standard_library=tuple(project['standard_library']), rules=scope / 'rules.json',
                    summaries=True, imports=True, context='html', depth=8, steps=4096, max_bytes=1048576)
    output = Path(request['reports'])
    label, reused, reaches = CACHE_STAGES[request['stage']]
    flow.require((manifest is None) == (label == 'cold'), 'cache stage lacks its predecessor')
    handle = client.project('find', request['entry'], '--in', request['path']).definition_target().handle
    retained = cache.analyze(client, handle, **settings)
    fresh = FlowCache(MemoryObjectStore()).analyze(client, handle, **settings)
    left, right = retained.report.to_data(), fresh.report.to_data()
    save(output / (label + '-saved.json'), left)
    save(output / (label + '-fresh.json'), right)
    for value in (left, right):
        value.pop('execution')
        value.pop('context_basis', None)
    flow.require(left == right and retained.reused == reused, 'saved/fresh answer or reuse decision disagrees: ' + label)
    flow.require(left['complete'] and bool(retained.witnesses) == reaches, 'cache control did not finish with the declared connection: ' + label)
    print(json.dumps({'stage': label, 'reused': retained.reused, 'complete': True, 'reaches': reaches,
                      'previous_manifest': manifest, 'manifest': cache.persist()}))


def collect_cache_controls(scope, project, directory, binary, row, result, output):
    path, entry, _ = prepare(scope, project, 'original', 'positive')
    module = scope.joinpath(*project['module'].split('.')).with_suffix('.py')
    if not module.is_file():
        module = scope.joinpath(*project['module'].split('.'), '__init__.py')
    original_bytes = module.read_bytes()
    original = original_bytes.decode()
    declaration, = [node for node in ast.parse(original).body if isinstance(node, ast.FunctionDef) and node.name == project['function']]
    returns = [node for node in ast.walk(declaration) if isinstance(node, ast.Return)]
    flow.require(returns, 'selected helper has no return to edit')
    edited = original
    for node in reversed(sorted(returns, key=lambda item: (item.lineno, item.col_offset))):
        fragment = ast.get_source_segment(original, node)
        flow.require(edited.count(fragment) == 1, 'relevant edit cannot select a unique return')
        edited = edited.replace(fragment, 'return 0', 1)
    flow.require(module.name != '__init__.py', 'package initializer replacement requires a separately declared control')
    hidden = module.with_suffix('.question-original')
    replacement = module.with_suffix('')
    flow.require(not replacement.exists() and not hidden.exists(), 'replacement paths already exist')
    row['cache'] = {'stages': [], 'result': {'complete': False}}
    manifest = None
    for index, (label, _, _) in enumerate(CACHE_STAGES):
        if label == 'unrelated-edit':
            (scope / 'unrelated_question_note.py').write_text('value = 42\n')
        if label == 'helper-replaced':
            module.write_text(edited)
        if label == 'import-replaced':
            module.rename(hidden)
            replacement.mkdir()
            parameters = ', '.join(arg.arg for arg in declaration.args.posonlyargs + declaration.args.args)
            (replacement / '__init__.py').write_text(f'def {project["function"]}({parameters}):\n    return 0\n')
        try:
            request = {'mode': 'cache', 'scope': str(scope), 'path': path, 'entry': entry, 'project': project,
                       'fr': str(binary.resolve()), 'reports': str(directory), 'objects': str(directory / 'cache-objects'),
                       'stage': index, 'previous_manifest': manifest}
            observation = execute(directory, 'cache-' + label, request, scope)
            row['cache']['stages'].append(observation)
            save(output / 'result.json', result)
            flow.require(observation['result'] is not None and observation['result']['complete'],
                         'cache stage failed or reached its resource budget: ' + label)
            manifest = observation['result']['manifest']
        finally:
            if label == 'import-replaced':
                (replacement / '__init__.py').unlink()
                replacement.rmdir()
                hidden.rename(module)
            module.write_bytes(original_bytes)
    row['cache']['result'] = {'complete': True}


def execute(output, name, request, scope):
    directory = output / name
    directory.mkdir()
    save(directory / 'request.json', request)
    environment = {'PATH': os.environ['PATH'], 'HOME': str(directory), 'LANG': 'C.UTF-8',
                   'PYTHONDONTWRITEBYTECODE': '1', 'PYTHONHASHSEED': '0'}
    command = ([sys.executable, '-I', '-B', '-c', RUNTIME_SOURCE, str(directory / 'request.json')]
               if request['mode'] == 'runtime' else
               [sys.executable, '-I', '-B', str(Path(__file__).resolve()), '--worker', str(directory / 'request.json')])
    with (directory / 'stdout').open('wb') as stdout, (directory / 'stderr').open('wb') as stderr:
        observation = run(command, b'', stdout, stderr, output, env=environment, cwd=scope, **LIMITS)
    observation['command'] = command
    observation['stdout_sha256'] = flow.sha((directory / 'stdout').read_bytes())
    observation['stderr_sha256'] = flow.sha((directory / 'stderr').read_bytes())
    observation['result'] = None
    if observation['exit_code'] == 0 and not observation['stop_reason']:
        observation['result'] = json.loads((directory / 'stdout').read_text())
    save(directory / 'observation.json', observation)
    return observation


def admission(root):
    licenses = sorted(path for path in root.iterdir() if path.is_file()
                      and path.name.lower().startswith(('license', 'copying')))
    requirements = [root / name for name in ('pyproject.toml', 'setup.cfg', 'setup.py', 'requirements.txt')
                    if (root / name).is_file()]
    flow.require(licenses, 'the original project root has no license for admission review')
    flow.require(sum(path.stat().st_size for path in [*licenses, *requirements]) <= 65536,
                 'license and requirement disclosure exceeds 64 KiB')
    def records(paths):
        return [{'path': path.relative_to(root).as_posix(), 'sha256': flow.sha(path.read_bytes()),
                 'text': path.read_text()} for path in paths]
    return {'licenses': records(licenses), 'requirements': records(requirements),
            'dependency_installation': 'The collector installs no third-party dependencies. Preserve original-import runtime failures; do not rewrite package requirements.'}


def checked_coverage(record, value, source, frozen, repository):
    declared = {project['id']: project for project in PLAN['projects'] if project['role'] == 'coverage'}
    flow.require(record['head_sha'] == source and record['status'] == 'completed' and record['conclusion'] == 'success'
                 and record['path'] == '.github/workflows/refinement-evidence.yml'
                 and record['event'] == 'workflow_dispatch'
                 and record['repository']['full_name'] == repository
                 and record['head_repository']['full_name'] == repository,
                 'coverage must pass at the same frozen commit in this repository')
    flow.require(value['source_commit'] == source and value['phase'] == 'coverage'
                 and value['implementation'] == frozen and value['plan'] == PLAN,
                 'coverage artifact uses a different phase, implementation or project plan')
    checked_questions(value['projects'], declared, True)


def checked_questions(rows, declared, require_cache):
    expected = set(declared)
    flow.require(len(rows) == len(expected) and {row['project']['id'] for row in rows} == expected,
                 'coverage artifact omits or duplicates a planned project')
    for row in rows:
        project = declared[row['project']['id']]
        flow.require(row['project'] == project, 'coverage changed a declared project or its Python examples')
        flow.require(row['complete'] is True and row['error'] is None and row['original_sources_unchanged'] is True,
                     'package comparison includes an unfinished project')
        stages = row['cache']['stages'] if require_cache else []
        if require_cache:
            flow.require(row['cache']['result']['complete'] is True, 'saved-answer comparison is unfinished')
        else:
            flow.require(not row.get('cache'), 'unfamiliar question unexpectedly changes the declared checks')
        flow.require(not require_cache or len(stages) == len(CACHE_STAGES), 'coverage omits a saved-answer stage')
        previous = None
        for observation, (label, reused, reaches) in zip(stages, CACHE_STAGES):
            stage = observation['result']
            flow.require(observation['exit_code'] == 0 and observation['stop_reason'] is None
                         and stage['stage'] == label and stage['reused'] is reused and stage['reaches'] is reaches
                         and stage['complete'] is True and stage['previous_manifest'] == previous
                         and isinstance(stage['manifest'], str) and len(stage['manifest']) == 64
                         and all(character in '0123456789abcdef' for character in stage['manifest']),
                         'saved-answer stages lost their verified connection')
            previous = stage['manifest']
        cells = row['cases']
        planned = {(variant, case) for variant in ('original', 'renamed-relocated')
                   for case in ('positive', 'overwritten', 'unknown', 'steps')}
        flow.require(len(cells) == len(planned) and {(cell['variant'], cell['case']) for cell in cells} == planned,
                     'coverage omits a planned question')
        for cell in cells:
            analysis = cell['analysis']
            answer = analysis['result']
            complete = cell['case'] in ('positive', 'overwritten')
            flow.require(analysis['exit_code'] == 0 and analysis['stop_reason'] is None
                         and answer['complete'] is complete
                         and (not answer['cutoffs'] and bool(answer['witnesses']) == (cell['case'] == 'positive') if complete else bool(answer['cutoffs'])),
                         'coverage contains an incorrect answer or resource stop')
            expected_values = [project['blocked']] * 2 if cell['case'] == 'overwritten' else project['expected']
            flow.require(len(cell['runtime']) == 2 and all(
                type(item['input']) is type(project['values'][index])
                and item['input'] == project['values'][index]
                and type(item['expected']) is type(expected_values[index]) and item['expected'] == expected_values[index]
                and item['observation']['exit_code'] == 0 and item['observation']['stop_reason'] is None
                and type(item['observation']['result']['value']) is type(expected_values[index])
                and item['observation']['result'] == {'value': expected_values[index]}
                for index, item in enumerate(cell['runtime'])),
                'coverage includes an incorrect Python example')


def coverage_result(run_id, output, source, frozen):
    repository = os.environ['GITHUB_REPOSITORY']
    record = json.loads(flow.command(['gh', 'api', f'repos/{repository}/actions/runs/{run_id}']).stdout)
    artifacts = json.loads(flow.command(['gh', 'api', f'repos/{repository}/actions/runs/{run_id}/artifacts']).stdout)['artifacts']
    artifact, = [item for item in artifacts if item['name'] == 'package-questions-summary' and not item['expired']]
    flow.require(artifact['size_in_bytes'] <= 4 * 1024**2, 'coverage summary archive exceeds its admission limit')
    downloaded = subprocess.run(['gh', 'api', '--allow-escape-sequences',
                                  f'repos/{repository}/actions/artifacts/{artifact["id"]}/zip'],
                                 capture_output=True, timeout=30)
    flow.require(downloaded.returncode == 0 and len(downloaded.stdout) <= 4 * 1024**2, 'coverage summary download failed or exceeded its limit')
    with zipfile.ZipFile(io.BytesIO(downloaded.stdout)) as archive:
        member = archive.getinfo('result.json')
        flow.require(member.file_size <= 1024**2, 'coverage summary exceeds 1 MiB')
        value = json.loads(archive.read(member))
    checked_coverage(record, value, source, frozen, repository)
    save(output / 'coverage-result.json', value)
    save(output / 'coverage-run.json', {'run': record, 'artifact': artifact, 'download_sha256': flow.sha(downloaded.stdout)})
    return value


def milestone_outcome(rows, coverage, phase, frozen):
    finished = [row for row in rows if row['complete'] is True and row['error'] is None]
    familiar = coverage['projects'] if coverage is not None else rows if phase in ('coverage', 'development') else []
    first = frozen is not None and any(row['complete'] is True and row['error'] is None for row in familiar)
    second = (frozen is not None and phase == 'held-out' and coverage is not None and bool(finished)
              and len({row['project']['repository'] for row in [*familiar, *finished]}) >= 3)
    independent = sorted({row['project']['repository'] for row in [*familiar, *finished]
                          if row['complete'] is True and row['error'] is None})
    return {'milestone_1_demonstrated': first, 'milestone_2_demonstrated': second,
            'independent_projects': independent,
            'milestones_complete': first and second}


def checked_completion(value, coverage, frozen):
    declared = {project['id']: project for project in PLAN['projects'] if project['role'] == 'held-out'}
    rows = value['projects']
    flow.require(len(rows) == len(declared) and {row['project']['id'] for row in rows} == set(declared)
                 and all(row['project'] == declared[row['project']['id']] for row in rows),
                 'unfamiliar comparison omitted or changed a planned project')
    finished = [row for row in rows if row['complete'] is True and row['error'] is None]
    checked_questions(finished, {row['project']['id']: declared[row['project']['id']] for row in finished}, False)
    outcome = milestone_outcome(rows, coverage, 'held-out', frozen)
    flow.require(outcome['milestones_complete'] is True and all(value[key] == expected for key, expected in outcome.items()),
                 'package completion differs from its checked questions')
    return outcome


def collect(args):
    flow.require(sys.platform == 'linux', 'package collection belongs on the declared Linux runner')
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    source = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
    frozen = None
    coverage = None
    if args.phase != 'development':
        frozen = json.loads((ROOT / 'tests/agent-eval/package-questions/freeze.json').read_text())
        verify_analyzer(ROOT, frozen)
        flow.require(frozen['repository'] == os.environ['GITHUB_REPOSITORY'], 'freeze belongs to another evaluation repository')
        actual = bindings()
        flow.require(frozen['collector_inputs'] == {name: actual[name] for name in COLLECTOR_INPUTS},
                     'collector helpers, source archive or earlier outcome differs from freeze')
    if args.phase == 'held-out':
        flow.require(args.coverage_run, 'held-out collection requires successful coverage at the same commit')
        coverage = coverage_result(args.coverage_run, output, source, frozen)
    result = {'schema': 'fr-original-package-questions', 'source_bindings': bindings(),
              'source_commit': source, 'binary_sha256': flow.sha(args.fr.resolve().read_bytes()),
              'phase': args.phase, 'plan': PLAN, 'limits': LIMITS,
              'python': sys.version, 'platform': sys.platform,
              'implementation': frozen, 'coverage_run': args.coverage_run,
              'scope': 'Original package imports and stated source models; no general Python, proof or cost-saving claim.',
              'projects': [], 'milestones_complete': False}
    save(output / 'result.json', result)
    role = 'held-out' if args.phase == 'held-out' else 'coverage'
    for declared in PLAN['projects']:
        if declared['role'] != role:
            continue
        project = dict(declared)
        directory = output / project['id']
        directory.mkdir()
        row = {'project': project, 'cases': [], 'error': None, 'complete': False}
        result['projects'].append(row)
        try:
            if project['id'] == 'boltons':
                data = (ROOT / 'tests/agent-eval/python-repositories/boltons.tar.gz').read_bytes()
            else:
                url = f'https://codeload.github.com/{project["repository"]}/tar.gz/{project["revision"]}'
                with urllib.request.urlopen(url, timeout=30) as response:
                    data = response.read(4 * 1024**2 + 1)
            (directory / 'source.tar.gz').write_bytes(data)
            row['archive_sha256'] = flow.sha(data)
            upstream = directory / 'upstream'
            upstream.mkdir()
            row['source_files'] = flow.snapshot(data, project, upstream)
            row['admission'] = admission(upstream)
            scope = upstream / project['python_root']
            if role == 'held-out':
                row['selection'] = project['public_question']
                document = (upstream / row['selection']['document']).read_bytes()
                blob = hashlib.sha1(b'blob ' + str(len(document)).encode() + b'\0' + document).hexdigest()
                flow.require(blob == row['selection']['document_blob'], 'public question document changed')
            for variant in ('original', 'renamed-relocated'):
                for case in ('positive', 'overwritten', 'unknown', 'steps'):
                    path, entry, adapter = prepare(scope, project, variant, case)
                    name = f'{variant}-{case}'
                    cell = {'variant': variant, 'case': case, 'path': path, 'entry': entry, 'adapter': adapter, 'runtime': []}
                    row['cases'].append(cell)
                    request = {'mode': 'analysis', 'scope': str(scope), 'path': path, 'entry': entry, 'project': project,
                               'case': case, 'fr': str(args.fr.resolve()), 'report': str(directory / (name + '.json'))}
                    cell['analysis'] = execute(directory, name, request, scope)
                    for index, value in enumerate(project['values']):
                        request = {'mode': 'runtime', 'scope': str(scope), 'path': path, 'entry': entry, 'value': value}
                        observed = execute(directory, f'{name}-python-{index}', request, scope)
                        expected = project['blocked'] if case == 'overwritten' else project['expected'][index]
                        cell['runtime'].append({'input': value, 'expected': expected, 'observation': observed})
                        flow.require(observed['result'] is not None
                                     and type(observed['result']['value']) is type(expected)
                                     and observed['result'] == {'value': expected},
                                     'ordinary Python disagrees with the independent example')
                    save(output / 'result.json', result)
            if role == 'coverage':
                collect_cache_controls(scope, project, directory, args.fr, row, result, output)
            row['complete'] = all(cell['analysis']['result'] is not None
                                   and (cell['analysis']['result']['complete'] if cell['case'] in ('positive', 'overwritten')
                                        else not cell['analysis']['result']['complete']) for cell in row['cases'])
            if role == 'coverage':
                row['complete'] &= row['cache']['result'] is not None and row['cache']['result']['complete']
        except (OSError, ValueError, SyntaxError, KeyError) as error:
            row['error'] = f'{type(error).__name__}: {error}'
        if 'source_files' in row:
            row['original_sources_unchanged'] = all(
                (upstream / path).is_file() and flow.sha((upstream / path).read_bytes()) == digest
                for path, digest in row['source_files'].items())
            if not row['original_sources_unchanged']:
                row['complete'] = False
                row['error'] = (row['error'] + '; ' if row['error'] else '') + 'original package files changed'
        save(output / 'result.json', result)
    flow.require(bindings() == result['source_bindings'], 'evaluation inputs changed during collection')
    result.update(milestone_outcome(result['projects'], coverage, args.phase, frozen))
    if args.phase == 'held-out' and result['milestones_complete']:
        checked_completion(result, coverage, frozen)
    save(output / 'result.json', result)
    print(json.dumps({'projects': [{'id': row['project']['id'], 'complete': row['complete'], 'error': row['error']} for row in result['projects']],
                      'milestone_1_demonstrated': result['milestone_1_demonstrated'], 'milestone_2_demonstrated': result['milestone_2_demonstrated']}))
    accepted = (result['milestones_complete'] if args.phase == 'held-out' else
                bool(result['projects']) and all(row['complete'] is True and row['error'] is None for row in result['projects']))
    if not accepted:
        raise SystemExit(1)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--worker', type=Path)
    parser.add_argument('--phase', choices=('development', 'coverage', 'held-out'))
    parser.add_argument('--coverage-run')
    parser.add_argument('--output', type=Path)
    parser.add_argument('--fr', type=Path)
    arguments = parser.parse_args()
    if arguments.worker:
        request = json.loads(arguments.worker.read_text())
        {'analysis': analysis, 'cache': cache_controls}[request['mode']](request)
    else:
        flow.require(arguments.phase is not None and arguments.output is not None and arguments.fr is not None,
                     'collection requires explicit phase, output and analyzer binary')
        collect(arguments)
