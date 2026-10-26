"""Check generated syntax before committing it to a development branch."""
import argparse
from pathlib import Path
import subprocess
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[1]
EXPECTED = {
    'slice_is_maximal', 'slice_and_remaining_partition', 'slice_zero_budget',
    'slice_finishes_when_budget_fits', 'slice_advances_to_available_boundary',
    'sourcePageLengths', 'prefix_length_bounded', 'page_preserves_rows',
    'page_respects_shared_budget', 'page_budget_partition', 'page_zero_budget',
}


def check(binary):
    for fixture, require_clean in [('lean-proof-locations.lean', True), ('lean-source-declarations.lean', False)]:
        result = subprocess.run([str(binary), 'parse', '--xml', str(ROOT / 'tests/fixtures' / fixture)],
                                cwd=ROOT / 'grammars/lean', capture_output=True, text=True, timeout=120)
        if require_clean:
            assert result.returncode == 0, result.stdout[-4096:] + result.stderr[-4096:]
        else:
            assert result.returncode in {0, 1}, result.stderr
        start = result.stdout.index('<sources>')
        end = result.stdout.index('</sources>', start) + len('</sources>')
        tree = ET.fromstring(result.stdout[start:end])
        if fixture == 'lean-source-declarations.lean':
            names = set()
            for definition in tree.iter('definition'):
                name = next(definition.iter('qualified_name'), None)
                if name is not None:
                    names.add(''.join(''.join(name.itertext()).split()))
            assert EXPECTED <= names, 'lost later source declarations: ' + repr(sorted(EXPECTED - names))
        print('Checked', fixture, flush=True)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--tree-sitter', type=Path, required=True)
    check(parser.parse_args().tree_sitter.resolve())
