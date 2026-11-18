"""Connect qualified product tasks to the existing native collector and admission."""
import hashlib
import json
import math
from pathlib import Path
import shlex
import shutil
import sys

from . import product_plan as plan, selected_tasks as common
from .selected_plan import ROOT, encoded, prepare, require

CONTROL_LIMITS = {'wall_seconds': 60, 'rss_bytes': 768 * 1024**2, 'cpu_limit_seconds': 25,
                  'disk_bytes': 64 * 1024**2, 'transcript_bytes': 128 * 1024}
RESERVE_BYTES = 128 * 1024**2
ADMISSION = {'conditions': ['files', 'fr'], 'complete_workflow_required': True,
             'session_accounting_required': True, 'independent_patch_replay_required': True,
             'client_context_reserve_bytes': RESERVE_BYTES, 'control_limits': CONTROL_LIMITS}


def checkers_path():
    return ROOT / plan.DIRECTORY / 'checkers.json'


def public_checks(task, checkers):
    if task.endswith('-proof'):
        return [{'name': 'lean', 'argv': [checkers['lake']['path'], '-d', 'specs', 'build', '--wfail'],
                 'cwd': '.', 'timeout_seconds': 120, 'covers': ['fixed executable Lean theorem without proof gaps']}]
    return [{'name': 'public-task', 'argv': [checkers['python']['path'], '-B', '.fr/product-check.py', task],
             'cwd': '.', 'timeout_seconds': 120, 'covers': ['public task examples; independent completeness is graded separately']}]


def install(project, task, checkers):
    directory = project / '.fr'
    directory.mkdir(exist_ok=True)
    common.save(directory / 'checks.json', {'schema': 1, 'checks': public_checks(task, checkers)})
    files = [directory / 'checks.json']
    if not task.endswith('-proof'):
        target = directory / 'product-check.py'
        shutil.copyfile(ROOT / 'tools/agent_eval/product_public.py', target)
        files.append(target)
    return {str(path.relative_to(project)): common.digest(path) for path in files}


def validate_controls(receipt, study, *, verify_commit=True):
    from . import selected_local as local
    schedule = prepare(ROOT, study)
    tasks = schedule['requirements']['tasks']
    require(receipt.get('schema') == 'fr-product-private-controls-1' and receipt.get('passed') is True and
            receipt.get('live_model_calls') == 0 and receipt.get('study') == study and
            isinstance(receipt.get('run'), str) and receipt['run'].isdigit() and
            isinstance(receipt.get('source_commit'), str) and len(receipt['source_commit']) == 40,
            'require the successful hosted qualification for this exact product round')
    require(receipt.get('bindings') == local.implementation_bindings(study), 'product qualification inputs changed')
    require(set(receipt.get('seeds', {})) == set(tasks), 'qualify all four unchanged product seeds')
    require(set(receipt.get('reference_patches', {})) == set(tasks), 'bind every supplied admission patch')
    expected = {task + '/' + name for task, names in receipt.get('variants', {}).items() for name in names}
    require(set(receipt.get('variants', {})) == set(tasks) and expected == set(receipt['controls']) and
            all(len(names) >= 7 and {'reference', 'missing', 'empty', 'invalid'} <= set(names)
                for names in receipt['variants'].values()), 'retain positive, malformed and flawed-answer controls')
    for identity, row in receipt['controls'].items():
        require(row['status'] == 'passed' and row['accepted_change'] is identity.endswith('/reference'),
                'every private control must reach its expected verdict')
    if verify_commit:
        for name, digest in receipt['bindings'].items():
            content = common.comparison_grade.BASE.git(ROOT, 'show', receipt['source_commit'] + ':' + name).stdout
            require(hashlib.sha256(content).hexdigest() == digest, 'qualification commit differs from its bound source')
    return receipt


def admission_spec(study):
    value = prepare(ROOT, study)['requirements']['native_admission']
    require(value == ADMISSION, 'preserve complete native admission and its resource reserve')
    return {'kind': 'product', 'features': ('independent_patch_replay',),
            'conditions': tuple(value['conditions']), 'by_condition': True, 'requirements': value}


def admission_script(task, setup, checkers, search_path):
    require(task in plan.TASKS['initial'] and setup in ('files', 'fr'), 'choose a qualified initial task and setup')
    lines = ['set -eu', 'export PATH=' + shlex.quote(search_path)]
    if setup == 'fr':
        lines.append('cat .agents/skills/fr/SKILL.md > /dev/null')
        if task == 'clamp-proof':
            arguments = ['fr', '--json', 'proof', 'start', 'specs/FrSpecs/Clamp.lean::clamp_bounds',
                         '--draft', '.fr/admission-draft', '--bytes', '4096', '--timeout-seconds', '20']
        else:
            name, path = {'utils-rename': ('to_int', 'python_utils/converters.py'),
                          'sqlparse-graph': ('parsestream', 'sqlparse/__init__.py'),
                          'slugify-bytes': ('slugify', 'slugify/slugify.py')}[task]
            arguments = ['fr', '--json', 'project', 'find', name, '--in', path, '--signature', '--bytes', '2048']
        lines.append(shlex.join(arguments) + ' > .fr/admission-discovery.json')
    lines += ['git apply .fr/admission-reference.patch']
    lines += [shlex.join(row['argv']) for row in public_checks(task, checkers)]
    # New feature tests/docs and graph output must enter the delivered patch too.
    lines += ['git add -N .', 'git diff --binary HEAD > change.patch', 'test -s change.patch',
              "printf 'COMPLETE_ALGORITHM_WORKFLOW\\n'"]
    return '\n'.join(lines) + '\n'


def install_admission(args, project, seed, receipt):
    require(seed['task'] in prepare(ROOT, args.study)['requirements']['tasks'] and
            receipt['seeds'][seed['task']] == seed['archive_sha256'], 'admit the exact qualified seed')
    checkers = {'python': {'path': str(Path(sys.executable))},
                'lake': {'path': str(args.lean_root / 'bin/lake')}}
    protected = install(project, seed['task'], checkers)
    if args.setup == 'fr':
        shutil.copytree(ROOT / 'skills/fr', project / '.agents/skills/fr')
        protected.update({str(path.relative_to(project)): common.digest(path)
                          for path in (project / '.agents/skills/fr').rglob('*') if path.is_file()})
    patch = args.inputs / 'reference.patch'
    require(common.digest(patch) == receipt['reference_patches'][seed['task']], 'admission reference differs from qualified patch')
    shutil.copyfile(patch, project / '.fr/admission-reference.patch')
    protected['.fr/admission-reference.patch'] = common.digest(patch)
    return {name: {'sha256': digest, 'mode': (project / name).stat().st_mode & 0o7777}
            for name, digest in protected.items()}, checkers


def validate_receipts(receipts, binaries, seeds, qualified_run, study, checkers):
    from . import algorithm_admission as native, selected_local as local
    from .selected_guarded import GUARD
    spec, schedule = admission_spec(study), prepare(ROOT, study)
    expected = {profile + '/' + task + '/' + setup: (profile, task, setup)
                for profile in schedule['profiles'] for task in schedule['requirements']['tasks']
                for setup in spec['conditions']}
    require(set(receipts) == set(expected), 'admit all 24 native profile, task and setup combinations')
    implementation = local.implementation_bindings(study)
    checker_hashes = {key: checkers[key]['sha256'] for key in ('lean', 'lake', 'leanc')}
    runtime = {'executable_sha256': checkers['python']['sha256'],
               'requirements_sha256': common.digest(native.NATIVE_REQUIREMENTS), 'packages': native.server_packages()}
    ceilings = {'elapsed_seconds': 60, 'sampled_aggregate_rss_bytes': CONTROL_LIMITS['rss_bytes'] - RESERVE_BYTES,
                'sampled_cpu_seconds': 25, 'sampled_disk_growth_bytes': CONTROL_LIMITS['disk_bytes'],
                'transcript_bytes': CONTROL_LIMITS['transcript_bytes']}
    for key, (profile, task, setup) in expected.items():
        row = receipts[key]
        require(all(row.get(name) is True for name in ('passed', 'complete_workflow', 'session_accounting', 'independent_patch_replay')) and
                row.get('study') == study and row.get('profile') == profile and row.get('task') == task and row.get('setup') == setup and
                row.get('live_model_calls') == 0 and row.get('upstream_forwarding') is False and
                row.get('scripted_requests') == 2 and row.get('scripted_errors') == 0, 'incomplete native product admission: ' + key)
        require(row.get('implementation') == implementation and row.get('qualified_run') == qualified_run and
                row.get('archive_sha256') == seeds[task]['receipt']['archive_sha256'] and
                row.get('fr_sha256') == binaries['fr']['sha256'] and
                row.get('client_sha256') == binaries[schedule['profiles'][profile]['client']]['sha256'] and
                row.get('guard_sha256') == common.digest(GUARD) and row.get('checker_sha256') == checker_hashes and
                row.get('python_runtime') == runtime and row.get('client_context_reserve_bytes') == RESERVE_BYTES,
                'native admission source, tools, seed or resource policy changed: ' + key)
        resources = row['resources']
        require(resources['limits'] == {'wall_seconds': 60, 'rss_bytes': CONTROL_LIMITS['rss_bytes'],
                'cpu_seconds': 25, 'disk_bytes': CONTROL_LIMITS['disk_bytes'], 'transcript_bytes': CONTROL_LIMITS['transcript_bytes']} and
                resources['exit_code'] == 0 and all(resources[name] is None for name in ('stop_reason', 'monitor_error', 'launch_error')),
                'native admission was interrupted or its limits changed')
        for name, maximum in ceilings.items():
            value = resources[name]
            require(type(value) in (int, float) and 0 <= value <= maximum and math.isfinite(value),
                    'native admission lacks its resource reserve: ' + key + '/' + name)
    return {'receipts_sha256': hashlib.sha256(encoded(receipts)).hexdigest(),
            'implementation_sha256': hashlib.sha256(encoded(implementation)).hexdigest(), 'admitted_combinations': len(expected)}


def attach_freeze(value, runtime, inputs, study):
    from . import selected_local as local
    from .selected_guarded import contract
    receipt = validate_controls(json.loads(runtime['controls'].read_text()), study)
    require(all(value['seeds'][task]['receipt']['archive_sha256'] == digest for task, digest in receipt['seeds'].items()),
            'execution seeds differ from their hosted qualification')
    value['fr_build'] = local.verified_fr_build(value['source'], value['binaries']['fr']['sha256'], runtime['fr_build'])
    value['controls'] = receipt
    value['resource_monitor'] = contract()
    value['admission'] = validate_receipts(json.loads(runtime['admission'].read_text()), value['binaries'],
        value['seeds'], receipt['run'], study, value['checkers'])
