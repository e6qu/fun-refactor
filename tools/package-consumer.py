"""Exercise an installed fr binary or SDK outside the source checkout."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

SOURCE = 'def leaf():\n    return 7\n\ndef main():\n    return leaf()\n'


def run(arguments, root, *, data=None, succeeds=True):
    result = subprocess.run(list(map(str, arguments)), cwd=root, input=data, capture_output=True, timeout=30)
    if (result.returncode == 0) != succeeds:
        raise AssertionError(f'consumer command returned {result.returncode}: {result.stderr.decode(errors="replace")[:2000]}')
    return result.stdout


def graph(report, source, name):
    nodes = {row['name']: row for row in report['nodes']}
    assert set(nodes) == {name, 'main'}
    assert len(report['edges']) == 1
    edge, = report['edges']
    assert edge['from'] == nodes['main']['id'] and edge['to'] == nodes[name]['id']
    assert report['source_digests']['app.py'] == hashlib.sha256(source.encode()).hexdigest()
    assert not report['coverage']['gaps'] and not report['unresolved']


def exercise(binary, version, sdk):
    with tempfile.TemporaryDirectory(prefix='fr-installed-consumer-') as temporary:
        root = Path(temporary)
        os.environ['FUN_REFACTOR_CACHE'] = str(root / '.fr/cache')
        source = root / 'app.py'
        source.write_text(SOURCE)
        installed = None
        if sdk:
            from importlib.metadata import version as distribution_version
            import fr_ir
            from fr_ir.runtime import FrClient, FrRuntimeError
            assert Path(fr_ir.__file__).resolve().is_relative_to(Path(sys.prefix).resolve()), 'SDK import escaped the consumer environment'
            client = FrClient(root, executable=binary, timeout=30)
            compatibility = client.compatibility()
            installed = distribution_version('fun-refactor-ir')
            assert installed == compatibility.version == version

            def call(*arguments, succeeds=True):
                try:
                    value = client.call(*arguments).to_data()
                except FrRuntimeError as error:
                    if succeeds or error.exit_code in (None, 0):
                        raise
                    return None
                assert succeeds, 'a refused change unexpectedly succeeded'
                return value
        else:
            assert run([binary, '--version'], root).decode().strip() == 'fr ' + version
            capabilities = json.loads(run([binary, '--json', 'capabilities'], root))
            assert capabilities and any(row['capability'] == 'rename' for row in capabilities)

            def call(*arguments, succeeds=True):
                output = run([binary, '--json', '-C', root, *arguments], root, succeeds=succeeds)
                return json.loads(output) if succeeds else None

        graph(call('graph'), SOURCE, 'leaf')
        plan = call('rename', 'leaf', 'renamed', '--save-plan')
        assert plan['applied'] is False and source.read_text() == SOURCE
        transaction = str(plan['transaction'])
        call('history', 'apply', transaction, '--write')
        changed = SOURCE.replace('leaf', 'renamed')
        assert source.read_text() == changed
        graph(call('graph'), changed, 'renamed')
        run([sys.executable, '-B', '-c', 'import app; assert app.main() == 7'], root)
        call('history', 'undo', transaction, '--write')
        assert source.read_text() == SOURCE
        call('history', 'redo', transaction, '--write')
        assert source.read_text() == changed
        patch = call('history', 'patch', transaction)['patch'].encode()
        assert patch.startswith(b'diff --git ')
        receiver = root / 'receiver'
        receiver.mkdir()
        (receiver / 'app.py').write_text(SOURCE)
        run(['git', 'init', '-q'], receiver)
        run(['git', 'apply', '--check', '-'], receiver, data=patch)
        run(['git', 'apply', '-'], receiver, data=patch)
        assert (receiver / 'app.py').read_text() == changed
        run([sys.executable, '-B', '-c', 'import app; assert app.main() == 7'], receiver)
        run(['git', 'apply', '--reverse', '-'], receiver, data=patch)
        assert (receiver / 'app.py').read_text() == SOURCE
        # A stale preview must not overwrite the consumer's later edit.
        shutil.rmtree(receiver)
        pending = call('rename', 'renamed', 'final_name', '--save-plan')
        drift = changed.replace('return 7', 'return 8')
        source.write_text(drift)
        call('history', 'apply', str(pending['transaction']), '--write', succeeds=False)
        assert source.read_text() == drift
        graph(call('graph'), drift, 'renamed')
        return {'passed': True, 'version': version, 'distribution_version': installed,
                'checks': ['graph-and-source-digests', 'read-only-preview', 'rename-and-behavior',
                           'fresh-graph-after-edit', 'undo-redo', 'fresh-receiver-patch', 'stale-preview-refusal']}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--fr', type=Path, required=True)
    parser.add_argument('--version', required=True)
    parser.add_argument('--sdk', action='store_true')
    args = parser.parse_args()
    print(json.dumps(exercise(args.fr.resolve(), args.version, args.sdk), sort_keys=True))
