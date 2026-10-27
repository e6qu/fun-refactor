"""Check generated syntax before committing it to a development branch."""
import argparse
from pathlib import Path
import re
import subprocess
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[1]


def check(binary, diagnostics):
    diagnostics.mkdir(parents=True, exist_ok=False)
    failures = []
    for fixture in ['lean-proof-locations.lean', 'lean-source-declarations.lean']:
        result = subprocess.run([str(binary), 'parse', '--xml', str(ROOT / 'tests/fixtures' / fixture)],
                                cwd=ROOT / 'grammars/lean', capture_output=True, text=True, timeout=120)
        (diagnostics / (fixture + '.xml')).write_text(result.stdout)
        (diagnostics / (fixture + '.stderr')).write_text(result.stderr)
        assert result.returncode in {0, 1}, result.stderr
        if result.returncode != 0:
            failures.append(fixture + ': syntax errors')
        start = result.stdout.index('<sources>')
        end = result.stdout.index('</sources>', start) + len('</sources>')
        tree = ET.fromstring(result.stdout[start:end])
        print('Syntax errors:', [(node.attrib, ''.join(node.itertext())[:100]) for node in tree.iter('ERROR')], flush=True)
        if fixture == 'lean-proof-locations.lean' and len(list(tree.iter('for_in'))) != 2:
            failures.append(fixture + ': expected two nested for loops')
        if fixture == 'lean-source-declarations.lean':
            expected = set(re.findall(r'(?m)^(?:def|theorem)\s+(\w+)', (ROOT / 'tests/fixtures' / fixture).read_text()))
            names = set()
            for definition in tree.iter('definition'):
                name = next(definition.iter('qualified_name'), None)
                if name is not None:
                    names.add(''.join(''.join(name.itertext()).split()))
            print('Recovered declarations:', sorted(names), flush=True)
            if names != expected:
                failures.append(fixture + ': declaration mismatch: ' + repr({'missing': sorted(expected - names), 'extra': sorted(names - expected)}))
        print('Checked', fixture, flush=True)
    for name, source in [
        ('if', 'def bad : Nat := Nat.succ if true then 1 else 0'),
        ('match', 'def bad : Nat := Nat.succ match true with | true => 1 | false => 0'),
        ('by', 'theorem bad : 0 = 0 := id by rfl'),
    ]:
        path = diagnostics / ('invalid-' + name + '.lean')
        path.write_text(source + '\n')
        result = subprocess.run([str(binary), 'parse', '--xml', str(path)],
                                cwd=ROOT / 'grammars/lean', capture_output=True, text=True, timeout=120)
        path.with_suffix('.xml').write_text(result.stdout)
        path.with_suffix('.stderr').write_text(result.stderr)
        if result.returncode != 1:
            failures.append(path.name + ': expected a syntax error')
        print('Checked rejection of', path.name, flush=True)
    assert not failures, '\n'.join(failures)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--tree-sitter', type=Path, required=True)
    parser.add_argument('--diagnostics', type=Path, required=True)
    args = parser.parse_args()
    check(args.tree_sitter.resolve(), args.diagnostics.resolve())
