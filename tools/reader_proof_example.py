"""Execute the complete Lean lesson without adding Lean to Python-only checks."""
import argparse
import json
import shlex
import subprocess
import sys
from pathlib import Path
import tempfile

from reader_examples import ROOT, blocks, run


def check(binary, folder):
    lesson = (ROOT / 'docs/first-lean-proof.md').read_text()
    manifest, = blocks(lesson, 'toml')
    source, = blocks(lesson, 'rust')
    bad_tactics, tactics = blocks(lesson, 'lean')
    property_author, = blocks(lesson, 'python')
    (folder / 'src').mkdir()
    (folder / 'Cargo.toml').write_text(manifest)
    source_path = folder / 'src/lib.rs'
    source_path.write_text(source)
    (folder / 'make-property.py').write_text(property_author)
    commands, checked, rejected, evidence = 0, False, False, None
    for block in blocks(lesson, 'sh'):
        for line in block.splitlines():
            if not line.strip():
                continue
            command = shlex.split(line)
            if command == ['python3', 'make-property.py']:
                subprocess.run([sys.executable, '-B', 'make-property.py'], cwd=folder,
                               check=True, capture_output=True, timeout=10)
                commands += 1
                continue
            assert command[0] == 'fr', command
            arguments = command[1:]
            redirect = None
            if '>' in arguments:
                at = arguments.index('>')
                assert arguments[at + 1] in {'formal-plan.json', 'property-task.json'} and len(arguments[at:]) == 2
                redirect, arguments = folder / arguments[at + 1], arguments[:at]
            model = folder / 'specs/FrSpecs/SrcLibRsAllowed.lean'
            before = model.read_bytes() if model.exists() else None
            if 'proof-check' in arguments:
                filename = arguments[arguments.index('--from') + 1]
                assert filename in {'bad-proof.lean', 'proof.lean'}
                (folder / filename).write_text(bad_tactics if filename == 'bad-proof.lean' else tactics)
            result = run(binary, folder, arguments)
            commands += 1
            if redirect is not None:
                redirect.write_text(json.dumps(result))
            if 'proof-check' in arguments:
                assert model.read_bytes() == before
                if filename == 'bad-proof.lean':
                    assert result['passed'] is False and result['receipt'] is None and result['diagnostics'], result
                    rejected = True
                else:
                    assert rejected and result['passed'] is True and result['receipt'], result
                    checked = True
            if 'prove' in arguments and '--write' not in arguments:
                assert checked and model.read_bytes() == before
            if 'evidence' in arguments:
                evidence = result
    assert source_path.read_text() == source
    assert checked and rejected and evidence is not None
    assert evidence['correspondence']['proved_implementation_model'] is False
    assert any(row['name'].endswith('allowedModel_access_rule') and row['status'] == 'checked_by_lean'
               for row in evidence['properties'])
    return commands


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--fr', required=True, type=Path)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix='fr-proof-reader-') as directory:
        commands = check(args.fr.resolve(), Path(directory))
    print(json.dumps({'first_proof_commands': commands}, sort_keys=True))
