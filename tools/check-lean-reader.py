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
        if fixture == 'lean-proof-locations.lean':
            for kind, count in [('do_while', 2), ('do_unless', 1), ('let_bind', 2),
                                ('tactic_rcases', 2), ('rcases_tuple_pattern', 1)]:
                if len(list(tree.iter(kind))) != count:
                    failures.append(fixture + ': expected ' + str(count) + ' ' + kind + ' statements')
            checked = set()
            for definition in tree.iter('definition'):
                name = definition.find('qualified_name')
                if name is None:
                    continue
                text = ''.join(''.join(name.itertext()).split())
                if text not in {'changeComparison', 'changeBeforeSplit'}:
                    continue
                checked.add(text)
                body = definition.find('by')
                tactic = body.find('tactic_apply') if body is not None else None
                if tactic is None or tactic.find('binary_expression') is None:
                    failures.append(text + ': comparison escaped the proof tactic')
            if checked != {'changeComparison', 'changeBeforeSplit'}:
                failures.append(fixture + ': missing comparison proof declarations')
            shown = set()
            for definition in tree.iter('definition'):
                name = definition.find('qualified_name')
                if name is None:
                    continue
                text = ''.join(''.join(name.itertext()).split())
                if text not in {'showComparison', 'showFrom'}:
                    continue
                shown.add(text)
                term = next(definition.iter('show'), None)
                if term is None or term.find('binary_expression') is None:
                    failures.append(text + ': expected a show term with a comparison type')
                elif text == 'showComparison' and term.find('by') is None:
                    failures.append(text + ': proof block escaped the show term')
            if shown != {'showComparison', 'showFrom'}:
                failures.append(fixture + ': missing show proof declarations')
            calculated = set()
            for definition in tree.iter('definition'):
                name = definition.find('qualified_name')
                if name is None:
                    continue
                text = ''.join(''.join(name.itertext()).split())
                if text not in {'calcInline', 'calcNested', 'calcNegative', 'afterCalc'}:
                    continue
                calculated.add(text)
                if text == 'afterCalc':
                    continue
                calc = next(definition.iter('tactic_calc'), None)
                if calc is None or len(calc.findall('calc_step')) != 2:
                    failures.append(text + ': expected two separate calculation steps')
                if text == 'calcNested':
                    body = definition.find('by')
                    if body is None or [child.tag for child in body] != ['tactic_have', 'tactic_apply']:
                        failures.append(text + ': following exact tactic escaped its proof')
            if calculated != {'calcInline', 'calcNested', 'calcNegative', 'afterCalc'}:
                failures.append(fixture + ': missing calculation proof declarations')
            typed = set()
            for definition in tree.iter('definition'):
                name = definition.find('qualified_name')
                if name is None:
                    continue
                text = ''.join(''.join(name.itertext()).split())
                if text not in {'typedProof', 'typedCaseLabel', 'typedByCases', 'typedCalc'}:
                    continue
                typed.add(text)
                term = next(definition.iter('parenthesized'), None)
                proof = term.find('by') if term is not None else None
                if proof is None or term.find("*[@field='type']") is None:
                    failures.append(text + ': missing the typed proof or its annotation')
                if text == 'typedCaseLabel' and proof is not None:
                    if proof.find("tactic_cases/identifier[@field='equation']") is None:
                        failures.append(text + ': case label escaped its tactic')
                if text == 'typedByCases' and proof is not None:
                    if proof.find("tactic_by_cases/identifier[@field='name']") is None:
                        failures.append(text + ': proposition label escaped its tactic')
            if typed != {'typedProof', 'typedCaseLabel', 'typedByCases', 'typedCalc'}:
                failures.append(fixture + ': missing typed proof declarations')
            converted = set()
            for definition in tree.iter('definition'):
                name = definition.find('qualified_name')
                if name is None:
                    continue
                text = ''.join(''.join(name.itertext()).split())
                if text not in {'castArgument', 'castSum', 'functionCoercion', 'typeCoercion',
                                'indexedClaim₁', 'afterLibrarySyntax'}:
                    continue
                converted.add(text)
                if text in {'indexedClaim₁', 'afterLibrarySyntax'}:
                    continue
                body = definition.find("*[@field='body']")
                selector, operator, operand = {
                    'castArgument': ("*[@field='arguments']", '↑', 'n'),
                    'castSum': ("*[@field='left']", '↑', 'n'),
                    'functionCoercion': ("*[@field='name']", '⇑', 'f'),
                    'typeCoercion': (None, '↥', 'c'),
                }[text]
                conversion = body.find(selector) if selector is not None and body is not None else body
                if conversion is None or conversion.tag != 'coercion':
                    failures.append(text + ': conversion escaped its application or arithmetic operand')
                    continue
                actual_operand = conversion.find("*[@field='operand']")
                if not ''.join(conversion.itertext()).strip().startswith(operator) or actual_operand is None or actual_operand.text != operand:
                    failures.append(text + ': conversion lost its operator or operand')
            if converted != {'castArgument', 'castSum', 'functionCoercion', 'typeCoercion',
                             'indexedClaim₁', 'afterLibrarySyntax'}:
                failures.append(fixture + ': missing subscript or conversion declarations')
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
        ('goal-marker', 'def bad (p q : Prop) : Prop := p ⊢ q'),
        ('show', 'theorem bad (p : Prop) : p := show p'),
        ('calc', 'theorem bad (a : Int) : a = a := by\n  calc\n    a = a :='),
        ('proof-type', 'theorem bad : True := by exact (by trivial : )'),
        ('conversion', 'def bad : Int := ↑'),
        ('subscript-name', 'def ₁bad : Nat := 0'),
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
