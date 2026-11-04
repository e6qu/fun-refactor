"""Build a recorded implementation control before any unread project is opened."""
import argparse
import difflib
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'sdk/python/src'))
from fr_ir.runtime import FrClient, FrRuntimeError
from fr_ir.change_scope import ChangeScope
from fr_ir.ir import TaskChange
from fr_ir.structural_change import RefactorRequest
from fr_ir.investigation import Dependency, DependencyKind, TaskPlan, TaskStep
from fr_ir.context import MemoryObjectStore
from project_change_findings import satisfy
from project_change_sources import digest, save, bindings
from evidence_basis import native_source_bindings

COMPILER_IMAGE = 'gcc:15.2.0@sha256:c101370f78e4a30be178c11dd18aeee64c65d617908a98157db2392ca73ab04f'


def compiler_command(root, toolchain, registry, target):
    return ['docker', 'run', '--rm', '--network=none', '--pull=never',
        '--platform=linux/amd64', '--user', f'{os.getuid()}:{os.getgid()}',
        '--mount', f'type=bind,src={root},dst=/work',
        '--mount', f'type=bind,src={toolchain},dst=/rust,readonly',
        '--mount', f'type=bind,src={registry},dst=/cargo',
        '--mount', f'type=bind,src={target},dst=/build', '--workdir', '/work',
        '--env', 'PATH=/rust/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin',
        '--env', 'RUSTC=/rust/bin/rustc', '--env', 'CARGO_HOME=/cargo',
        '--env', 'CARGO_TARGET_DIR=/build', '--env', 'CARGO_NET_OFFLINE=true',
        '--env', 'CARGO_BUILD_JOBS=2', '--env', 'CARGO_INCREMENTAL=0',
        '--env', 'CARGO_PROFILE_DEV_DEBUG=0', '--env', 'CARGO_PROFILE_TEST_DEBUG=0',
        '--env', 'RUSTFLAGS=-D warnings', COMPILER_IMAGE,
        '/rust/bin/cargo', 'build', '--locked', '--bin', 'fr']


def compile_binary(destination):
    assert os.environ.get('GITHUB_ACTIONS') == 'true', 'compile checkers on GitHub runners'
    toolchain = Path(subprocess.check_output(['rustc', '--print', 'sysroot'], text=True).strip())
    assert '1.99.1' in subprocess.check_output(['rustc', '--version'], text=True)
    registry = Path(os.environ['CARGO_HOME']).resolve()
    subprocess.run(['cargo', 'fetch', '--locked'], cwd=ROOT, check=True, timeout=120)
    subprocess.run(['docker', 'pull', '--platform=linux/amd64', COMPILER_IMAGE], check=True, timeout=120)
    target = ROOT / 'target/project-change-pinned'
    target.mkdir(parents=True, exist_ok=True)
    subprocess.run(compiler_command(ROOT, toolchain, registry, target),
                   cwd=ROOT, check=True, timeout=180)
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(target / 'debug/fr', destination)
    destination.chmod(0o755)


def verify(binary, root, accepted):
    client = FrClient(root, executable=binary, timeout=20, max_output_bytes=1048576)
    handle = client.project('find', 'original').definition_target().handle
    scope = ChangeScope.inspect(client, [handle], depth=4, nodes=128,
        references=512, max_bytes=65536)
    assert scope.ready, scope.report.to_data()
    change = TaskChange([], [RefactorRequest.rename('n' * 256, documentation='preserve').target('rename', handle)],
                        {'files-changed': 1}, ['syntax'])
    try:
        report = client.review(scope.bind(change)).to_data()
    except FrRuntimeError as error:
        assert not accepted and '255' in str(error), str(error)
        return {'accepted': False, 'error': str(error), 'report': error.report}
    assert accepted, 'alternate checker did not change real rename admission'
    return {'accepted': True, 'report': report}


def build(output):
    output.mkdir(parents=True, exist_ok=False)
    source = ROOT / 'src/project/structural_change.rs'
    original = source.read_text()
    old = 'name.len() <= 256, "rename name exceeds 256 bytes."'
    new = 'name.len() <= 255, "rename name exceeds 255 bytes."'
    assert original.count(old) == 1
    changed = original.replace(old, new)
    (output / 'implementation.patch').write_text(''.join(difflib.unified_diff(
        original.splitlines(keepends=True), changed.splitlines(keepends=True),
        fromfile='a/src/project/structural_change.rs', tofile='b/src/project/structural_change.rs')))
    compile_binary(output / 'production-fr')
    production = native_source_bindings(ROOT)
    investigation = digest(ROOT / 'src/project/investigation.rs')
    try:
        source.write_text(changed)
        alternate = native_source_bindings(ROOT)
        compile_binary(output / 'alternate-fr')
    finally:
        source.write_text(original)
    assert digest(ROOT / 'src/project/investigation.rs') == investigation
    assert production != alternate
    with tempfile.TemporaryDirectory(prefix='fr-checker-control-') as temporary:
        root = Path(temporary)
        (root / 'control.rs').write_text('fn original() -> i64 { 1 }\n')
        (root / '.fr').mkdir()
        save(root / '.fr/checks.json', {'schema': 1, 'checks': [{'name': 'syntax',
            'argv': ['rustc', '--crate-type', 'lib', '--emit=metadata', 'control.rs', '-o', '.fr/control.rmeta'],
            'cwd': '.', 'timeout_seconds': 30, 'covers': ['Compile the control library']}]})
        save(root / '.fr/check-scopes.json', {'schema': 1,
            'checks': [{'name': 'syntax', 'paths': ['control.rs']}]})
        controls = {name: verify(output / (name + '-fr'), root, name == 'production')
                    for name in ('production', 'alternate')}
        production_client = FrClient(root, executable=output / 'production-fr', timeout=20, max_output_bytes=1048576)
        plan = TaskPlan('Check implementation identity without a workspace dependency',
            ('analyzer checked', 'declarations checked', 'source checked'), (
                TaskStep('analyzer', 'Which checker implementation was used?', (
                    Dependency(DependencyKind.ANALYZER, 'implementation'),), satisfies=('analyzer checked',)),
                TaskStep('declarations', 'Which declaration checker was used?', (
                    Dependency(DependencyKind.DECLARATION_ANALYZER, 'implementation'),), satisfies=('declarations checked',)),
                TaskStep('source', 'Is the original source file unchanged?', (
                    Dependency(DependencyKind.SOURCE, 'control.rs'),), satisfies=('source checked',)),
            ))
        for name in ('analyzer', 'declarations', 'source'):
            plan = satisfy(plan, production_client, name, 'recorded public checker control')
        store = MemoryObjectStore()
        saved = plan.store(store)
        resumed = TaskPlan.restore(store, saved).resume(FrClient(root, executable=output / 'alternate-fr',
            timeout=20, max_output_bytes=1048576))
        assert resumed.invalidated == ('analyzer', 'declarations')
        assert resumed.plan.steps[2] == plan.steps[2]
        invalidation = {'baseline': plan.to_data(), 'resumed': resumed.report.to_data(),
            'invalidated': list(resumed.invalidated), 'independent_source_retained': True}
    save(output / 'checkers.json', {'production': {'sha256': digest(output / 'production-fr'),
        'source_bindings': production}, 'alternate': {'sha256': digest(output / 'alternate-fr'),
        'source_bindings': alternate}, 'unchanged_investigation_sha256': investigation,
        'compiler_image': COMPILER_IMAGE,
        'source_commit': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
        'source_bindings': bindings(ROOT),
        'invalidation_control': invalidation,
        'control': controls, 'scope': 'Same public format and investigation source. The alternate implementation admits names up to 255 bytes; production admits 256. No production limit was raised.'})


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=['compile', 'checkers'])
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    if args.action == 'compile':
        compile_binary(args.output.resolve())
    else:
        build(args.output.resolve())
