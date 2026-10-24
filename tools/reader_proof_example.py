"""Execute the complete Lean lesson without adding Lean to Python-only checks."""
import argparse
import json
from pathlib import Path
import tempfile

from reader_examples import ROOT, blocks, run


def check(binary, folder):
    lesson = (ROOT / 'docs/first-lean-proof.md').read_text()
    manifest, = blocks(lesson, 'toml')
    source, = blocks(lesson, 'rust')
    tactics, = blocks(lesson, 'lean')
    (folder / 'src').mkdir()
    (folder / 'Cargo.toml').write_text(manifest)
    source_path = folder / 'src/lib.rs'
    source_path.write_text(source)
    commands, checked, evidence = 0, False, None
    for block in blocks(lesson, 'sh'):
        for line in block.splitlines():
            if not line.strip():
                continue
            command = shlex.split(line)
            assert command[0] == 'fr', command
            arguments = command[1:]
            redirect = None
            if '>' in arguments:
                at = arguments.index('>')
                assert arguments[at:] == ['>', 'formal-plan.json']
                redirect, arguments = folder / 'formal-plan.json', arguments[:at]
            model = folder / 'specs/FrSpecs/SrcLibRsKeep.lean'
            before = model.read_bytes() if model.exists() else None
            if 'proof-check' in arguments and not (folder / 'proof.lean').exists():
                (folder / 'proof.lean').write_text(tactics)
            result = run(binary, folder, arguments)
            commands += 1
            if redirect is not None:
                redirect.write_text(json.dumps(result))
            if 'proof-check' in arguments:
                assert result['passed'] is True, result
                assert model.read_bytes() == before
                checked = True
            if 'prove' in arguments and '--write' not in arguments:
                assert checked and model.read_bytes() == before
            if 'evidence' in arguments:
                evidence = result
    assert source_path.read_text() == source
    assert evidence is not None
    assert evidence['correspondence']['proved_implementation_model'] is False
    assert any(row['name'].endswith('keepModel_identity') and row['status'] == 'checked_by_lean'
               for row in evidence['properties'])
    return commands



if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--fr', required=True, type=Path)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix='fr-proof-reader-') as directory:
        commands = check(args.fr.resolve(), Path(directory))
    print(json.dumps({'first_proof_commands': commands}, sort_keys=True))
