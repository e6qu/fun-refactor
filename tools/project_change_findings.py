"""Reopen saved discoveries in separate processes after explicit project changes."""
from dataclasses import replace
import json
from pathlib import Path
import shutil
import sys
import time

from fr_ir.change_scope import ChangeScope
from fr_ir.context import DirectoryObjectStore
from fr_ir.investigation import Dependency, DependencyKind, Evidence, EvidenceKind, TaskPlan, TaskStep, StepState
from fr_ir.runtime import FrClient
from project_change_sources import save, digest
from project_change_delivery import rows, target
from agent_eval.bounded_host import run as bounded_run

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / 'tools/project-changes.py'


def discovery(client, question):
    report = client.project('find', question['name'], '--source', '--bytes', '65536')
    candidates = rows(report)
    scopes = [ChangeScope.inspect(client, [row['handle']], depth=question['depth'],
        nodes=128, references=512, max_bytes=262144).report.to_data() for row in candidates]
    added = client.project('find', question['added_name'], '--bytes', '65536')
    rows(added)
    return {'request': question, 'find': report.to_data(), 'added_lookup': added.to_data(), 'scopes': scopes,
        'scope': 'Indexed declarations and consumers only. Scope gaps and unresolved references remain in the returned reports.'}


def satisfy(plan, client, step, reference):
    started = plan.resume(client, transition=f'{step}:start')
    evidence = Evidence('observation', EvidenceKind.OBSERVATION, started.input_digests[step], True, reference)
    attached = replace(started.plan, steps=tuple(replace(item, evidence=(evidence,))
        if item.id == step else item for item in started.plan.steps))
    return attached.resume(client, transition=f'{step}:satisfy').plan


def reopen(args):
    request = json.loads(args.request.read_text())
    client = FrClient(args.project, executable=args.fr, timeout=20, max_output_bytes=2097152)
    store = DirectoryObjectStore(args.objects)
    if args.plan:
        plan = TaskPlan.restore(store, args.plan)
        if args.change_settings:
            plan = replace(plan, steps=tuple(replace(step, question=json.dumps(request, sort_keys=True))
                if step.id == 'selected' else step for step in plan.steps))
        resumed = plan.resume(client)
        assert resumed.invalidated == ('selected',), resumed.report.to_data()
        independent = next(step for step in resumed.plan.steps if step.id == 'independent')
        assert independent.state == StepState.SATISFIED
        assert independent.evidence == plan.steps[1].evidence
        save(args.output / 'resumed.json', resumed.report.to_data())
        observed = discovery(client, request)
        save(args.output / 'observation.json', observed)
        finished = satisfy(resumed.plan, client, 'selected', 'sha256:' + digest(args.output / 'observation.json'))
        assert finished.resume(client).complete
        save(args.output / 'plan.json', {'root': finished.store(store), 'independent_retained': True})
    else:
        save(args.output / 'observation.json', discovery(client, request))


def mutate(task, root, selected, case):
    path = root / selected['path']
    name = task['function']
    suffix = path.suffix
    if case == 'rename':
        for source in root.rglob('*' + suffix):
            source.write_text(source.read_text().replace(name, task['renamed']))
    elif case == 'move':
        destination = path.with_name('fr_moved_' + path.name)
        path.rename(destination)
        if suffix == '.rs':
            manifest = root / 'Cargo.toml'
            manifest.write_text(manifest.read_text() + '\n[lib]\npath = "' + str(destination.relative_to(root)) + '"\n')
        else:
            old_module, new_module = path.stem, destination.stem
            for source in root.rglob('*.py'):
                text = source.read_text()
                for prefix in ('from .', 'from ', '.'):
                    text = text.replace(prefix + old_module + ' import', prefix + new_module + ' import')
                source.write_text(text)
    elif case == 'delete':
        path.unlink()
    elif case == 'competing-import':
        if suffix == '.py':
            (root / 'fr_competitor.py').write_text(f'def {name}(*args, **kwargs):\n    return None\n'
                f'\ndef fr_added_{name}(*args, **kwargs):\n    return {name}(*args, **kwargs)\n')
            with path.open('a') as stream:
                stream.write(f'\nfrom fr_competitor import {name}\n')
        else:
            with path.open('a') as stream:
                stream.write(f'\nmod fr_competitor {{ pub fn {name}() {{}} pub fn fr_added_{name}() {{ {name}(); }} }}\nuse fr_competitor::{name};\n')
    elif case == 'project-configuration':
        manifest = root / '.fr/checks.json'
        checks = json.loads(manifest.read_text())
        checks['checks'][0]['argv'] = [sys.executable, '-B', '-c',
            'import pathlib; p=pathlib.Path(' + repr(selected['path']) + '); assert p.is_file() and p.stat().st_size>0']
        checks['checks'][0]['covers'] = ['Require the selected source to be present and nonempty']
        save(manifest, checks)
    elif case not in ('question-settings', 'analyzer'):
        raise ValueError(case)


def measure(task, original, output, binary, alternate):
    started = time.monotonic()
    output.mkdir(parents=True, exist_ok=False)
    baseline_root = output / 'baseline'
    shutil.copytree(original, baseline_root)
    client = FrClient(baseline_root, executable=binary, timeout=20, max_output_bytes=2097152)
    selected, found = target(client, task['function'], task['language'])
    configure(baseline_root, selected)
    save(output / 'discovery.json', found)
    suffix = '.rs' if task['language'] == 'Rust' else '.py'
    independent = next(path for path in sorted(baseline_root.rglob('*'))
        if path.is_file() and path.suffix in ('.rs', '.py', '.md')
        and str(path.relative_to(baseline_root)) != selected['path']
        and task['function'].encode() not in path.read_bytes()
        and (suffix != '.py' or Path(selected['path']).stem.encode() not in path.read_bytes()))
    relative = str(independent.relative_to(baseline_root))
    question = {'name': task['function'], 'added_name': 'fr_added_' + task['function'],
                'depth': 4, 'scope': task['question']}
    configuration = 'Cargo.toml' if suffix == '.rs' else 'pyproject.toml'
    plan = TaskPlan('Keep original-project discovery current', ('selected observed', 'independent observed'), (
        TaskStep('selected', json.dumps(question, sort_keys=True), (
            Dependency(DependencyKind.WORKSPACE, 'original-project'),
            Dependency(DependencyKind.LOOKUP, task['function']),
            Dependency(DependencyKind.LOOKUP, question['added_name']),
            Dependency(DependencyKind.CONFIGURATION, configuration),
            Dependency(DependencyKind.CHECK_CONFIGURATION, 'selected-project'),
            Dependency(DependencyKind.ANALYZER, 'native-implementation')),
            satisfies=('selected observed',)),
        TaskStep('independent', 'Retain this unchanged original source observation', (
            Dependency(DependencyKind.SOURCE, relative),), satisfies=('independent observed',)),
    ))
    store = DirectoryObjectStore(output / 'objects')
    observation = discovery(client, question)
    assert not observation['added_lookup']['rows'], 'missing-name control already exists'
    save(output / 'baseline-observation.json', observation)
    plan = satisfy(plan, client, 'selected', 'sha256:' + digest(output / 'baseline-observation.json'))
    plan = satisfy(plan, client, 'independent', 'sha256:' + digest(independent))
    saved = plan.store(store)
    save(output / 'baseline-plan.json', {'root': saved, 'independent_path': relative,
        'independent_sha256': digest(independent)})
    results = []
    for case in ('rename', 'move', 'delete', 'competing-import', 'project-configuration', 'question-settings', 'analyzer'):
        case_output = output / case
        case_output.mkdir()
        # Reuse this exact root path: public handles bind the original workspace.
        shutil.rmtree(baseline_root)
        shutil.copytree(original, baseline_root)
        configure(baseline_root, selected)
        mutate(task, baseline_root, selected, case)
        request = {**question, 'depth': 1} if case == 'question-settings' else question
        request_path = case_output / 'request.json'
        save(request_path, request)
        executable = alternate if case == 'analyzer' else binary
        args = [sys.executable, '-B', str(SCRIPT), 'reopen', '--fr', str(executable),
            '--project', str(baseline_root), '--objects', str(output / 'objects'), '--request', str(request_path)]
        for route in ('saved', 'fresh'):
            destination = case_output / route
            command = args + ['--output', str(destination)]
            if route == 'saved':
                command += ['--plan', saved]
                if case == 'question-settings':
                    command += ['--change-settings']
            with (case_output / (route + '.stdout')).open('wb') as stdout, \
                 (case_output / (route + '.stderr')).open('wb') as stderr:
                measured = bounded_run(command, b'', stdout, stderr, output,
                    wall_seconds=20, cpu_limit_seconds=10, rss_bytes=512 * 1024**2,
                    disk_bytes=16 * 1024**2, transcript_bytes=1024**2, cwd=ROOT)
            save(case_output / (route + '-process.json'), {'argv': command, 'measurement': measured})
            assert measured['exit_code'] == 0 and measured['stop_reason'] is None, (task['id'], case, route, measured)
        saved_observation = json.loads((case_output / 'saved/observation.json').read_text())
        fresh_observation = json.loads((case_output / 'fresh/observation.json').read_text())
        assert saved_observation == fresh_observation, (task['id'], case, 'saved and fresh disagree')
        if case == 'competing-import':
            assert fresh_observation['added_lookup']['rows'], 'newly available name was missed'
        assert digest(baseline_root / relative) == digest(original / relative)
        changed = {}
        for path in baseline_root.rglob('*'):
            relative_path = path.relative_to(baseline_root)
            if not path.is_file() or (relative_path.parts[0] == '.fr'
                    and relative_path.as_posix() not in ('.fr/checks.json', '.fr/check-scopes.json')):
                continue
            old = original / relative_path
            if not old.is_file() or digest(path) != digest(old):
                destination = case_output / 'changed-sources' / relative_path
                destination.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(path, destination)
                changed[relative_path.as_posix()] = digest(path)
        deleted = [str(path.relative_to(original)) for path in original.rglob('*')
                   if path.is_file() and not (baseline_root / path.relative_to(original)).is_file()]
        save(case_output / 'source-changes.json', {'changed': changed, 'deleted': deleted})
        results.append({'case': case, 'passed': True, 'identical_saved_and_fresh': True,
            'independent_retained': True, 'binary_sha256': digest(executable)})
    elapsed = time.monotonic() - started
    assert elapsed <= 180, 'original project change matrix exceeds its task budget'
    result = {'project': task['id'], 'passed': True, 'cases': results, 'seconds': elapsed}
    save(output / 'result.json', result)
    shutil.rmtree(baseline_root)
    return result


def configure(root, selected):
    (root / '.fr').mkdir(exist_ok=True)
    save(root / '.fr/checks.json', {'schema': 1, 'checks': [{'name': 'source-present',
        'argv': [sys.executable, '-B', '-c', 'import pathlib; assert pathlib.Path(' + repr(selected['path']) + ').is_file()'],
        'cwd': '.', 'timeout_seconds': 10, 'covers': ['Require the selected original source to exist']}]})
    save(root / '.fr/check-scopes.json', {'schema': 1,
        'checks': [{'name': 'source-present', 'paths': [selected['path']]}]})
